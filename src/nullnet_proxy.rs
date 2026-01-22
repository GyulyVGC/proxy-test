use crate::BrowserRequest;
use nullnet_grpc_lib::NullnetGrpcInterface;
use nullnet_grpc_lib::nullnet_grpc::ProxyRequest;
use nullnet_liberror::{Error, ErrorHandler, Location, location};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct NullnetProxy {
    /// Mapping of client IP + target service to upstream VLAN address
    connections: Arc<Mutex<HashMap<BrowserRequest, SocketAddr>>>,
    /// gRPC interface to Nullnet control service
    server: NullnetGrpcInterface,
}

impl NullnetProxy {
    pub async fn new() -> Result<Self, Error> {
        let host = std::env::var("CONTROL_SERVICE_ADDR").unwrap_or(String::from("0.0.0.0"));
        let port_str = std::env::var("CONTROL_SERVICE_PORT").unwrap_or(String::from("50051"));
        let port = port_str.parse::<u16>().handle_err(location!())?;

        let server = NullnetGrpcInterface::new(&host, port, false)
            .await
            .handle_err(location!())?;

        Ok(Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            server,
        })
    }

    pub async fn get_or_add_upstream(&self, browser_req: BrowserRequest) -> Option<SocketAddr> {
        if let Some(upstream) = self.connections.lock().await.get(&browser_req) {
            return Some(*upstream);
        }

        println!("requesting new upstream...");

        let proxy_request = ProxyRequest {
            service_name: browser_req.service.clone(),
        };
        let response = self.server.proxy(proxy_request).await.ok()?;

        let veth_ip: IpAddr = response.ip.parse().ok()?;
        let host_port = u16::try_from(response.port).ok()?;
        let upstream = SocketAddr::new(veth_ip, host_port);

        self.connections.lock().await.insert(browser_req, upstream);

        Some(upstream)
    }
}
