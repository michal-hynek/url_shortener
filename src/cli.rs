use clap::Parser;

/// Simple URL shortener that uses Tailscale for user authentication
#[derive(Parser, Debug)]
pub struct Args {
    #[arg(short, long, default_value = "8000")]
    pub server_port: u16,

    #[arg(short, long, default_value = "go")]
    pub ts_node_name: String,

    #[arg(short, long, default_value = "src/db/links.db")]
    pub db_path: String,
}