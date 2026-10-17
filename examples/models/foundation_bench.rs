//! Foundation baseline benchmark (A0).
//!
//! Builds parts with the real OCCT kernel, measures cold/incremental rebuild
//! times, the cost of a second tessellation and the mesh size, and saves the
//! documents so the same geometry can be used for screenshots and later gates.
//!
//! Run with:
//!   cargo run --release --example foundation_bench
//!
//! Optional environment variables:
//!   RCAD_BENCH_RUNS=<n>      cold rebuilds of the 8-hole plate (default 3)
//!   RCAD_BENCH_ROUNDS=0      skip the rounds on the plate
//!   RCAD_BENCH_SCALING=1     also measure 0/2/4-hole plates (one run each)
//!
//! Output is line-oriented (`metric,<name>,<value>`) so the numbers can be
//! pasted into plans/foundation-baseline.md without reformatting.

use std::path::PathBuf;
use std::time::Instant;

use rcad_doc::{EdgeSet, Feature, FeatureKind, Part, Project};
use rcad_features::{add_box, add_cylinder, add_extrude, add_fillet};
use rcad_geom::{BooleanOp, GeometryBackend, Profile2D};
use rcad_occt::OcctBackend;
use rcad_param::ShapeCache;

/// A circle approximated by a closed polygon, centred on (cx, cy).
///
/// The sketch still only produces polygons (foundation B will replace this with
/// analytic arcs); the approximation is deliberate so the baseline measures the
/// current pipeline, not a future one.
fn circle_profile(cx: f64, cy: f64, radius: f64, segments: usize) -> Profile2D {
    let outer = (0..segments)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / segments as f64;
            [cx + radius * a.cos(), cy + radius * a.sin()]
        })
        .collect();
    Profile2D {
        outer,
        holes: Vec::new(),
    }
}

fn demo_part() -> Part {
    let mut part = Part::new_solid("Demo");
    add_box(&mut part, "Base", [40.0, 30.0, 8.0]);
    let tool = add_cylinder(&mut part, "HoleTool", 5.0, 20.0);
    part.features.push(Feature::new(
        "CutHole",
        FeatureKind::Boolean {
            tool_feature: tool,
            op: BooleanOp::Subtract,
        },
    ));
    part
}

/// Positions of the eight holes of the benchmark plate.
fn hole_positions() -> [[f64; 2]; 8] {
    let mut positions = [[0.0; 2]; 8];
    for row in 0..2 {
        for col in 0..4 {
            positions[row * 4 + col] = [24.0 + col as f64 * 24.0, 24.0 + row as f64 * 32.0];
        }
    }
    positions
}

/// The medium part: rounded plate with `holes` holes cut through it.
///
/// The rounds are applied to the plain box, before the holes, so the benchmark
/// stays dominated by the boolean + naming work rather than by filleting the
/// hole rims (that pathological case is measured separately).
fn plate_part(holes: usize, with_rounds: bool) -> Part {
    let mut part = Part::new_solid("FoundationPlate");
    add_box(&mut part, "Base", [120.0, 80.0, 10.0]);
    if with_rounds {
        add_fillet(&mut part, "Rounds", 2.0, EdgeSet::All);
    }

    for (index, [x, y]) in hole_positions().into_iter().take(holes).enumerate() {
        let profile = circle_profile(x, y, 4.0, 32);
        let tool = add_extrude(
            &mut part,
            &format!("HoleTool{index}"),
            profile,
            10.0,
            None,
            None,
        );
        part.features.push(Feature::new(
            format!("Hole{index}"),
            FeatureKind::Boolean {
                tool_feature: tool,
                op: BooleanOp::Subtract,
            },
        ));
    }
    part
}

/// Display tessellation for this run: the project defaults, overridable with
/// `RCAD_BENCH_LINEAR`, `RCAD_BENCH_ANGULAR_DEG` and `RCAD_BENCH_RELATIVE=0`.
///
/// This lets A5 compare the old default (0.004 relative, 28.6°) with the new
/// one (0.002 relative, 11.5°) on the same machine.
fn bench_tessellation() -> (f64, f64, bool) {
    let default = rcad_doc::DisplayTessellation::default();
    let linear = std::env::var("RCAD_BENCH_LINEAR")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default.linear);
    let angular_deg = std::env::var("RCAD_BENCH_ANGULAR_DEG")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default.angular_deg);
    let relative = std::env::var("RCAD_BENCH_RELATIVE")
        .map(|value| value != "0")
        .unwrap_or(default.relative);
    (linear, angular_deg.to_radians(), relative)
}

struct Stats {
    label: String,
    features: usize,
    triangles: usize,
    vertices: usize,
    cold_ms: f64,
    incremental_ms: f64,
    tessellate_ms: f64,
    errors: Vec<String>,
}

