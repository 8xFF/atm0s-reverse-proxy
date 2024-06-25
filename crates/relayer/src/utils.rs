use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::Instant,
};

/// get home id from domain by get first subdomain
pub fn home_id_from_domain(domain: &str) -> u64 {
    let mut parts = domain.split('.');
    let mut hasher = DefaultHasher::default();
    parts.next().unwrap_or(domain).hash(&mut hasher);
    hasher.finish().into()
}

pub fn latency_to_label(pre: Instant) -> &'static str {
    let ms = pre.elapsed().as_millis();
    if ms < 10 {
        "<10ms"
    } else if ms < 50 {
        "<50ms"
    } else if ms < 100 {
        "<100ms"
    } else if ms < 500 {
        "<500ms"
    } else {
        ">500ms"
    }
}
