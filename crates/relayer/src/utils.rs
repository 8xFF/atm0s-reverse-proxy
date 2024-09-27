use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::{SystemTime, UNIX_EPOCH},
};

/// get home id from domain by get first subdomain
pub fn home_id_from_domain(domain: &str) -> u64 {
    let mut parts = domain.split('.');
    let mut hasher = DefaultHasher::default();
    parts.next().unwrap_or(domain).hash(&mut hasher);
    hasher.finish()
}

pub fn now_ms() -> u64 {
    let start = SystemTime::now();
    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}
