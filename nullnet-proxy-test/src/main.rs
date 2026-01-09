use async_trait::async_trait;
use ipnetwork::Ipv4Network;
use pingora_core::server::Server;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::{Error, ErrorType, Result};
use pingora_proxy::{ProxyHttp, Session};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::Mutex;

pub struct NullnetProxy {
    /// Mapping of client IPs to upstream service addresses
    cs_map: Arc<Mutex<HashMap<IpAddr, SocketAddr>>>,
    /// Last registered VLAN ID
    last_registered_vlan: Arc<Mutex<u16>>,
    /// UDP socket for sending VLAN setup requests
    udp_socket: Arc<UdpSocket>,
}

impl NullnetProxy {
    pub fn new() -> Self {
        Self {
            cs_map: Arc::new(Mutex::new(HashMap::new())),
            last_registered_vlan: Arc::new(Mutex::new(100)),
            udp_socket: Arc::new(
                UdpSocket::bind("0.0.0.0:9997").expect("Failed to bind UDP socket"),
            ),
        }
    }

    pub fn get_or_add_upstream(&self, client_ip: IpAddr) -> Option<SocketAddr> {
        if let Some(upstream) = self.cs_map.lock().ok()?.get(&client_ip) {
            return Some(*upstream);
        }

        println!("Setting up new upstream for client {client_ip}...");

        let vlan_id = {
            let mut last_id = self.last_registered_vlan.lock().ok()?;
            *last_id += 1;
            *last_id
        };
        let [a, b] = vlan_id.to_be_bytes();

        // create dedicated VLAN on this machine
        let port_ip = Ipv4Addr::new(10, a, b, 2);
        let ipv4_network = Ipv4Network::new(port_ip, 24).ok()?;
        self.send_vlan_setup_request(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 130)),
            vlan_id,
            vec![ipv4_network],
        )?;

        // create dedicated VLAN on webserver and get its upstream address
        let port_ip = Ipv4Addr::new(10, a, b, 1);
        let ipv4_network = Ipv4Network::new(port_ip, 24).ok()?;
        self.send_vlan_setup_request(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 104)),
            vlan_id,
            vec![ipv4_network],
        )?;

        let upstream = SocketAddr::new(IpAddr::V4(port_ip), 3001);
        self.cs_map.lock().ok()?.insert(client_ip, upstream);

        // wait a bit for VLAN setup to complete
        std::thread::sleep(std::time::Duration::from_secs(1));

        Some(upstream)
    }

    pub fn send_vlan_setup_request(
        &self,
        to: IpAddr,
        vlan_id: u16,
        vlan_ports: Vec<Ipv4Network>,
    ) -> Option<()> {
        let ovs_vlan = OvsVlan {
            id: vlan_id,
            ports: vlan_ports,
        };
        let request_body = toml::to_string(&ovs_vlan).ok()?;
        let to = SocketAddr::new(to, 9998);
        self.udp_socket.send_to(request_body.as_bytes(), to).ok()?;
        Some(())
    }
}

#[async_trait]
impl ProxyHttp for NullnetProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
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

        let upstream = self
            .get_or_add_upstream(client_ip)
            .ok_or_else(|| Error::explain(ErrorType::BindError, "Failed to retrieve upstream"))?;
        println!("client: {client_ip}\nupstream: {upstream}\n");

        let peer = Box::new(HttpPeer::new(upstream, false, String::new()));
        Ok(peer)
    }
}

fn main() {
    let proxy_address = "0.0.0.0:7777";
    println!("Running Nullnet proxy at {proxy_address}\n");

    // start proxy server
    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();

    let mut proxy =
        pingora_proxy::http_proxy_service(&my_server.configuration, NullnetProxy::new());
    proxy.add_tcp(proxy_address);

    my_server.add_service(proxy);
    my_server.run_forever();
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct OvsVlan {
    pub id: u16,
    pub ports: Vec<Ipv4Network>,
}

#[cfg(test)]
mod tests {

    use crate::OvsVlan;
    use ipnetwork::Ipv4Network;
    use serde_test::{Configure, Token, assert_tokens};
    use std::net::Ipv4Addr;

    fn vlan_for_tests() -> OvsVlan {
        OvsVlan {
            id: 10,
            ports: vec![
                Ipv4Network::new(Ipv4Addr::new(8, 8, 8, 8), 24).unwrap(),
                Ipv4Network::new(Ipv4Addr::new(16, 16, 16, 16), 8).unwrap(),
            ],
        }
    }

    #[test]
    fn test_serialize_and_deserialize_vlan() {
        let vlan_setup_request = vlan_for_tests();

        assert_tokens(
            &vlan_setup_request.readable(),
            &[
                Token::Struct {
                    name: "OvsVlan",
                    len: 2,
                },
                Token::Str("id"),
                Token::U16(10),
                Token::Str("ports"),
                Token::Seq { len: Some(2) },
                Token::Str("8.8.8.8/24"),
                Token::Str("16.16.16.16/8"),
                Token::SeqEnd,
                Token::StructEnd,
            ],
        );
    }

    #[test]
    fn test_toml_string_vlan() {
        let vlan_setup_request = vlan_for_tests();

        assert_eq!(
            toml::to_string(&vlan_setup_request).unwrap(),
            "id = 10\n\
             ports = [\"8.8.8.8/24\", \"16.16.16.16/8\"]\n"
        );
    }
}