fn measure(label: &str, part: &Part, runs: usize) -> Stats {
    let mut backend = OcctBackend::new();
    let (linear, angular_rad, relative) = bench_tessellation();
    backend.set_display_tessellation(linear, angular_rad, relative);
    let mut cold_total = 0.0f64;
    let mut last = None;

    for run in 0..runs {
        let mut cache = ShapeCache::new();
        let started = Instant::now();
        let result = cache.rebuild_part_incremental(part, &mut backend);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        cold_total += elapsed;
        eprintln!("[{label}] cold rebuild {run}: {elapsed:.1} ms");
        last = Some(result);
    }
    let result = last.expect("at least one run");

    // A second rebuild on the same cache should hit the shape cache.
    let mut cache = ShapeCache::new();
    let _ = cache.rebuild_part_incremental(part, &mut backend);
    let started = Instant::now();
    let _incremental = cache.rebuild_part_incremental(part, &mut backend);
    let incremental_ms = started.elapsed().as_secs_f64() * 1000.0;
    eprintln!("[{label}] incremental rebuild: {incremental_ms:.1} ms");

    // Time a second tessellation of the final body. The rebuild already
    // tessellated it for display, so this isolates tessellation + topology
    // naming from the feature operations.
    let mut tessellate_ms = 0.0;
    if let Some((_, shape)) = result.shapes.last() {
        let started = Instant::now();
        let _ = backend.tessellate(*shape);
        tessellate_ms = started.elapsed().as_secs_f64() * 1000.0;
        eprintln!("[{label}] second tessellation: {tessellate_ms:.1} ms");
    }

    let mesh = &result.display_mesh;
    Stats {
        label: label.to_string(),
        features: part.features.len(),
        triangles: mesh.triangle_count(),
        vertices: mesh.positions.len(),
        cold_ms: cold_total / runs as f64,
        incremental_ms,
        tessellate_ms,
        errors: result.errors.clone(),
    }
}

fn print_stats(stats: &Stats) {
    println!("metric,part,{}", stats.label);
    println!("metric,features,{}", stats.features);
    println!("metric,triangles,{}", stats.triangles);
    println!("metric,vertices,{}", stats.vertices);
    println!("metric,cold_rebuild_ms,{:.2}", stats.cold_ms);
    println!("metric,incremental_rebuild_ms,{:.2}", stats.incremental_ms);
    println!("metric,second_tessellation_ms,{:.2}", stats.tessellate_ms);
    if stats.errors.is_empty() {
        println!("metric,rebuild_errors,0");
    } else {
        println!("metric,rebuild_errors,{}", stats.errors.len());
        for error in &stats.errors {
            println!("error,{}", error);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has a parent")
        .to_path_buf();
    let models = root.join("examples").join("models");
    std::fs::create_dir_all(&models)?;

    let plate_runs: usize = std::env::var("RCAD_BENCH_RUNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let with_rounds = std::env::var("RCAD_BENCH_ROUNDS")
        .map(|value| value != "0")
        .unwrap_or(true);
    let scaling = std::env::var("RCAD_BENCH_SCALING")
        .map(|value| value == "1")
        .unwrap_or(false);

    let (tess_linear, tess_angular, tess_relative) = bench_tessellation();
    println!("metric,tessellation_linear,{tess_linear}");
    println!("metric,tessellation_angular_deg,{:.2}", tess_angular.to_degrees());
    println!("metric,tessellation_relative,{tess_relative}");

    let demo = demo_part();
    let plate = plate_part(8, with_rounds);
    eprintln!(
        "bench: plate features={} rounds={with_rounds} runs={plate_runs} scaling={scaling}          tessellation linear={tess_linear} angular={:.1}° relative={tess_relative}",
        plate.features.len(),
        tess_angular.to_degrees()
    );

    let demo_stats = measure("demo", &demo, 5);
    print_stats(&demo_stats);

    if scaling {
        for holes in [0usize, 2, 4] {
            let part = plate_part(holes, with_rounds);
            let stats = measure(&format!("plate-h{holes}"), &part, 1);
            print_stats(&stats);
        }
    }

    let plate_stats = measure("plate", &plate, plate_runs);
    print_stats(&plate_stats);

    let mut demo_project = Project::new("FoundationDemo");
    // Project::new starts with an empty "Part1" placeholder; the benchmark owns
    // exactly one part, so replace it rather than leaving an empty first part
    // (the app still edits the first part of the document).
    demo_project.parts = vec![demo];
    let demo_path = models.join("foundation_demo.rcad");
    demo_project.save(&demo_path)?;
    println!("saved,{}", demo_path.display());

    let mut plate_project = Project::new("FoundationPlate");
    plate_project.parts = vec![plate];
    let plate_path = models.join("foundation_plate.rcad");
    plate_project.save(&plate_path)?;
    println!("saved,{}", plate_path.display());

    Ok(())
}
