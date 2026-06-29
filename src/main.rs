use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{Router, routing::post};
use clap::Parser;
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use tailscale::{Config, Device};
use tower::util::ServiceExt;

use crate::api::LinkRepository;

mod api;
mod cli;

#[derive(Clone)]
struct AppState {
    repository: LinkRepository,
    ts_device: Arc<Device>,
}

#[derive(Clone)]
struct ClientIdentity {
    stable_id: String,
    hostname: String,
    tailnet: Option<String>,
}

impl ClientIdentity {
    // tailscale crate doesn't support deriving the user ID from the node
    // user ID would be a better approach as a single user can own multiple nodes
    // TODO: update the function to use the user ID if/when the taiscale supports the relation between users and nodes
    pub fn id(&self) -> String {
        format!("{}@{}", self.hostname, self.tailnet.as_deref().unwrap_or_default())
    }
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
    let dev = Arc::new(Device::new(&config, auth_key).await?);

    let repository = LinkRepository::new(db_pool.clone());
    let state = Arc::new(AppState {
        repository,
        ts_device: Arc::clone(&dev),
    });

    let app = Router::new()
        .route("/links", post(crate::api::create_link))
        .layer(axum::middleware::from_fn_with_state(Arc::clone(&state), crate::api::verify_tailscale_identity))
        .with_state(state);

    println!("Starting up tailscale TCP listener");
    let listener = dev
        .tcp_listen((dev.ipv4_addr().await?, args.server_port).into())
        .await?;

    let url = format!("{}", listener.local_addr());
    println!("Listening on {}", &url);

    while let Ok(stream) = listener.accept().await {
        let router = app.clone();
        let remote_addr = stream.remote_addr();
        let make_service = router.into_make_service_with_connect_info::<SocketAddr>();

        tokio::spawn(async move {
            let Ok(connection_service) = make_service.oneshot(remote_addr).await;
            let io = TokioIo::new(stream);
            let hyper_service = TowerToHyperService::new(connection_service);

            let _ = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                .serve_connection(io, hyper_service)
                .await;
        });
    }

    Ok(())
}
