// src/lib.rs
use base64::{ engine::general_purpose::URL_SAFE_NO_PAD, Engine as _ };
use regex::Regex;
use sha2::{ Sha256, Sha512, Digest };
use md5::{ Md5 };
use hmac::{ Hmac, Mac };
use rand::Rng;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

/// Remove all non-alphanumeric + spaces
pub fn remove_special_characters(text: &str) -> String {
    let re = Regex::new(r"[^a-zA-Z0-9 ]").unwrap();
    re.replace_all(text, "").to_string()
}

pub struct BullGCrypto {
    key: String,
    version: String,
}

impl BullGCrypto {
    pub fn new(key: &str, version: &str) -> Self {
        Self {
            key: key.to_string(),
            version: version.to_string(),
        }
    }

    fn generate_quad(&self, machine_id: Option<&str>, container_id: Option<&str>) -> String {
        let mut quad = String::new();
        if let Some(mid) = machine_id {
            quad.push_str(mid);
        }
        if let Some(cid) = container_id {
            quad.push_str(cid);
        }
        if !self.key.is_empty() {
            quad.push_str(&self.key.replace("-", "").replace(" ", "").replace("_", ""));
        }
        if !self.version.is_empty() {
            quad.push_str(&self.version.replace(" ", "").replace(".", ""));
        }

        let mut hasher = Sha256::new();
        hasher.update(quad.as_bytes());
        let result = hasher.finalize();

        URL_SAFE_NO_PAD.encode(result)
    }

    pub fn map_encryption_key(
        &self,
        machine_id: Option<&str>,
        container_id: Option<&str>
    ) -> String {
        self.generate_quad(machine_id, container_id)
    }

    pub fn generate_salt(len: usize) -> String {
        let mut rng = rand::rng();
        (0..len).map(|_| format!("{:x}", rng.random_range(0..16))).collect()
    }

    pub fn hash_bullg_password(password: &str, salt: Option<&str>) -> (String, String) {
        let generated_salt = Self::generate_salt(16);
        let salt_val = salt.unwrap_or(&generated_salt);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(format!("{}{}", password, salt_val).as_bytes());

        let mut sha1 = sha1::Sha1::new();
        sha1.update(&bytes);
        let md5_hash = Md5::digest(&sha1.finalize());

        let mut sha512 = Sha512::new();
        sha512.update(&md5_hash);
        let sha512_result = sha512.finalize();

        let mut sha256 = Sha256::new();
        sha256.update(&sha512_result);
        let hashed = format!("{:x}", sha256.finalize());

        (hashed, salt_val.to_string())
    }

    pub fn check_password(password: &str, salt: &str, hashed: &str) -> bool {
        let (real, _) = Self::hash_bullg_password(password, Some(salt));
        real == hashed
    }

    pub fn b64_encode_nopad(data: &str) -> String {
        URL_SAFE_NO_PAD.encode(data.as_bytes())
    }

    pub fn b64_decode_nopad(data: &str) -> Vec<u8> {
        URL_SAFE_NO_PAD.decode(data).unwrap_or_default()
    }

    pub fn key_to_salt(key: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(key.as_bytes()).unwrap();
        mac.update(&Self::b64_decode_nopad(key));
        let result = mac.finalize().into_bytes();
        remove_special_characters(&URL_SAFE_NO_PAD.encode(result))
    }

    pub fn encode_data(data: &str, key: &str) -> String {
        let salt = Self::key_to_salt(key);
        let enc = format!("{}{}{}", salt, data, salt);
        let b64 = URL_SAFE_NO_PAD.encode(enc.as_bytes());
        format!("{}{}{}", salt, b64, salt)
    }

    pub fn decode_data(data: &str, token: &str) -> String {
        let salt = Self::key_to_salt(token);
        let decoded = String::from_utf8(Self::b64_decode_nopad(data)).unwrap_or_default();
        decoded.replace(&salt, "")
    }

    pub fn encrypt_data(map: HashMap<String, String>, key: &str) -> HashMap<String, String> {
        map.into_iter()
            .map(|(k, v)| (k, Self::encode_data(&v, key)))
            .collect()
    }

    pub fn decrypt_data(map: HashMap<String, String>, key: &str) -> HashMap<String, String> {
        map.into_iter()
            .map(|(k, v)| (k, Self::decode_data(&v, key)))
            .collect()
    }

    pub fn int_to_base64(value: u128) -> String {
        URL_SAFE_NO_PAD.encode(value.to_be_bytes()).trim_end_matches('=').to_string()
    }
}
