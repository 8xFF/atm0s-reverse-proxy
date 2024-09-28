use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

/// get home id from domain by get first subdomain
pub fn home_id_from_domain(domain: &str) -> u64 {
    let mut parts = domain.split('.');
    let mut hasher = DefaultHasher::default();
    parts.next().unwrap_or(domain).hash(&mut hasher);
    hasher.finish()
}
