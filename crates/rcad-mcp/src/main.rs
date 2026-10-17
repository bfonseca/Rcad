//! rcad-mcp - Model Context Protocol server for the rcad CAD application.
//!
//! Speaks the MCP stdio transport: one JSON-RPC 2.0 message per line on stdin,
//! one response per line on stdout. Diagnostics go to stderr so the protocol
//! stream stays clean.
//!
//! The crate used to depend on the abandoned mcp_rs 0.1 crate, whose API did
//! not match the code at all (the workspace did not compile). It is now
//! self-contained and implements the tools the README promised.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use rcad_doc::{FeatureKind, Part, Project};
use rcad_geom::{BooleanOp, GeometryBackend, Profile2D, ShapeId};
use rcad_occt::OcctBackend;
use serde_json::{json, Value};
use uuid::Uuid;

const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Parser, Debug)]
#[command(name = "rcad-mcp", version, about = "MCP server for the rcad CAD application")]
struct Args {
    /// Transport: only "stdio" is implemented.
    #[arg(long, default_value = "stdio")]
    transport: String,

    /// Allow tools that modify the model or write files.
    #[arg(long, default_value = "true")]
    allow_write: bool,

    /// Project file to open on startup (.rcad).
    #[arg(long)]
    project: Option<PathBuf>,

    /// Log request traffic to stderr.
    #[arg(long)]
    verbose: bool,
}

/// Server state: an in-memory project plus the OCCT kernel used to build it.
struct Server {
    project: Project,
    backend: OcctBackend,
    path: Option<PathBuf>,
    allow_write: bool,
    verbose: bool,
}

impl Server {
    fn new(allow_write: bool, verbose: bool) -> Self {
        Self {
            project: Project::default(),
            backend: OcctBackend::new(),
            path: None,
            allow_write,
            verbose,
        }
    }

    fn log(&self, message: &str) {
        if self.verbose {
            eprintln!("[rcad-mcp] {message}");
        }
    }

    fn active_part(&self) -> Result<&Part, String> {
        self.project
            .parts
            .first()
            .ok_or_else(|| "the project has no parts".to_string())
    }

    fn active_part_mut(&mut self) -> Result<&mut Part, String> {
        self.project
            .parts
            .first_mut()
            .ok_or_else(|| "the project has no parts".to_string())
    }

    /// Rebuild the active part and return the shape representing the active body.
    fn active_body(&mut self) -> Result<ShapeId, String> {
        let part = self.active_part()?.clone();
        let result = rcad_param::rebuild_part(&part, &mut self.backend);
        match result.shapes.last().map(|(_, id)| *id) {
            Some(id) => Ok(id),
            None => {
                let detail = if result.errors.is_empty() {
                    "the part contains no solid body".to_string()
                } else {
                    result.errors.join("; ")
                };
                Err(format!("nothing to export: {detail}"))
            }
        }
    }

    fn rebuild_report(&mut self) -> Result<String, String> {
        let part = self.active_part()?.clone();
        let result = rcad_param::rebuild_part(&part, &mut self.backend);
        let mut report = format!(
            "Rebuilt '{}': {} feature(s), {} body shape(s), {} triangle(s).",
            part.name,
            part.features.len(),
            result.shapes.len(),
            result.display_mesh.triangle_count()
        );
        if !result.errors.is_empty() {
            report.push_str(&format!(" Errors: {}", result.errors.join("; ")));
        }
        Ok(report)
    }

