use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use atm0s_sdn::NodeAliasId;

pub fn domain_hash(domain: &str) -> NodeAliasId {
    let mut hasher = DefaultHasher::default();
    domain.hash(&mut hasher);
    hasher.finish().into()
}
