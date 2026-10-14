//! rcad-mcp - Model Context Protocol server for rcad CAD application
//!
//! This binary provides an MCP server that exposes rcad functionality to AI assistants.
//! It runs as a separate process communicating via stdio or TCP.

use clap::Parser;
use mcp_rs::{Server, ServerConfig, TransportType};
use tracing::{info, error};

#[derive(Parser, Debug)]
#[command(name = "rcad-mcp", version, about = "MCP Server for rcad CAD application")]
struct Args {
    /// Transport type: stdio (default) or tcp
    #[arg(long, default_value = "stdio")]
    transport: String,

    /// TCP host (if transport is tcp)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// TCP port (if transport is tcp, 0 = disabled)
    #[arg(long, default_value = "0")]
    port: u16,

    /// Allow write operations (create/modify)
    #[arg(long, default_value = "true")]
    allow_write: bool,

    /// Project file to operate on (optional, can be specified per-request)
    #[arg(long)]
    project: Option<std::path::PathBuf>,

    /// Request timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    info!("Starting rcad-mcp server with transport: {}", args.transport);

    // Create server config
    let transport_type = match args.transport.as_str() {
        "stdio" => TransportType::Stdio,
        "tcp" => {
            if args.port > 0 {
                TransportType::Sse
            } else {
                error!("TCP transport requires --port > 0");
                std::process::exit(1);
            }
        }
        _ => {
            error!("Unknown transport: {}", args.transport);
            std::process::exit(1);
        }
    };

    let config = ServerConfig {
        server: mcp_rs::server::config::ServerSettings {
            name: "rcad-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            transport: transport_type,
            host: args.host,
            port: args.port,
            max_connections: 10,
            timeout_ms: args.timeout * 1000,
        },
        resources: mcp_rs::server::config::ResourceSettings {
            root_path: std::path::PathBuf::from("./resources"),
            allowed_schemes: vec!["file".to_string(), "rcad".to_string()],
            max_file_size: 10 * 1024 * 1024,
            enable_templates: true,
        },
        security: mcp_rs::server::config::SecuritySettings {
            enable_auth: false,
            token_secret: None,
            rate_limit: mcp_rs::server::config::RateLimitSettings {
                requests_per_minute: 60,
                burst_size: 10,
            },
            allowed_origins: vec!["*".to_string()],
        },
        logging: mcp_rs::server::config::LoggingSettings {
            level: "info".to_string(),
            file: None,
            format: mcp_rs::server::config::LogFormat::Pretty,
        },
    };

    let mut server = Server::new(config);

    info!("rcad-mcp server initialized. Project: {:?}", args.project);
    info!("Available resources: rcad://project, rcad://parts, rcad://sketches, rcad://assemblies");
    info!("Planned tools: create_part, add_sketch, add_extrude, add_revolve, add_boolean, create_assembly, add_mate, export_step, export_stl, get_project_state, load_project, new_project");

    server.run().await?;

    Ok(())
}