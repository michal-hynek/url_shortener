use anyhow::Result;
use axum::{Router, routing::post};
use rusqlite::Connection;
use tokio::net::TcpListener;

mod api;

fn run_db_migrations() -> Result<()> {
    let db_path = "src/db/links.db";
    let connection = Connection::open(db_path)?;
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
    run_db_migrations()?;

    let app = Router::new()
        .route("/link", post(crate::api::create_link));

    let addr = "0.0.0.0:8000";
    let listener = TcpListener::bind(addr).await.unwrap();

    println!("Listening on {addr}");
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