    fn project_state(&self) -> Value {
        json!({
            "name": self.project.name,
            "format_version": self.project.format_version,
            "path": self.path.as_ref().map(|p| p.display().to_string()),
            "parts": self.project.parts.iter().map(|part| json!({
                "id": part.id.to_string(),
                "name": part.name,
                "kind": format!("{:?}", part.kind),
                "features": part.features.iter().map(|feature| json!({
                    "id": feature.id.to_string(),
                    "name": feature.name,
                    "suppressed": feature.suppressed,
                    "visible": feature.visible,
                    "kind": feature_kind_name(&feature.kind),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "assemblies": self.project.assemblies.len(),
        })
    }

    fn require_write(&self) -> Result<(), (i64, String)> {
        if self.allow_write {
            Ok(())
        } else {
            Err((
                -32600,
                "the server was started with --allow-write false".to_string(),
            ))
        }
    }
    // ---------------------------------------------------------------- tools

    fn call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "tools/call requires a 'name'".to_string()))?
            .to_string();
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.log(&format!("tool {name}"));

        let outcome: Result<String, (i64, String)> = match name.as_str() {
            "get_project_state" => Ok(self.project_state().to_string()),
            "new_project" => (|| {
                self.require_write()?;
                let project_name = arg_str(&args, "name").unwrap_or_else(|| "Untitled".into());
                self.project = Project::new(project_name);
                self.path = None;
                Ok(format!(
                    "Created a new project with part '{}'.",
                    self.project.parts[0].name
                ))
            })(),
            "create_part" => (|| {
                self.require_write()?;
                let name = arg_str(&args, "name")
                    .unwrap_or_else(|| format!("Part{}", self.project.parts.len() + 1));
                let part = Part::new_solid(name);
                let id = part.id.to_string();
                self.project.parts.push(part);
                Ok(format!("Created part {id}."))
            })(),
            "save_project" => (|| {
                self.require_write()?;
                let path = PathBuf::from(required_str(&args, "path")?);
                ensure_parent(&path).map_err(tool_failure)?;
                self.project.save(&path).map_err(|e| tool_failure(e.to_string()))?;
                self.path = Some(path.clone());
                Ok(format!("Saved project to {}.", path.display()))
            })(),
            "load_project" => (|| {
                self.require_write()?;
                let path = PathBuf::from(required_str(&args, "path")?);
                let project = Project::load(&path).map_err(|e| tool_failure(e.to_string()))?;
                let summary =
                    format!("Loaded {} part(s) from {}.", project.parts.len(), path.display());
                self.project = project;
                self.path = Some(path);
                Ok(summary)
            })(),
            "add_box" => (|| {
                self.require_write()?;
                let size = arg_vec3(&args, "size", [10.0, 10.0, 10.0])?;
                let part = self.active_part_mut().map_err(tool_failure)?;
                let feature_name = arg_str(&args, "name")
                    .unwrap_or_else(|| rcad_features::unique_feature_name(part, "Box"));
                let id = rcad_features::add_box(part, &feature_name, size);
                Ok(format!(
                    "Added box '{feature_name}' ({id}) sized {} x {} x {}.",
                    size[0], size[1], size[2]
                ))
            })(),
            "add_cylinder" => (|| {
                self.require_write()?;
                let radius = required_f64(&args, "radius")?;
                let height = required_f64(&args, "height")?;
                let part = self.active_part_mut().map_err(tool_failure)?;
                let feature_name = arg_str(&args, "name")
                    .unwrap_or_else(|| rcad_features::unique_feature_name(part, "Cylinder"));
                let id = rcad_features::add_cylinder(part, &feature_name, radius, height);
                Ok(format!(
                    "Added cylinder '{feature_name}' ({id}) radius {radius}, height {height}."
                ))
            })(),
            "add_extrude" => (|| {
                self.require_write()?;
                let profile = Profile2D {
                    outer: arg_profile(&args, "profile")?,
                    holes: arg_holes(&args, "holes")?,
                };
                let height = required_f64(&args, "height")?;
                let part = self.active_part_mut().map_err(tool_failure)?;
                let feature_name = arg_str(&args, "name")
                    .unwrap_or_else(|| rcad_features::unique_feature_name(part, "Extrude"));
                let id = rcad_features::add_extrude(part, &feature_name, profile, height, None);
                Ok(format!(
                    "Added extrude '{feature_name}' ({id}) of height {height}."
                ))
            })(),
            "add_revolve" => (|| {
                self.require_write()?;
                let profile = Profile2D {
                    outer: arg_profile(&args, "profile")?,
                    holes: Vec::new(),
                };
                let angle = args
                    .get("angle_deg")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(360.0);
                let part = self.active_part_mut().map_err(tool_failure)?;
                let feature_name = arg_str(&args, "name")
                    .unwrap_or_else(|| rcad_features::unique_feature_name(part, "Revolve"));
                let id = rcad_features::add_revolve(part, &feature_name, profile, angle, None);
                Ok(format!(
                    "Added revolve '{feature_name}' ({id}) by {angle} degrees."
                ))
            })(),
            "add_boolean" => (|| {
                self.require_write()?;
                let tool = required_str(&args, "tool_feature")?;
                let tool_id = Uuid::parse_str(&tool)
                    .map_err(|_| invalid_request(format!("'{tool}' is not a valid feature id")))?;
                let op = match arg_str(&args, "op")
                    .unwrap_or_else(|| "subtract".into())
                    .as_str()
                {
                    "union" => BooleanOp::Union,
                    "subtract" | "cut" => BooleanOp::Subtract,
                    "intersect" => BooleanOp::Intersect,
                    other => {
                        return Err(invalid_request(format!(
                            "unknown boolean op '{other}' (union/subtract/intersect)"
                        )))
                    }
                };
                let part = self.active_part_mut().map_err(tool_failure)?;
                if !part.features.iter().any(|f| f.id == tool_id) {
                    return Err(invalid_request(format!("no feature with id {tool_id}")));
                }
                let feature_name = arg_str(&args, "name")
                    .unwrap_or_else(|| rcad_features::unique_feature_name(part, "Boolean"));
                let feature = rcad_doc::Feature::new(
                    &feature_name,
                    FeatureKind::Boolean {
                        tool_feature: tool_id,
                        op,
                    },
                );
                let id = feature.id;
                part.features.push(feature);
                Ok(format!("Added boolean '{feature_name}' ({id})."))
            })(),
            "rebuild" => self.rebuild_report().map_err(tool_failure),
            "export_step" => (|| {
                self.require_write()?;
                self.export(&args, ExportKind::Step)
            })(),
            "export_stl" => (|| {
                self.require_write()?;
                self.export(&args, ExportKind::Stl)
            })(),
            other => return Err((-32602, format!("unknown tool '{other}'"))),
        };

        match outcome {
            Ok(message) => Ok(text_result(message)),
            Err((code, message)) => Ok(error_result(code, &message)),
        }
    }

    fn export(&mut self, args: &Value, kind: ExportKind) -> Result<String, (i64, String)> {
        let path = PathBuf::from(required_str(args, "path")?);
        ensure_parent(&path).map_err(tool_failure)?;
        let body = self.active_body().map_err(tool_failure)?;
        let result = match kind {
            ExportKind::Step => self.backend.write_step(&[body], &path),
            ExportKind::Stl => self.backend.write_stl(&[body], &path),
        };
        result.map_err(|e| tool_failure(e.to_string()))?;
        self.log(&format!("wrote {}", path.display()));
        Ok(format!(
            "Exported {} ({} bytes).",
            path.display(),
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        ))
    }
    // ---------------------------------------------------------------- rpc

    /// Handle one JSON-RPC message. Returns None for notifications.
    fn handle(&mut self, message: &Value) -> Option<Value> {
        let Some(id) = message.get("id").cloned() else {
            // Notification: nothing to answer.
            self.log("notification received");
            return None;
        };
        let method = message
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string();
        self.log(&format!("request {method}"));

        let result: Result<Value, (i64, String)> = match method.as_str() {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {}, "resources": {} },
                "serverInfo": {
                    "name": "rcad-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.call_tool(message.get("params").unwrap_or(&Value::Null)),
            "resources/list" => Ok(json!({ "resources": resource_definitions() })),
            "resources/read" => self.read_resource(message.get("params").unwrap_or(&Value::Null)),
            other => Err((-32601, format!("method not found: {other}"))),
        };

        Some(match result {
            Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message }
            }),
        })
    }

