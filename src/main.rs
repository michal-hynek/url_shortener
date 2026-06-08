use anyhow::Result;
use axum::{Router, routing::post};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use tokio::net::TcpListener;

mod api;

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
    let db_path = "src/db/links.db";
    let db_pool = init_db_pool(&db_path)?;
    run_db_migrations(db_pool.get()?)?;

    let app = Router::new()
        .route("/link", post(crate::api::create_link))
        .with_state(db_pool);

    let addr = "0.0.0.0:8000";
    let listener = TcpListener::bind(addr).await.unwrap();

    println!("Listening on {addr}");
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
