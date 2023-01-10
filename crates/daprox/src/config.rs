//! Configuration types.

use std::net::{IpAddr, SocketAddr};

use anyhow::Context;

/// Main server configuration.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ServerConfig {
    pub listen: SocketAddr,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        let listen = if let Ok(value) = std::env::var("DAPROX_LISTEN") {
            value.parse().with_context(|| {
                format!("Could not parse socket address in env var DAPROX_LISTEN")
            })?
        } else {
            "[::]:9627".parse().unwrap()
        };

        Ok(Self { listen })
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(("::".parse::<IpAddr>().unwrap(), 9627)),
        }
    }
}
