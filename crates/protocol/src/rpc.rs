use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterRequest {
    pub pub_key: ed25519_dalek::VerifyingKey,
    pub signature: ed25519_dalek::Signature,
}

impl From<&RegisterRequest> for Vec<u8> {
    fn from(req: &RegisterRequest) -> Self {
        bincode::serialize(req).expect("Should ok")
    }
}

impl TryFrom<&[u8]> for RegisterRequest {
    type Error = bincode::Error;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize(buf)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterResponse {
    pub response: Result<String, String>,
}

impl From<&RegisterResponse> for Vec<u8> {
    fn from(resp: &RegisterResponse) -> Self {
        bincode::serialize(resp).expect("Should ok")
    }
}

impl TryFrom<&[u8]> for RegisterResponse {
    type Error = bincode::Error;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize(buf)
    }
}
