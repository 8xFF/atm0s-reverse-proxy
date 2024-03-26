pub trait AgentSigner<RES> {
    fn sign_connect_req(&self) -> Vec<u8>;
    fn validate_connect_res(&self, resp: &[u8]) -> Result<RES, String>;
}

pub trait ClusterValidator<REQ>: Clone {
    fn validate_connect_req(&self, req: &[u8]) -> Result<REQ, String>;
    fn generate_domain(&self, req: &REQ) -> Result<String, String>;
    fn sign_response_res(&self, m: &REQ, err: Option<String>) -> Vec<u8>;
}
