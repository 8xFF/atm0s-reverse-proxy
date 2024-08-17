use std::{collections::HashMap, net::SocketAddr};

use super::ServiceRegistry;

pub struct SimpleServiceRegistry {
    default_tcp: SocketAddr,
    default_tls: SocketAddr,
    tcp_services: HashMap<u16, SocketAddr>,
    tls_services: HashMap<u16, SocketAddr>,
}

impl SimpleServiceRegistry {
    pub fn new(default_tcp: SocketAddr, default_tls: SocketAddr) -> Self {
        Self {
            default_tcp,
            default_tls,
            tcp_services: HashMap::new(),
            tls_services: HashMap::new(),
        }
    }

    pub fn set_tcp_service(&mut self, service: u16, dest: SocketAddr) {
        self.tcp_services.insert(service, dest);
    }

    pub fn set_tls_service(&mut self, service: u16, dest: SocketAddr) {
        self.tls_services.insert(service, dest);
    }
}

impl ServiceRegistry for SimpleServiceRegistry {
    fn dest_for(&self, tls: bool, service: Option<u16>, _domain: &str) -> Option<SocketAddr> {
        match (tls, service) {
            (false, None) => Some(self.default_tcp),
            (true, None) => Some(self.default_tls),
            (false, Some(service)) => self.tcp_services.get(&service).cloned(),
            (true, Some(service)) => self.tls_services.get(&service).cloned(),
        }
    }
}
