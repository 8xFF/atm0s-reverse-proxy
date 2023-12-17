use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signer, Verifier};
use rand::rngs::OsRng;

use crate::rpc::RegisterRequest;

pub struct LocalKey {
    sign_key: SigningKey,
}

impl LocalKey {
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
            .ok()
            .expect("Should ok");
        (&key).to_string()
    }

    pub fn to_request(&self) -> RegisterRequest {
        let msg = self.sign_key.verifying_key().to_bytes();
        let signature = self.sign_key.sign(&msg);

        RegisterRequest {
            pub_key: self.sign_key.verifying_key(),
            signature,
        }
    }
}

pub fn validate_request(req: &RegisterRequest) -> Option<String> {
    let msg = req.pub_key.to_bytes();
    req.pub_key.verify(&msg, &req.signature).ok()?;
    //convert pub_key to hex url
    Some(convert_hex(&req.pub_key.to_bytes()[0..16])) //TODO find better way to convert without loss data
}

fn convert_hex(buf: &[u8]) -> String {
    let mut s = String::new();
    for b in buf {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
