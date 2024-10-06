use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AliasGuard {
    // internal: Arc<AliasGuardInternal>,
}

#[derive(Debug)]
struct AliasGuardInternal {
    alias: u64,
}

impl Drop for AliasGuardInternal {
    fn drop(&mut self) {}
}
