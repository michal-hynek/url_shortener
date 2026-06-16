use clap::Parser;

/// Simple URL shortener that uses Tailscale for user authentication
#[derive(Parser, Debug)]
pub struct Args {
    #[arg(short, long, default_value = "0.0.0.0:8000")]
    pub server_address: String,

    #[arg(short, long, default_value = "src/db/links.db")]
    pub db_path: String,
}