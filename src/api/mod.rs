use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{Extension, Json, body::Body, extract::{ConnectInfo, State}, http::{Request, StatusCode}, middleware::Next, response::{IntoResponse, Response}};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use thiserror::Error;

use crate::{AppState, ClientIdentity};

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
            "insert into links(alias, url, owner) values(:alias, :url, :owner)",
            &[(":alias", &alias), (":url", &url), (":owner", &user)],
        )?;

        Ok(())
    }
}

pub async fn verify_tailscale_identity(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let source_ip = addr.ip();
    let ts_node = state.ts_device.peer_by_tailnet_ip(source_ip)
        .await
        .map_err(|e| {
            eprintln!("error when retrieving tailscale identify - {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let ts_node = if let Some(ts_node) = ts_node {
        ts_node
    } else {
        return Err(StatusCode::FORBIDDEN);
    };

    let client = ClientIdentity {
        hostname: ts_node.hostname,
        tailnet: ts_node.tailnet,
    };

    request.extensions_mut().insert(client);
    Ok(next.run(request).await)
}

pub async fn create_link(
    State(app_state): State<Arc<AppState>>,
    Extension(client): Extension<ClientIdentity>,
    Json(payload): Json<CreateLinkRequest>,
) -> Response {
    match app_state.repository.create_link(payload.alias.clone(), payload.url, client.id()).await {
        Ok(_) => CreateLinkResponse { alias: payload.alias }.into_response(),
        Err(e) => {
            eprintln!("{:?}", e);
            e.into_response()
        }
    }
}