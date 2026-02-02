use nullnet_grpc_lib::NullnetGrpcInterface;
use nullnet_grpc_lib::nullnet_grpc::ProxyRequest;
use nullnet_liberror::{Error, ErrorHandler, Location, location};
use std::net::{IpAddr, SocketAddr};

pub struct NullnetProxy {
    /// Mapping of client IP + target service to upstream VLAN address
    // TODO: re-enable connection caching if needed
    // connections: Arc<Mutex<HashMap<ProxyRequest, SocketAddr>>>,
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
            // TODO: re-enable connection caching if needed
            // connections: Arc::new(Mutex::new(HashMap::new())),
            server,
        })
    }

    pub async fn get_or_add_upstream(&self, proxy_req: ProxyRequest) -> Option<SocketAddr> {
        // TODO: re-enable connection caching if needed
        // if let Some(upstream) = self.connections.lock().await.get(&proxy_req) {
        //     return Some(*upstream);
        // }

        println!("requesting new upstream...");

        let response = self.server.proxy(proxy_req).await.ok()?;

        let veth_ip: IpAddr = response.ip.parse().ok()?;
        let host_port = u16::try_from(response.port).ok()?;
        let upstream = SocketAddr::new(veth_ip, host_port);

        // TODO: re-enable connection caching if needed
        // self.connections.lock().await.insert(proxy_req, upstream);

        Some(upstream)
    }
}