    fn read_resource(&self, params: &Value) -> Result<Value, (i64, String)> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "resources/read requires 'uri'".to_string()))?;
        let text = match uri {
            "rcad://project" => self.project_state().to_string(),
            "rcad://project/parts" => json!({
                "parts": self.project.parts.iter().map(|p| json!({
                    "id": p.id.to_string(),
                    "name": p.name,
                    "features": p.features.len(),
                })).collect::<Vec<_>>()
            })
            .to_string(),
            "rcad://project/features" => json!({
                "features": self.project.parts.first().map(|p| p.features.iter().map(|f| json!({
                    "id": f.id.to_string(),
                    "name": f.name,
                    "kind": feature_kind_name(&f.kind),
                })).collect::<Vec<_>>()).unwrap_or_default()
            })
            .to_string(),
            other => return Err((-32602, format!("unknown resource: {other}"))),
        };
        Ok(json!({
            "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }]
        }))
    }
}

enum ExportKind {
    Step,
    Stl,
}

type ToolError = (i64, String);

fn invalid_request(message: impl Into<String>) -> ToolError {
    (-32602, message.into())
}

fn tool_failure(message: impl Into<String>) -> ToolError {
    (-32000, message.into())
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn text_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn error_result(code: i64, message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": format!("error {code}: {message}") }],
        "isError": true
    })
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn required_str(args: &Value, key: &str) -> Result<String, ToolError> {
    arg_str(args, key).ok_or_else(|| invalid_request(format!("missing string argument '{key}'")))
}

