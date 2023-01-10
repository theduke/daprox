#![feature(async_fn_in_trait)]

use std::sync::Arc;

use anyhow::bail;
use daprox_core::{ColumnNames, SqlBackend, SqlQuery};
use postgres_types::{FromSql, ToSql, Type};
use rustls::client::ServerCertVerifier;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;
use tokio_postgres::{Client, Column, Row, RowStream};
use url::Url;

pub struct PostgresProx(Arc<Mutex<State>>);

struct State {}

/// A [`ServerCertVerifier`] that accepts any certificate.
struct NoopCertVerifier;

impl ServerCertVerifier for NoopCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

#[cfg(feature = "rustls")]
async fn start_connection_rustls(uri: &str) -> Result<Client, anyhow::Error> {
    let mut config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();

    // FIXME: decide on verifier based on sslmode!
    config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoopCertVerifier));
    let tls = tokio_postgres_rustls::MakeRustlsConnect::new(config);
    let (client, connection) = tokio_postgres::connect(uri, tls).await?;

    // TODO: handle connection future better?
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::warn!("connection error: {}", e);
        }
    });
    Ok(client)
}

async fn start_connection_insecure(uri: &str) -> Result<Client, anyhow::Error> {
    let (client, connection) = tokio_postgres::connect(&uri, tokio_postgres::NoTls).await?;
    // TODO: handle connection future better?
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::warn!("connection error: {}", e);
        }
    });
    Ok(client)
}

async fn start_connection(uri: &str) -> Result<Client, anyhow::Error> {
    let url: Url = uri.parse()?;
    let ssl_mode = url
        .query_pairs()
        .find_map(
            |(name, value)| {
                if name == "sslmode" {
                    Some(value)
                } else {
                    None
                }
            },
        )
        .filter(|x| !x.trim().is_empty());

    let (try_ssl, needs_ssl) = match ssl_mode.as_deref() {
        Some("disable") => (false, false),
        Some("allow" | "prefer") => (true, false),
        Some("require" | "verifiy-ca" | "verify-full") => (true, true),
        Some("") | None => (true, false),
        Some(other) => {
            bail!("Unsupported sslmode {}", other);
        }
    };

    tracing::trace!(%uri, %try_ssl, %needs_ssl, "connecting to postgres server");

    #[cfg(feature = "rustls")]
    {
        if try_ssl {
            let uri = if ssl_mode.is_none() {
                let mut url = url.clone();
                url.query_pairs_mut().append_pair("sslmode", "require");
                url.to_string()
            } else {
                uri.to_string()
            };

            match start_connection_rustls(&uri).await {
                Ok(client) => return Ok(client),
                Err(e) => {
                    tracing::warn!("Failed to connect with rustls: {}", e);
                    if needs_ssl {
                        bail!("Failed to connect to Postgres server '{uri}': {}", e);
                    }
                }
            }
        }
    }

    #[cfg(not(feature = "rustls"))]
    {
        if needs_ssl {
            bail!("Failed to connect to Postgres server '{uri}': TLS required, but not supported in this daproxy instance");
        }
    }

    start_connection_insecure(uri).await
}

impl PostgresProx {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(State {})))
    }

    pub async fn connect(&self, uri: &str) -> Result<Client, anyhow::Error> {
        start_connection(uri).await
    }

    async fn query(&self, query: &SqlQuery) -> Result<Vec<Row>, anyhow::Error> {
        let client = self.connect(&query.db).await?;
        let rows = client.query(&query.query, &[]).await?;
        Ok(rows)
    }

    async fn query_stream(&self, query: &SqlQuery) -> Result<RowStream, anyhow::Error> {
        let client = self.connect(&query.db).await?;
        let params: Vec<&dyn ToSql> = vec![];
        let rows = client.query_raw(&query.query, params).await?;
        Ok(rows)
    }
}

impl SqlBackend for PostgresProx {
    async fn query_json_maps(
        &self,
        query: daprox_core::SqlQuery,
    ) -> Result<Vec<serde_json::Value>, anyhow::Error> {
        let client = self.connect(&query.db).await?;
        let rows = client.query(&query.query, &[]).await?;
        rows.into_iter().map(|r| row_to_json_map(&r)).collect()
    }

    async fn query_column_arrays(
        &self,
        query: daprox_core::SqlQuery,
    ) -> Result<(ColumnNames, Vec<Vec<JsonValue>>), anyhow::Error> {
        let client = self.connect(&query.db).await?;

        let rows = client.query(&query.query, &[]).await?;

        let names = if let Some(first) = rows.first() {
            first
                .columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect()
        } else {
            vec![]
        };

        let arrays = rows
            .into_iter()
            .map(|r| row_to_json_columns(&r))
            .collect::<Result<_, _>>()?;

        Ok((names, arrays))
    }
}

