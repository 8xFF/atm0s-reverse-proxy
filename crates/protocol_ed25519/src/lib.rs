use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signer, Verifier};
use protocol::key::{AgentSigner, ClusterValidator};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterRequest {
    pub pub_key: ed25519_dalek::VerifyingKey,
    pub signature: ed25519_dalek::Signature,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterResponse {
    pub response: Result<String, String>,
}

pub struct AgentLocalKey {
    sign_key: SigningKey,
}

impl AgentLocalKey {
    pub fn random() -> Self {
        let mut csprng = OsRng;
        Self {
            sign_key: SigningKey::generate(&mut csprng),
        }
    }

    pub fn from_buf(buf: &[u8]) -> Option<Self> {
        let buf: &[u8; 32] = buf.try_into().ok()?;
        Some(Self {
            sign_key: SigningKey::from_bytes(buf),
        })
    }

    pub fn to_buf(&self) -> Vec<u8> {
        self.sign_key.to_bytes().to_vec()
    }

    pub fn from_pem(buf: &str) -> Option<Self> {
        let key = SigningKey::from_pkcs8_pem(buf).ok()?;
        Some(Self { sign_key: key })
    }

    pub fn to_pem(&self) -> String {
        let key = self
            .sign_key
            .to_pkcs8_pem(LineEnding::CRLF)
            .expect("Should ok");
        key.to_string()
    }
}

impl AgentSigner<RegisterResponse> for AgentLocalKey {
    fn sign_connect_req(&self) -> Vec<u8> {
        let msg = self.sign_key.verifying_key().to_bytes();
        let signature = self.sign_key.sign(&msg);

        bincode::serialize(&RegisterRequest {
            pub_key: self.sign_key.verifying_key(),
            signature,
        })
        .expect("should serialize")
        .to_vec()
    }

    fn validate_connect_res(&self, resp: &[u8]) -> Result<RegisterResponse, String> {
        bincode::deserialize(resp).map_err(|e| e.to_string())
    }
}

#[derive(Clone)]
pub struct ClusterValidatorImpl {
    root_domain: String,
}

impl ClusterValidatorImpl {
    pub fn new(root_domain: String) -> Self {
        Self { root_domain }
    }
}

impl ClusterValidator<RegisterRequest> for ClusterValidatorImpl {
    fn validate_connect_req(&self, req: &[u8]) -> Result<RegisterRequest, String> {
        let req: RegisterRequest = bincode::deserialize(req).map_err(|e| e.to_string())?;
        let msg = req.pub_key.to_bytes();
        req.pub_key
            .verify(&msg, &req.signature)
            .map_err(|e| e.to_string())?;
        Ok(req)
    }

    fn generate_domain(&self, req: &RegisterRequest) -> Result<String, String> {
        Ok(format!(
            "{}.{}",
            convert_hex(&req.pub_key.to_bytes()[0..16]),
            self.root_domain
        ))
    }

    fn sign_response_res(&self, m: &RegisterRequest, err: Option<String>) -> Vec<u8> {
        if let Some(err) = err {
            bincode::serialize(&RegisterResponse { response: Err(err) })
                .expect("should serialize")
                .to_vec()
        } else {
            bincode::serialize(&RegisterResponse {
                response: Ok(format!(
                    "{}.{}",
                    convert_hex(&m.pub_key.to_bytes()[0..16]),
                    self.root_domain
                )),
            })
            .expect("should serialize")
            .to_vec()
        }
    }
}

fn convert_hex(buf: &[u8]) -> String {
    let mut s = String::new();
    for b in buf {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