fn required_f64(args: &Value, key: &str) -> Result<f64, ToolError> {
    args.get(key)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| invalid_request(format!("missing or invalid numeric argument '{key}'")))
}

fn arg_vec3(args: &Value, key: &str, default: [f64; 3]) -> Result<[f64; 3], ToolError> {
    match args.get(key) {
        None => Ok(default),
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| invalid_request(format!("'{key}' must be an array of three numbers")))?;
            if array.len() != 3 {
                return Err(invalid_request(format!(
                    "'{key}' must contain exactly three numbers"
                )));
            }
            let mut out = [0.0; 3];
            for (i, item) in array.iter().enumerate() {
                out[i] = item
                    .as_f64()
                    .ok_or_else(|| invalid_request(format!("'{key}[{i}]' is not a number")))?;
            }
            Ok(out)
        }
    }
}

fn arg_profile(args: &Value, key: &str) -> Result<Vec<[f64; 2]>, ToolError> {
    let array = args
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid_request(format!("'{key}' must be an array of [x, y] points")))?;
    if array.len() < 3 {
        return Err(invalid_request(format!("'{key}' needs at least three points")));
    }
    let mut points = Vec::with_capacity(array.len());
    for (i, item) in array.iter().enumerate() {
        let pair = item
            .as_array()
            .ok_or_else(|| invalid_request(format!("'{key}[{i}]' is not an [x, y] pair")))?;
        if pair.len() < 2 {
            return Err(invalid_request(format!("'{key}[{i}]' is not an [x, y] pair")));
        }
        let x = pair[0]
            .as_f64()
            .ok_or_else(|| invalid_request(format!("'{key}[{i}][0]' is not a number")))?;
        let y = pair[1]
            .as_f64()
            .ok_or_else(|| invalid_request(format!("'{key}[{i}][1]' is not a number")))?;
        points.push([x, y]);
    }
    Ok(points)
}

