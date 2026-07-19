use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{Extension, Json, body::Body, extract::{ConnectInfo, Path, State}, http::{Request, StatusCode}, middleware::Next, response::{IntoResponse, Response}};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{OptionalExtension, fallible_iterator::FallibleIterator};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{AppState, ClientIdentity};

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("duplicate_alias")]
    DuplicateAlias(String),

    #[error("alias_not_found")]
    AliasNotFound(String),

    #[error("internal")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            ApiError::DuplicateAlias(alias) => (StatusCode::CONFLICT, format!("duplicate alias {alias}").to_string()),
            ApiError::AliasNotFound(alias) => (StatusCode::NOT_FOUND, format!("alias {alias} not found").to_string()),
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

#[derive(Deserialize, Clone)]
pub struct UpdateLinkRequest {
    url: String,
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

#[derive(Serialize)]
pub struct Link {
    alias: String,
    url: String,
    owner: String,
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

    pub async fn get_links(&self) -> Result<Vec<Link>, ApiError> {
        let connection = self.connection_pool.get()?;

        let links = connection.prepare("select * from links")?
            .query([])?
            .map(|row| Ok(
                Link {
                    alias: row.get("alias")?,
                    url: row.get("url")?,
                    owner: row.get("owner")?,
                })
            )
            .collect::<Vec<Link>>()?;

        Ok(links)
    }

    pub async fn get_link(&self, alias: &str) -> Result<Option<Link>, ApiError> {
        let connection = self.connection_pool.get()?;
        let link = connection.query_one("select * from links where alias = :alias",
            &[(":alias", alias)],
            |row| {
                Ok(
                    Link {
                        alias: row.get("alias")?,
                        url: row.get("url")?,
                        owner: row.get("owner")?,
                    }
                )
            }).optional()?;
        
        Ok(link)
    }

    pub async fn update_link(&self, alias: &str, updated_url: &str) -> Result<(), ApiError> {
        Ok(())
    }

    pub async fn delete_link(&self, alias: &str) -> Result<(), ApiError> {
        let connection = self.connection_pool.get()?;
        let rows_updated = connection.execute("delete from links where alias = :alias", &[(":alias", alias)])?;

        if rows_updated > 0 {
            Ok(())
        } else {
            Err(ApiError::AliasNotFound(alias.to_string()))
        }
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

pub async fn get_links(State(app_state): State<Arc<AppState>>) -> Response {
    match app_state.repository.get_links().await {
        Ok(links) => (StatusCode::OK, Json(serde_json::json!(links))).into_response(),
        Err(e) => {
            eprintln!("{:?}", e);
            e.into_response()
        }
    }
}

pub async fn get_link(State(app_state): State<Arc<AppState>>, Path(alias): Path<String>) -> Response {
    match app_state.repository.get_link(&alias).await {
        Ok(link) => {
            match link {
                Some(link) => (StatusCode::OK, Json(serde_json::json!(link))).into_response(),
                None => (StatusCode::NOT_FOUND).into_response()
            }
        },
        Err(e) => {
            eprintln!("{:?}", e);
            e.into_response()
        }
    }
}

pub async fn update_link(
    State(app_state): State<Arc<AppState>>,
    Path(alias): Path<String>,
    Json(updated_link): Json<UpdateLinkRequest>,
) -> Response {
    match app_state.repository.update_link(&alias, &updated_link.url).await {
        Ok(_) => (StatusCode::OK).into_response(),
        Err(e) => {
            eprintln!("{:?}", e);
            e.into_response()
        }
    }
}

pub async fn delete_link(State(app_state): State<Arc<AppState>>, Path(alias): Path<String>) -> Response {
    match app_state.repository.delete_link(&alias).await {
        Ok(_) => (StatusCode::OK).into_response(),
        Err(e) => {
            eprintln!("{:?}", e);
            e.into_response()
        }
    }
}