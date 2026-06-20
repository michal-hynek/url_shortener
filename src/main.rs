use std::sync::Arc;

use anyhow::Result;
use axum::{Router, routing::post};
use clap::Parser;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use tailscale::{Config, Device};

use crate::api::LinkRepository;

mod api;
mod cli;

#[derive(Clone)]
struct AppState {
    repository: LinkRepository,
    ts_device: Arc<Device>,
}

fn init_db_pool(db_path: &str) -> Result<Pool<SqliteConnectionManager>> {
    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::builder()
        .max_size(10)
        .build(manager)?;

    Ok(pool)
}

fn run_db_migrations(connection: PooledConnection<SqliteConnectionManager>) -> Result<()> {
    let migrations = std::fs::read_dir("src/db/migrations")?;

    for entry in migrations {
        let path = entry?.path();

        if path.is_file() {
            let migration = std::fs::read_to_string(path)?;
            connection.execute(&migration, ())?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = crate::cli::Args::parse();

    let db_path = args.db_path;
    let db_pool = init_db_pool(&db_path)?;
    run_db_migrations(db_pool.get()?)?;

    // tailscale setup
    unsafe {
        std::env::set_var("TS_RS_EXPERIMENT", "this_is_unstable_software");
    }
    let config = Config::default_with_key_file("tsrs_keys.json").await?;
    let auth_key = std::env::var("TS_AUTHKEY").ok();
    let dev = Device::new(&config, auth_key).await?;

    let repository = LinkRepository::new(db_pool.clone());
    let state = Arc::new(AppState {
        repository,
        ts_device: Arc::new(dev),
    });

    let app = Router::new()
        .route("/links", post(crate::api::create_link))
        .with_state(state);

    println!("Starting up tailscale TCP listener");
    let listener = dev
        .tcp_listen((dev.ipv4_addr().await?, args.server_port).into())
        .await?;
    let url = format!("{}", listener.local_addr());
    println!("Listening on {}", &url);

    axum::serve(tailscale::axum::Listener::from(listener), app).await?;

    Ok(())
}
