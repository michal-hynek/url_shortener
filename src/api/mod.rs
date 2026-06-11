use anyhow::Result;
use axum::{Json, extract::State, http::StatusCode, response::{IntoResponse, Response}};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("internal")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string()),
        };

        (status, Json(serde_json::json!({ "error": body }))).into_response()
    }
}

#[derive(Deserialize, Clone)]
pub struct CreateLinkRequest {
    alias: String,
    url: String,
}

pub struct CreateLinkResponse {
    alias: String,
}

impl IntoResponse for CreateLinkResponse {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "message": format!("added alias {}", self.alias),
        });

        (
            StatusCode::OK,
            Json(body),
        ).into_response()
    }
}

#[derive(Clone)]
pub struct LinkRepository {
    connection_pool: Pool<SqliteConnectionManager>,
}

impl LinkRepository {
    pub fn new(connection_pool: Pool<SqliteConnectionManager>) -> Self {
        Self { connection_pool }
    }

    pub async fn create_link(&self, alias: String, url: String, user: String) -> Result<()> {
        let connection = self.connection_pool.get()?;
        connection.execute(
            "insert into links(alias, url, user) values(:alias, :url, :user)",
            &[(":alias", &alias), (":url", &url), (":user", &user)],
        )?;

        Ok(())
    }
}

pub async fn create_link(State(repository): State<LinkRepository>,  Json(payload): Json<CreateLinkRequest>) -> Response {
    match repository.create_link(payload.alias.clone(), payload.url, "test_user".into()).await {
        Ok(_) => CreateLinkResponse { alias: payload.alias }.into_response(),
        Err(e) => {
            eprintln!("{:?}", e);
            ApiError::Internal(e).into_response()
        }
    }
}