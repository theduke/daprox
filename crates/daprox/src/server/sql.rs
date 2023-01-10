use axum::{
    extract::{Query, State},
    response::Response,
    Json,
};

use daprox_core::SqlQuery;

use super::{AppState, HandlerError};
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(super) struct SingleQuery {
    #[serde(flatten)]
    query: SqlQuery,
    format: Option<SqlOutputFormat>,
}

/// The available output formats for SQL queries.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum SqlOutputFormat {
    /// A JSON array of objects, one per row.
    /// The object keys are the column names.
    Json,
    /// One JSON object per row, separated by newlines.
    /// The object keys are the column names.
    JsonLines,
    /// A JSON array of arrays.
    /// The inner arrays contain the column values.
    /// NOTE: The first array contains the column names.
    JsonColumns,
    /// JSON arrays for each row, separated by newlines.
    /// The arrays contain the column values.
    /// NOTE: The first line contains an array with the column names.
    JsonColumnLines,
}

impl Default for SqlOutputFormat {
    fn default() -> Self {
        Self::Json
    }
}

pub(super) async fn handler_sql_query_get(
    State(ctx): AppState,
    Query(query): Query<SingleQuery>,
) -> Result<Response, HandlerError> {
    let format = query.format.unwrap_or_default();

    ctx.query_sql(query.query, format)
        .await
        .map_err(HandlerError)
}

pub(super) async fn handler_sql_query_post(
    State(ctx): AppState,
    Json(query): Json<SingleQuery>,
) -> Result<Response, HandlerError> {
    let format = query.format.unwrap_or_default();

    ctx.query_sql(query.query, format)
        .await
        .map_err(HandlerError)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn test_postgres_uri() -> String {
        std::env::var("TEST_POSTGRES_URI").expect("env var TEST_POSTGRES_URI not set")
    }

    #[tokio::test]
    async fn test_postgres() {
        let client =
            axum_test_helper::TestClient::new(super::super::build_router(Default::default()));
        let uri = test_postgres_uri();

        let res = client
            .post("/sql/query")
            .json(&SqlQuery {
                db: uri.clone(),
                query: "SELECT 1 as v".to_string(),
                args: None,
                kw_args: None,
            })
            .send()
            .await
            .json::<Vec<serde_json::Value>>()
            .await;
        assert_eq!(res, vec![json!({"v": 1})]);
    }
}
