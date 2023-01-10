#![feature(async_fn_in_trait)]

use std::collections::HashMap;

use serde_json::Value as JsonValue;

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct SqlQuery {
    pub query: String,
    pub args: Option<Vec<JsonValue>>,
    pub kw_args: Option<HashMap<String, JsonValue>>,
    pub db: String,
}

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SqlOutputFormat {
    Json,
    JsonLines,
    JsonColumns,
    JsonColumnLines,
}

pub type ColumnNames = Vec<String>;

pub trait SqlBackend {
    async fn query_json_maps(&self, query: SqlQuery) -> Result<Vec<JsonValue>, anyhow::Error>;
    async fn query_column_arrays(
        &self,
        query: SqlQuery,
    ) -> Result<(ColumnNames, Vec<Vec<JsonValue>>), anyhow::Error>;
}