fn row_column_to_json(
    row: &Row,
    column: &Column,
    index: usize,
) -> Result<JsonValue, anyhow::Error> {
    let value: JsonValue = match column.type_() {
        &Type::BOOL => get_column_json_value::<bool>(row, index)?,
        &Type::INT2 => get_column_json_value::<i16>(row, index)?,
        &Type::INT4 => get_column_json_value::<i32>(row, index)?,
        &Type::INT8 => get_column_json_value::<i64>(row, index)?,
        &Type::FLOAT4 => get_column_json_value::<f32>(row, index)?,
        &Type::FLOAT8 => get_column_json_value::<f64>(row, index)?,
        &Type::CHAR => get_column_json_value::<String>(row, index)?,
        &Type::VARCHAR => get_column_json_value::<String>(row, index)?,
        &Type::TEXT => get_column_json_value::<String>(row, index)?,
        &Type::JSON => get_column_json_value::<JsonValue>(row, index)?,
        &Type::JSONB => get_column_json_value::<JsonValue>(row, index)?,
        // Arrays.
        &Type::BOOL_ARRAY => get_column_json_array_as_value::<bool>(row, index)?,
        &Type::INT2_ARRAY => get_column_json_array_as_value::<i16>(row, index)?,
        &Type::INT4_ARRAY => get_column_json_array_as_value::<i32>(row, index)?,
        &Type::INT8_ARRAY => get_column_json_array_as_value::<i64>(row, index)?,
        &Type::FLOAT4_ARRAY => get_column_json_array_as_value::<f32>(row, index)?,
        &Type::FLOAT8_ARRAY => get_column_json_array_as_value::<f64>(row, index)?,
        &Type::CHAR_ARRAY => get_column_json_array_as_value::<String>(row, index)?,
        &Type::VARCHAR_ARRAY => get_column_json_array_as_value::<String>(row, index)?,
        &Type::TEXT_ARRAY => get_column_json_array_as_value::<String>(row, index)?,
        &Type::JSON_ARRAY => get_column_json_array_as_value::<JsonValue>(row, index)?,
        &Type::JSONB_ARRAY => get_column_json_array_as_value::<JsonValue>(row, index)?,
        other => {
            bail!(
                "Could not convert column '{}' to json - unsupported column type '{}'",
                column.name(),
                other
            );
        }
    };
    Ok(value)
}

fn row_to_json_map(row: &Row) -> Result<JsonValue, anyhow::Error> {
    let mut map = serde_json::Map::new();

    for (index, col) in row.columns().iter().enumerate() {
        let name = col.name();
        let value = row_column_to_json(row, col, index)?;
        map.insert(name.to_string(), value);
    }

    Ok(JsonValue::Object(map))
}

fn row_to_json_columns(row: &Row) -> Result<Vec<JsonValue>, anyhow::Error> {
    let columns = row.columns();
    let mut vals = Vec::with_capacity(columns.len());

    for (index, col) in row.columns().iter().enumerate() {
        let value = row_column_to_json(row, col, index)?;
        vals.push(value);
    }

    Ok(vals)
}

// fn get_column_value_opt<'a, T: FromSql<'a>>(
//     row: &'a tokio_postgres::Row,
//     index: usize,
// ) -> Result<Option<T>, tokio_postgres::Error> {
//     row.try_get::<_, Option<T>>(index)
// }

// fn get_column_json_value_opt<'a, T>(
//     row: &'a tokio_postgres::Row,
//     index: usize,
// ) -> Result<Option<JsonValue>, tokio_postgres::Error>
// where
//     T: FromSql<'a>,
//     JsonValue: From<T>,
// {
//     if let Some(v) = row.try_get::<_, Option<T>>(index)? {
//         Ok(Some(v.into()))
//     } else {
//         Ok(None)
//     }
// }

fn get_column_json_value<'a, T>(
    row: &'a Row,
    index: usize,
) -> Result<JsonValue, tokio_postgres::Error>
where
    T: FromSql<'a>,
    JsonValue: From<T>,
{
    if let Some(v) = row.try_get::<_, Option<T>>(index)? {
        Ok(v.into())
    } else {
        Ok(JsonValue::Null)
    }
}

// fn get_column_json_array_opt<'a, T>(
//     row: &'a tokio_postgres::Row,
//     index: usize,
// ) -> Result<Option<Vec<JsonValue>>, tokio_postgres::Error>
// where
//     T: FromSql<'a>,
//     JsonValue: From<T>,
// {
//     let items = match row.try_get::<_, Option<Vec<Option<T>>>>(index)? {
//         Some(a) => a,
//         None => return Ok(None),
//     };

//     let json_items = items
//         .into_iter()
//         .map(|item| item.map(JsonValue::from).unwrap_or(JsonValue::Null))
//         .collect();

//     Ok(Some(json_items))
// }

fn get_column_json_array_as_value<'a, T>(
    row: &'a Row,
    index: usize,
) -> Result<JsonValue, tokio_postgres::Error>
where
    T: FromSql<'a>,
    JsonValue: From<T>,
{
    let items = match row.try_get::<_, Option<Vec<Option<T>>>>(index)? {
        Some(a) => a,
        None => return Ok(JsonValue::Null),
    };

    let json_items = items
        .into_iter()
        .map(|item| item.map(JsonValue::from).unwrap_or(JsonValue::Null))
        .collect();
    Ok(JsonValue::Array(json_items))
}
