mod sql;

use std::sync::Arc;

use anyhow::{bail, Context as _};
use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use daprox_core::{SqlBackend, SqlQuery};
use daprox_postgres::PostgresProx;
use serde_json::Value as JsonValue;

use crate::config::ServerConfig;

use self::sql::SqlOutputFormat;

#[derive(Clone, Debug)]
struct ServerState {
    config: ServerConfig,
}

impl Default for ServerState {
    fn default() -> Self {
        Self {
            config: Default::default(),
        }
    }
}

type Ctx = Arc<ServerState>;
type AppState = State<Ctx>;

impl ServerState {
    async fn query_sql(
        &self,
        query: SqlQuery,
        format: sql::SqlOutputFormat,
    ) -> Result<Response, anyhow::Error> {
        if query.db.starts_with("postgres://") {
            let b = PostgresProx::new();
            Self::query_sql_with_backend(&b, query, format).await
        } else {
            bail!("Unsupported database type {}", query.db);
        }
    }

    async fn query_sql_with_backend<B: SqlBackend>(
        backend: &B,
        query: SqlQuery,
        format: SqlOutputFormat,
    ) -> Result<Response, anyhow::Error> {
        match format {
            SqlOutputFormat::Json => {
                let items = backend.query_json_maps(query).await?;
                let data = Json(items);
                Ok(data.into_response())
            }
            SqlOutputFormat::JsonLines => {
                // TODO: stream the body!
                let items = backend.query_json_maps(query).await?;
                let mut buf = Vec::<u8>::new();

                for item in items {
                    serde_json::to_writer(&mut buf, &item)?;
                    buf.push(b'\n');
                }

                let res = Response::builder()
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(buf))
                    .unwrap();
                Ok(res.into_response())
            }
            SqlOutputFormat::JsonColumns => {
                let (_columns, items) = backend.query_column_arrays(query).await?;
                let data = Json(items);
                Ok(data.into_response())
            }
            SqlOutputFormat::JsonColumnLines => {
                // TODO: stream the body!
                let (names, items) = backend.query_column_arrays(query).await?;
                let mut buf = Vec::<u8>::new();

                serde_json::to_writer(&mut buf, &names)?;
                buf.push(b'\n');

                for item in items {
                    serde_json::to_writer(&mut buf, &item)?;
                    buf.push(b'\n');
                }

                let res = Response::builder()
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(buf))
                    .unwrap();
                Ok(res.into_response())
            }
        }
    }
}

fn build_router(ctx: Ctx) -> Router {
    Router::<Ctx>::new()
        .route(
            "/sql/query",
            get(sql::handler_sql_query_get).post(sql::handler_sql_query_post),
        )
        .with_state(ctx)
}

pub async fn start(config: ServerConfig) -> Result<(), anyhow::Error> {
    let ctx = Arc::new(ServerState { config });
    let router = build_router(ctx.clone());

    tracing::info!(listen=%ctx.config.listen, "Starting server");
    axum::Server::bind(&ctx.config.listen)
        .serve(router.into_make_service())
        .await
        .context("Server failed")?;

    Ok(())
}

pub struct HandlerError(pub anyhow::Error);

impl From<anyhow::Error> for HandlerError {
    fn from(e: anyhow::Error) -> Self {
        Self(e)
    }
}

impl axum::response::IntoResponse for HandlerError {
    fn into_response(self) -> axum::response::Response {
        let api_error = if self.0.is::<ApiError>() {
            self.0.downcast::<ApiError>().unwrap()
        } else {
            ApiError::from(self.0)
        };
        api_error.into_response()
    }
}

#[derive(serde::Serialize, PartialEq, Eq, Clone, Debug)]
struct ApiResponse<T = ()> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    errors: Option<Vec<HttpApiError>>,
}

impl<T> ApiResponse<T> {
    fn from_data(data: T) -> Self {
        Self {
            data: Some(data),
            errors: None,
        }
    }
}

impl<T> From<T> for ApiResponse<T> {
    fn from(data: T) -> Self {
        Self {
            data: Some(data),
            errors: None,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: String) -> Self {
        Self { status, message }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.status)
    }
}

impl std::error::Error for ApiError {}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = HttpApiError {
            message: self.message,
        };
        (self.status, Json(err)).into_response()
    }
}

#[derive(serde::Serialize, PartialEq, Eq, Clone, Debug)]
struct HttpApiError {
    message: String,
}

impl HttpApiError {
    fn from_anyhow(err: anyhow::Error) -> Self {
        Self {
            message: err.to_string(),
        }
    }

    fn to_json(&self) -> JsonValue {
        serde_json::to_value(self).unwrap()
    }
}
