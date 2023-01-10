//! Configuration types.

use std::net::{IpAddr, SocketAddr};

/// Main server configuration.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ServerConfig {
    pub listen: SocketAddr,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(("::".parse::<IpAddr>().unwrap(), 9627)),
        }
    }
}
