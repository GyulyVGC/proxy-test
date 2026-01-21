mod nullnet_proxy;

use crate::nullnet_proxy::NullnetProxy;
use async_trait::async_trait;
use nullnet_liberror::{ErrorHandler, Location, location};
use pingora_core::server::Server;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::{Error, ErrorType, Result};
use pingora_proxy::{ProxyHttp, Session};
use std::fmt::Display;
use std::net::IpAddr;

const PROXY_PORT: u16 = 7777;

#[async_trait]
impl ProxyHttp for NullnetProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        let host_header = session
            .get_header("host")
            .ok_or_else(|| Error::explain(ErrorType::BindError, "No host header in request"))?;
        let host_str = host_header
            .to_str()
            .map_err(|_| Error::explain(ErrorType::BindError, "Invalid host header"))?;
        let url = host_str
            .strip_suffix(&format!(":{PROXY_PORT}"))
            .ok_or_else(|| {
                Error::explain(
                    ErrorType::BindError,
                    "Host header does not contain proxy port",
                )
            })?;
        let client_ip = session
            .client_addr()
            .ok_or_else(|| {
                Error::explain(ErrorType::BindError, "Client address not found in session")
            })?
            .as_inet()
            .ok_or_else(|| {
                Error::explain(
                    ErrorType::BindError,
                    "Client address is not an Inet address",
                )
            })?
            .ip();

        let service = url.to_string();
        let client_req = BrowserRequest { client_ip, service };
        println!("{client_req}");
        let upstream = self
            .get_or_add_upstream(client_req)
            .await
            .ok_or_else(|| Error::explain(ErrorType::BindError, "Failed to retrieve upstream"))?;
        println!("upstream: {upstream}\n");

        let peer = Box::new(HttpPeer::new(upstream, false, String::new()));
        Ok(peer)
    }
}

#[tokio::main]
async fn main() -> Result<(), nullnet_liberror::Error> {
    let proxy_address = format!("0.0.0.0:{PROXY_PORT}");
    println!("Running Nullnet proxy at {proxy_address}\n");

    // start proxy server
    let mut my_server = Server::new(None).handle_err(location!())?;
    my_server.bootstrap();

    let nullnet_proxy = NullnetProxy::new().await?;
    let mut proxy = pingora_proxy::http_proxy_service(&my_server.configuration, nullnet_proxy);
    proxy.add_tcp(&proxy_address);

    my_server.add_service(proxy);
    my_server.run_forever();
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct BrowserRequest {
    pub(crate) client_ip: IpAddr,
    pub(crate) service: String,
}

impl Display for BrowserRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> {}", self.client_ip, self.service)
    }
}
