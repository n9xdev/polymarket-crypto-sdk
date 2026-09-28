use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn l2_signature(secret: &str, timestamp: &str, method: &str, path: &str, body: &str) -> String {
    let message = format!("{timestamp}{method}{path}{body}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(message.as_bytes());
    B64.encode(mac.finalize().into_bytes())
}
