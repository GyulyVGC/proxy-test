use nullnet_grpc_lib::NullnetGrpcInterface;
use nullnet_grpc_lib::nullnet_grpc::ProxyRequest;
use nullnet_liberror::{Error, ErrorHandler, Location, location};
use std::net::{IpAddr, SocketAddr};

pub struct NullnetProxy {
    /// gRPC interface to Nullnet control service
    server: NullnetGrpcInterface,
}

impl NullnetProxy {
    pub async fn new() -> Result<Self, Error> {
        let host = CONTROL_SERVICE_ADDR.to_string();
        let port = *CONTROL_SERVICE_PORT;

        let server = NullnetGrpcInterface::new(&host, port, false)
            .await
            .handle_err(location!())?;

        Ok(Self { server })
    }

    pub async fn get_or_add_upstream(&self, proxy_req: ProxyRequest) -> Option<SocketAddr> {
        println!("requesting new upstream...");

        let response = self.server.proxy(proxy_req).await.ok()?;

        let veth_ip: IpAddr = response.ip.parse().ok()?;
        let host_port = u16::try_from(response.port).ok()?;
        let upstream = SocketAddr::new(veth_ip, host_port);

        Some(upstream)
    }
}
