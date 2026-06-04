use axum::Json;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateLink {
    alias: String,
    url: String,
}

pub async fn create_link(Json(payload): Json<CreateLink>) {
    todo!();
}