mod nullnet_proxy;

use crate::nullnet_proxy::NullnetProxy;
use async_trait::async_trait;
use nullnet_grpc_lib::nullnet_grpc::ProxyRequest;
use nullnet_liberror::{ErrorHandler, Location, location};
use pingora_core::server::Server;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::{Error, ErrorType, Result};
use pingora_proxy::{ProxyHttp, Session};
use std::thread;

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
            .ip()
            .to_string();

        let service_name = url.to_string();
        let proxy_req = ProxyRequest {
            client_ip,
            service_name,
        };
        println!("{proxy_req:?}");
        let upstream = self
            .get_or_add_upstream(proxy_req)
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

    // run on separate thread to avoid "cannot start a runtime from within a runtime"
    let handle = thread::spawn(|| my_server.run_forever());
    handle.join().unwrap();
    Ok(())
}
