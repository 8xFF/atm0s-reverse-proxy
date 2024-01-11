use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use atm0s_sdn::NodeAliasId;

/// get home id from domain by get first subdomain
pub fn home_id_from_domain(domain: &str) -> NodeAliasId {
    let mut parts = domain.split('.');
    let mut hasher = DefaultHasher::default();
    parts.next().unwrap_or(domain).hash(&mut hasher);
    hasher.finish().into()
}
