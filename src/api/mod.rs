use axum::{Json, extract::State};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateLink {
    alias: String,
    url: String,
}

pub async fn create_link(State(pool): State<Pool<SqliteConnectionManager>>,  Json(payload): Json<CreateLink>) {
    todo!();
}