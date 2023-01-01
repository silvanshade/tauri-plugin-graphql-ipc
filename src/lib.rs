#[cfg(any(feature = "graphql-ide"))]
pub struct GraphQlIdeConfig {
    addr: std::net::SocketAddr,
    open: bool,
}

#[cfg(any(feature = "graphql-ide"))]
impl GraphQlIdeConfig {
    pub fn new(addr: impl Into<std::net::SocketAddr>, open: bool) -> Self {
        let addr = addr.into();
        Self { addr, open }
    }
}

mod async_graphql;
pub use crate::async_graphql::*;
