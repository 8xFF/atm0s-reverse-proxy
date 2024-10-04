use tokio::net::TcpStream;

use super::LocalTunnel;

impl LocalTunnel for TcpStream {}
