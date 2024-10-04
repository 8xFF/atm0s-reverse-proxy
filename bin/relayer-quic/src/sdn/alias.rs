pub struct AliasGuard {
    alias: u64,
}

impl Drop for AliasGuard {
    fn drop(&mut self) {
        todo!()
    }
}