fn arg_holes(args: &Value, key: &str) -> Result<Vec<Vec<[f64; 2]>>, ToolError> {
    match args.get(key) {
        None => Ok(Vec::new()),
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| invalid_request(format!("'{key}' must be an array of loops")))?;
            let mut holes = Vec::with_capacity(array.len());
            for (i, loop_value) in array.iter().enumerate() {
                let wrapper = json!({ "loop": loop_value });
                holes.push(arg_profile(&wrapper, "loop").map_err(|(code, message)| {
                    (code, format!("{key}[{i}]: {message}"))
                })?);
            }
            Ok(holes)
        }
    }
}
fn feature_kind_name(kind: &FeatureKind) -> &'static str {
    match kind {
        FeatureKind::Box { .. } => "Box",
        FeatureKind::Cylinder { .. } => "Cylinder",
        FeatureKind::Extrude { .. } => "Extrude",
        FeatureKind::Revolve { .. } => "Revolve",
        FeatureKind::Boolean { .. } => "Boolean",
        FeatureKind::Translate { .. } => "Translate",
        FeatureKind::Fillet { .. } => "Fillet",
        FeatureKind::Chamfer { .. } => "Chamfer",
        FeatureKind::Construction { .. } => "Construction",
        FeatureKind::ExtrudeSurface { .. } => "ExtrudeSurface",
        FeatureKind::Sew { .. } => "Sew",
        FeatureKind::Loft { .. } => "Loft",
        FeatureKind::Boundary { .. } => "Boundary",
        FeatureKind::Thicken { .. } => "Thicken",
        FeatureKind::Sketch { .. } => "Sketch",
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_project_state",
            "description": "Return the full project tree: parts, features and ids.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }),
        json!({
            "name": "new_project",
            "description": "Discard the current project and start a new one.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": { "type": "string" } }
            }
        }),
        json!({
            "name": "create_part",
            "description": "Add a new empty solid part to the project.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": { "type": "string" } }
            }
        }),
        json!({
            "name": "save_project",
            "description": "Save the project to a .rcad file.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
        json!({
            "name": "load_project",
            "description": "Load a .rcad project file, replacing the current project.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
        json!({
            "name": "add_box",
            "description": "Append a box primitive to the active part.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "size": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }
                }
            }
        }),
        json!({
            "name": "add_cylinder",
            "description": "Append a cylinder primitive to the active part.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "radius": { "type": "number" },
                    "height": { "type": "number" }
                },
                "required": ["radius", "height"]
            }
        }),
        json!({
            "name": "add_extrude",
            "description": "Extrude a closed 2D profile (CCW outer loop, optional holes).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "height": { "type": "number" },
                    "profile": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } },
                    "holes": { "type": "array", "items": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } } }
                },
                "required": ["height", "profile"]
            }
        }),
        json!({
            "name": "add_revolve",
            "description": "Revolve a closed 2D profile about the Z axis.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "angle_deg": { "type": "number" },
                    "profile": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } }
                },
                "required": ["profile"]
            }
        }),
        json!({
            "name": "add_boolean",
            "description": "Combine the active body with another feature (union, subtract or intersect).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "tool_feature": { "type": "string", "description": "Feature id of the tool body." },
                    "op": { "type": "string", "enum": ["union", "subtract", "intersect"] }
                },
                "required": ["tool_feature", "op"]
            }
        }),
        json!({
            "name": "rebuild",
            "description": "Regenerate the model and report shape and triangle counts plus any errors.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "export_step",
            "description": "Write the active body to a STEP file.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
        json!({
            "name": "export_stl",
            "description": "Write the active body to an STL file.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
    ]
}

fn resource_definitions() -> Vec<Value> {
    vec![
        json!({
            "uri": "rcad://project",
            "name": "Project",
            "description": "Full project state as JSON.",
            "mimeType": "application/json"
        }),
        json!({
            "uri": "rcad://project/parts",
            "name": "Parts",
            "description": "Parts and their feature counts.",
            "mimeType": "application/json"
        }),
        json!({
            "uri": "rcad://project/features",
            "name": "Features",
            "description": "Feature tree of the active part.",
            "mimeType": "application/json"
        }),
    ]
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.transport != "stdio" {
        anyhow::bail!(
            "unsupported transport '{}': only 'stdio' is implemented",
            args.transport
        );
    }

    let mut server = Server::new(args.allow_write, args.verbose);
    if let Some(project) = &args.project {
        server.project = Project::load(project)?;
        server.path = Some(project.clone());
        eprintln!("[rcad-mcp] loaded {}", project.display());
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(err) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("parse error: {err}") }
                });
                writeln!(out, "{response}")?;
                out.flush()?;
                continue;
            }
        };

        if let Some(response) = server.handle(&message) {
            writeln!(out, "{response}")?;
            out.flush()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server::new(true, false)
    }

    fn request(id: u64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    fn call(server: &mut Server, name: &str, arguments: Value) -> Value {
        let response = server
            .handle(&request(1, "tools/call", json!({ "name": name, "arguments": arguments })))
            .expect("tools/call returns a response");
        assert!(response.get("error").is_none(), "unexpected error: {response}");
        response["result"].clone()
    }

    #[test]
    fn initialize_handshake() {
        let mut server = server();
        let response = server
            .handle(&request(1, "initialize", json!({})))
            .expect("initialize responds");
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], "rcad-mcp");
    }

    #[test]
    fn notifications_get_no_reply() {
        let mut server = server();
        let notification = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(server.handle(&notification).is_none());
    }

    #[test]
    fn unknown_method_is_an_error() {
        let mut server = server();
        let response = server.handle(&request(7, "does/not/exist", json!({}))).unwrap();
        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(response["id"], 7);
    }

    #[test]
    fn tools_list_covers_the_documented_tools() {
        let mut server = server();
        let response = server.handle(&request(1, "tools/list", json!({}))).unwrap();
        let names: Vec<String> = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "get_project_state",
            "add_box",
            "add_extrude",
            "add_revolve",
            "add_boolean",
            "export_step",
            "export_stl",
            "rebuild",
        ] {
            assert!(names.contains(&expected.to_string()), "missing tool {expected}");
        }
    }

    #[test]
    fn add_box_then_rebuild_reports_a_body() {
        let mut server = server();
        call(&mut server, "add_box", json!({ "size": [20.0, 10.0, 5.0] }));
        let result = call(&mut server, "rebuild", json!({}));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("1 body shape"), "unexpected rebuild report: {text}");
        assert!(!text.contains("Errors:"), "unexpected rebuild errors: {text}");
    }

    #[test]
    fn export_writes_a_real_stl_file() {
        let mut server = server();
        call(&mut server, "add_box", json!({ "size": [10.0, 10.0, 10.0] }));
        let path = std::env::temp_dir().join("rcad_mcp_test_export.stl");
        let _ = std::fs::remove_file(&path);
        call(
            &mut server,
            "export_stl",
            json!({ "path": path.display().to_string() }),
        );
        let bytes = std::fs::read(&path).expect("STL file was written");
        assert!(bytes.len() > 100, "STL looks empty: {} bytes", bytes.len());
    }

    #[test]
    fn export_without_geometry_fails_cleanly() {
        let mut server = server();
        let path = std::env::temp_dir().join("rcad_mcp_test_empty.step");
        let result = call(
            &mut server,
            "export_step",
            json!({ "path": path.display().to_string() }),
        );
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn write_tools_are_rejected_when_disabled() {
        let mut server = Server::new(false, false);
        let result = call(&mut server, "add_box", json!({}));
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("allow-write"), "unexpected message: {text}");
    }

    #[test]
    fn invalid_arguments_are_reported() {
        let mut server = server();
        let result = call(&mut server, "add_extrude", json!({ "profile": [[0, 0], [1, 1]] }));
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn resources_round_trip() {
        let mut server = server();
        call(&mut server, "add_box", json!({}));
        let response = server
            .handle(&request(1, "resources/read", json!({ "uri": "rcad://project/features" })))
            .unwrap();
        let text = response["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("Box"), "unexpected resource: {text}");
    }

    #[test]
    fn boolean_requires_a_known_tool_feature() {
        let mut server = server();
        let result = call(
            &mut server,
            "add_boolean",
            json!({ "tool_feature": Uuid::new_v4().to_string(), "op": "subtract" }),
        );
        assert_eq!(result["isError"], true);
    }
}
