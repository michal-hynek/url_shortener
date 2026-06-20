use std::sync::Arc;

use anyhow::Result;
use axum::{Json, extract::State, http::StatusCode, response::{IntoResponse, Response}};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use thiserror::Error;

use crate::AppState;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("duplicate_alias")]
    DuplicateAlias(String),

    #[error("internal")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            ApiError::DuplicateAlias(alias) => (StatusCode::CONFLICT, format!("duplicate alias {alias}").to_string()),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string()),
        };

        (status, Json(serde_json::json!({ "error": body }))).into_response()
    }
}

impl From<r2d2::Error> for ApiError {
    fn from(value: r2d2::Error) -> Self {
        ApiError::Internal(value.into())
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(value: rusqlite::Error) -> Self {
        ApiError::Internal(value.into())
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

    pub async fn create_link(&self, alias: String, url: String, user: String) -> Result<(), ApiError> {
        let connection = self.connection_pool.get()?;

        let alias_exists: Option<String> = connection.query_one(
            "select alias from links where alias = ?",
            [&alias],
            |row| row.get(0))
            .optional()?;

        if alias_exists.is_some() {
            return Err(ApiError::DuplicateAlias(alias));
        }

        connection.execute(
            "insert into links(alias, url, user) values(:alias, :url, :user)",
            &[(":alias", &alias), (":url", &url), (":user", &user)],
        )?;

        Ok(())
    }
}

pub async fn create_link(State(app_state): State<Arc<AppState>>,  Json(payload): Json<CreateLinkRequest>) -> Response {
    match app_state.repository.create_link(payload.alias.clone(), payload.url, "test_user".into()).await {
        Ok(_) => CreateLinkResponse { alias: payload.alias }.into_response(),
        Err(e) => {
            eprintln!("{:?}", e);
            e.into_response()
        }
    }
}