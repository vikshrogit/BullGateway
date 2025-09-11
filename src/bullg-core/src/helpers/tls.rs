use anyhow::{Result, Context};
use std::fs::File;
use std::io::{BufReader, Cursor, Error, ErrorKind};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{RootCertStore, ServerConfig};
use rustls_pemfile::{certs as certsfn, pkcs8_private_keys, read_one, rsa_private_keys, Item};
use std::fs;
use std::iter::Map;
use std::path::Path;
use rcgen::{BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, PKCS_RSA_SHA256};
use std::time::{Duration, SystemTime };

pub fn make_tls_config_v1(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
    // --- Load certificates ---
    let cert_file = File::open(cert_path)
        .with_context(|| format!("cannot open certificate file: {}", cert_path))?;
    let mut cert_reader = BufReader::new(cert_file);

    let certs: Vec<CertificateDer<'static>> = certsfn(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse certificates from: {}", cert_path))?;

    // --- Load private key ---
    let key_file = File::open(key_path)
        .with_context(|| format!("cannot open private key file: {}", key_path))?;
    let mut key_reader = BufReader::new(key_file);

    let key = loop {
        match read_one(&mut key_reader)
            .with_context(|| format!("failed to parse key file: {}", key_path))?
        {
            Some(Item::Pkcs8Key(k)) => break PrivateKeyDer::Pkcs8(k),
            Some(Item::Pkcs1Key(k)) => break PrivateKeyDer::Pkcs1(k),
            Some(Item::Sec1Key(k))  => break PrivateKeyDer::Sec1(k),
            Some(_) => continue, // skip unrelated PEM blocks
            None => anyhow::bail!("no keys found in {}", key_path),
        }
    };

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("invalid certificate/key pair")?;

    Ok(config)
}


pub fn make_tls_config_v2(cert_path: &str, key_path: &str, ca_path: Option<&str>) -> Result<ServerConfig> {
    // --- Load certificates ---
    let cert_file = File::open(cert_path)
        .with_context(|| format!("cannot open certificate file: {}", cert_path))?;
    let mut cert_reader = BufReader::new(cert_file);

    let certs: Vec<CertificateDer<'static>> = certsfn(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse certificates from: {}", cert_path))?;
    if certs.is_empty() {
        anyhow::bail!("no certificates found in {}", cert_path);
    }

    // --- Load private key ---
    let key_file = File::open(key_path)
        .with_context(|| format!("cannot open private key file: {}", key_path))?;
    let mut key_reader = BufReader::new(key_file);

    let key = loop {
        match read_one(&mut key_reader)
            .with_context(|| format!("failed to parse key file: {}", key_path))?
        {
            Some(Item::Pkcs8Key(k)) => break PrivateKeyDer::Pkcs8(k),
            Some(Item::Pkcs1Key(k)) => break PrivateKeyDer::Pkcs1(k),
            Some(Item::Sec1Key(k))  => break PrivateKeyDer::Sec1(k),
            Some(_) => continue, // skip unrelated PEM blocks
            None => anyhow::bail!("no keys found in {}", key_path),
        }
    };

    // --- Optional CA bundle ---
    let mut root_store = RootCertStore::empty();
    if let Some(ca_path) = ca_path {
        let ca_file = File::open(ca_path)
            .with_context(|| format!("cannot open CA bundle file: {}", ca_path))?;
        let mut ca_reader = BufReader::new(ca_file);

        let cas: Vec<CertificateDer<'static>> = certsfn(&mut ca_reader)
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("failed to parse CA bundle from: {}", ca_path))?;

        for cert in cas {
            root_store.add(cert)
                .map_err(|_| anyhow::anyhow!("invalid CA certificate in {}", ca_path))?;
        }
    }

    // --- Build server config ---
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("invalid certificate/key pair")?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(config)
}

/// Build TLS config from PEM strings (cert, key, optional CA bundle).
pub fn make_tls_config_from_pem(
    cert_pem: &str,
    key_pem: &str,
    ca_pem: Option<&str>,
) -> Result<ServerConfig, Error> {
    // --- Load server certs ---
    let certs = {
        let mut reader = Cursor::new(cert_pem.as_bytes());
        certsfn(&mut reader).collect::<Result<Vec<_>, _>>()?
    };
    if certs.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "no certificates found in cert PEM"));
    }

    // --- Load private key ---
    let key = {
        let mut reader = Cursor::new(key_pem.as_bytes());

        // Try pkcs8 first
        let mut keys = pkcs8_private_keys(&mut reader).collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() {
            // rewind and try RSA
            reader.set_position(0);
           let mut keys = rsa_private_keys(&mut reader).collect::<Result<Vec<_>, _>>()?;
        }
        if keys.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "no private keys found in key PEM"));
        }
        PrivateKeyDer::from(keys.remove(0))
    };

    // --- Optional CA root store ---
    let mut root_store = RootCertStore::empty();
    if let Some(ca_pem) = ca_pem {
        let mut reader = Cursor::new(ca_pem.as_bytes());
        let cas = certsfn(&mut reader).collect::<Result<Vec<_>, _>>()?;
        let cas = cas.into_iter().map(CertificateDer::from).collect::<Vec<_>>();
        for cert in cas {
            root_store.add(cert).map_err(|_| Error::new(ErrorKind::InvalidData, "invalid CA cert"))?;
        }
    }

    // --- Build config ---
    let mut cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("tls config error: {e}")))?;

    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(cfg)
}



// BullG Own Cert Manager to Manage certificates


/// Certificate Manager that can generate self-signed certs or CA-signed certs.
pub struct CertManager;

impl CertManager {
    /// Generate a self-signed certificate (for localhost/dev usage).
    pub fn generate_self_signed(
        dns_names: &[&str],
        days_valid: Option<u64>,
        rsa_bits: Option<u32>,
    ) -> anyhow::Result<(String, String)> {
        let sub:Vec<String> = dns_names.iter().map(|s| s.to_string()).collect();
        let mut params = CertificateParams::new(sub)?;

        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, dns_names[0]);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyEncipherment,
            rcgen::KeyUsagePurpose::DigitalSignature,
        ];
        params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
        ];

        let now = SystemTime::now();
        params.not_before = (now - Duration::from_secs(60)).into();
        let days_valid = days_valid.unwrap_or(360);
        params.not_after = (now + Duration::from_secs(24 * 60 * 60 * days_valid)).into();
        let key = KeyPair::generate_for(&PKCS_RSA_SHA256)?;
        let cert =params.self_signed(&key)?;
        let cert_pem = cert.pem();
        //let cert_pem = pem_serialized.as_bytes();
        //let cert_pem = pem::parse(&pem_serialized)?;
        let key_pem = key.serialize_pem();

        Ok((cert_pem, key_pem))
    }

    /// Generate a certificate signed by a custom CA (similar to Kubernetes cert-manager).
    pub fn generate_signed_by_ca(
        dns_names: &[&str],
        ca_cert: &Certificate,
        ca_key: &KeyPair,
    ) -> anyhow::Result<(String, String)> {
        let sub:Vec<String> = dns_names.iter().map(|s| s.to_string()).collect();
        let mut params = CertificateParams::new(sub)?;

        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, dns_names[0]);
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyEncipherment,
            rcgen::KeyUsagePurpose::DigitalSignature,
        ];
        params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
        ];

        let key_pair = KeyPair::generate_for(&PKCS_RSA_SHA256)?;

        let cert = params.self_signed(&key_pair)?;
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        Ok((cert_pem, key_pem))
    }

    // Load a CA (certificate + key) from files.
    // pub fn load_ca<P: AsRef<Path>>(
    //     cert_path: P,
    //     key_path: P,
    // ) -> Result<(Certificate, KeyPair)> {
    //     //let cert_pem = fs::read_to_string(cert_path)?;
    //     let key_pem = fs::read_to_string(key_path)?;
    // 
    //     let ca_key = KeyPair::from_pem(&key_pem)?;
    //     let cert_der = CertificateDer::from_pem_file(cert_path)?;
    //     pem::load
    //     let ca_cert = Certificate::from_params({
    //         let mut params = CertificateParams::from_ca_cert_pem(&cert_pem, ca_key.clone())?;
    //         params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    //         params
    //     })?;
    // 
    //     Ok((ca_cert, ca_key))
    // }
}



// Commented for Openssl no support if needed we will custom build for clients

// use std::sync::Arc;
// use openssl::asn1::{Asn1Integer, Asn1Time};
// use openssl::bn::BigNum;
// use openssl::error::ErrorStack;
// use openssl::hash::MessageDigest;
// use openssl::nid::Nid;
// use openssl::pkey::{PKey, Private};
// use openssl::rsa::Rsa;
// use openssl::x509::extension::{BasicConstraints, KeyUsage, SubjectAlternativeName, ExtendedKeyUsage};
// use openssl::x509::{X509NameBuilder, X509, X509Builder, X509Ref};

// use thiserror::Error as ThisError;
// use tokio::sync::RwLock;


// /// Error type for the cert manager
// #[derive(ThisError, Debug)]
// pub enum CertError {
//     #[error("OpenSSL error: {0}")]
//     OpenSsl(#[from] ErrorStack),
//
//     #[error("invalid input: {0}")]
//     InvalidInput(String),
// }
//
// /// Internal CA storage
// #[derive(Clone)]
// struct CaState {
//     cert: X509,
//     key: PKey<Private>,
// }
//
// /// Public CertManager
// #[derive(Clone)]
// pub struct CertManager {
//     inner: Arc<RwLock<Option<CaState>>>,
// }
//
// impl CertManager {
//     /// Create a new CertManager with no CA loaded (will fallback to self-signed localhost)
//     pub fn new() -> Self {
//         Self {
//             inner: Arc::new(RwLock::new(None)),
//         }
//     }
//
//     /// Load a CA from PEM-encoded certificate and private key strings.
//     /// Replaces any pre-existing CA.
//     pub async fn load_ca_from_pem(&self, ca_cert_pem: &str, ca_key_pem: &str) -> Result<(), CertError> {
//         let cert = X509::from_pem(ca_cert_pem.as_bytes())?;
//         let key = PKey::private_key_from_pem(ca_key_pem.as_bytes())?;
//
//         // Ensure cert is CA: check basic constraints
//         if !is_cert_ca(&cert) {
//             return Err(CertError::InvalidInput("provided CA certificate is not marked as CA".into()));
//         }
//
//         let mut guard = self.inner.write().await;
//         *guard = Some(CaState { cert, key });
//         Ok(())
//     }
//
//     /// Create a new CA (self-signed) and load it into the manager.
//     /// Returns (cert_pem, key_pem).
//     pub async fn create_ca(&self, common_name: &str, days_valid: i64, rsa_bits: u32) -> Result<(String, String), CertError> {
//         // Generate RSA key
//         let rsa = Rsa::generate(rsa_bits)?;
//         let pkey = PKey::from_rsa(rsa)?;
//
//         // Subject / issuer name
//         let mut name_builder = X509NameBuilder::new()?;
//         name_builder.append_entry_by_nid(Nid::COMMONNAME, common_name)?;
//         let name = name_builder.build();
//
//         // Build X509
//         let mut builder = X509Builder::new()?;
//         // serial
//         let mut serial = BigNum::new()?;
//         serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)?;
//         let serial_asn1 = Asn1Integer::from_bn(&serial)?;
//         builder.set_serial_number(&serial_asn1)?;
//
//         // validity
//         let not_before = Asn1Time::days_from_now(0)?; // now
//         let not_after = Asn1Time::days_from_now(days_valid as u32)?;
//         builder.set_not_before(&not_before)?;
//         builder.set_not_after(&not_after)?;
//
//         builder.set_subject_name(&name)?;
//         builder.set_issuer_name(&name)?;
//         builder.set_pubkey(&pkey)?;
//
//         // BasicConstraints: CA:TRUE
//         let basic_constraints = BasicConstraints::new().critical().ca().build()?;
//         builder.append_extension(basic_constraints)?;
//
//         // KeyUsage: keyCertSign, cRLSign
//         let key_usage = KeyUsage::new().critical().key_cert_sign().crl_sign().build()?;
//         builder.append_extension(key_usage)?;
//
//         // ExtendedKeyUsage maybe empty for CA
//
//         // sign
//         builder.sign(&pkey, MessageDigest::sha256())?;
//         let cert = builder.build();
//
//         // store
//         {
//             let mut guard = self.inner.write().await;
//             *guard = Some(CaState { cert: cert.clone(), key: pkey.clone() });
//         }
//
//         // Return PEMs
//         let cert_pem = cert.to_pem()?;
//         let key_pem = pkey.private_key_to_pem_pkcs8()?;
//
//         Ok((String::from_utf8(cert_pem).unwrap(), String::from_utf8(key_pem).unwrap()))
//     }
//
//     /// Generate a certificate signed by the loaded CA.
//     ///
//     /// `common_name` - subject CN.
//     /// `sans` - Subject Alternative Names (DNS or IP strings). The function will detect IP addresses.
//     /// `days_valid` - validity in days.
//     /// `rsa_bits` - key size to generate for the leaf cert (e.g., 2048).
//     ///
//     /// Returns (cert_pem, key_pem).
//     pub async fn generate_cert_signed_by_ca(
//         &self,
//         common_name: &str,
//         sans: &[&str],
//         days_valid: i64,
//         rsa_bits: u32,
//     ) -> Result<(String, String), CertError> {
//         let ca = {
//             let guard = self.inner.read().await;
//             guard.clone().ok_or_else(|| CertError::InvalidInput("no CA loaded".into()))?
//         };
//
//         // Generate leaf key
//         let rsa = Rsa::generate(rsa_bits)?;
//         let pkey = PKey::from_rsa(rsa)?;
//
//         // Subject name
//         let mut name_builder = X509NameBuilder::new()?;
//         name_builder.append_entry_by_nid(Nid::COMMONNAME, common_name)?;
//         let name = name_builder.build();
//
//         // Build certificate
//         let mut builder = X509Builder::new()?;
//
//         // serial
//         let mut serial = BigNum::new()?;
//         serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)?;
//         let serial_asn1 = Asn1Integer::from_bn(&serial)?;
//         builder.set_serial_number(&serial_asn1)?;
//
//         // validity
//         let not_before = Asn1Time::days_from_now(0)?;
//         let not_after = Asn1Time::days_from_now(days_valid as u32)?;
//         builder.set_not_before(&not_before)?;
//         builder.set_not_after(&not_after)?;
//
//         builder.set_subject_name(&name)?;
//         // issuer from CA
//         builder.set_issuer_name(ca.cert.subject_name())?;
//         builder.set_pubkey(&pkey)?;
//
//         // SubjectAltName
//         if !sans.is_empty() {
//             let mut san_builder = SubjectAlternativeName::new();
//             for s in sans {
//                 if s.parse::<std::net::IpAddr>().is_ok() {
//                     san_builder.ip(s);
//                 } else {
//                     san_builder.dns(s);
//                 }
//             }
//             let san_ext = san_builder.build(&builder.x509v3_context(Some(&ca.cert), None))?;
//             builder.append_extension(san_ext)?;
//         }
//
//         // Key usage: digitalSignature, keyEncipherment
//         let key_usage = KeyUsage::new().critical().digital_signature().key_encipherment().build()?;
//         builder.append_extension(key_usage)?;
//
//         // ExtendedKeyUsage: serverAuth, clientAuth
//         let eku = ExtendedKeyUsage::new().server_auth().client_auth().build()?;
//         builder.append_extension(eku)?;
//
//         // sign using CA key
//         builder.sign(&ca.key, MessageDigest::sha256())?;
//         let cert = builder.build();
//
//         // PEM outputs
//         let cert_pem = cert.to_pem()?;
//         let key_pem = pkey.private_key_to_pem_pkcs8()?;
//
//         Ok((String::from_utf8(cert_pem).unwrap(), String::from_utf8(key_pem).unwrap()))
//     }
//
//     /// Generate a self-signed certificate for localhost (CN=localhost, SANs: localhost, 127.0.0.1, ::1).
//     /// Returns (cert_pem, key_pem).
//     pub async fn generate_self_signed_localhost(&self, days_valid: i64, rsa_bits: u32) -> Result<(String, String), CertError> {
//         // Generate key
//         let rsa = Rsa::generate(rsa_bits)?;
//         let pkey = PKey::from_rsa(rsa)?;
//
//         // Subject name
//         let mut name_builder = X509NameBuilder::new()?;
//         name_builder.append_entry_by_nid(Nid::COMMONNAME, "localhost")?;
//         let name = name_builder.build();
//
//         // Create builder
//         let mut builder = X509Builder::new()?;
//
//         // serial
//         let mut serial = BigNum::new()?;
//         serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)?;
//         let serial_asn1 = Asn1Integer::from_bn(&serial)?;
//         builder.set_serial_number(&serial_asn1)?;
//
//         // validity
//         let not_before = Asn1Time::days_from_now(0)?;
//         let not_after = Asn1Time::days_from_now(days_valid as u32)?;
//         builder.set_not_before(&not_before)?;
//         builder.set_not_after(&not_after)?;
//
//         builder.set_subject_name(&name)?;
//         builder.set_issuer_name(&name)?; // self-signed
//         builder.set_pubkey(&pkey)?;
//
//         // SANs
//         let mut san_builder = SubjectAlternativeName::new();
//         san_builder.dns("localhost");
//         san_builder.ip("127.0.0.1");
//         san_builder.ip("::1");
//         let san_ext = san_builder.build(&builder.x509v3_context(None, None))?;
//         builder.append_extension(san_ext)?;
//
//         // BasicConstraints: CA:FALSE
//         let bc = BasicConstraints::new().critical().build()?;
//         builder.append_extension(bc)?;
//
//         // KeyUsage
//         let key_usage = KeyUsage::new().critical().digital_signature().key_encipherment().build()?;
//         builder.append_extension(key_usage)?;
//
//         // EKU serverAuth
//         let eku = ExtendedKeyUsage::new().server_auth().build()?;
//         builder.append_extension(eku)?;
//
//         // sign self
//         builder.sign(&pkey, MessageDigest::sha256())?;
//         let cert = builder.build();
//
//         let cert_pem = cert.to_pem()?;
//         let key_pem = pkey.private_key_to_pem_pkcs8()?;
//
//         Ok((String::from_utf8(cert_pem).unwrap(), String::from_utf8(key_pem).unwrap()))
//     }
//
//     /// Get CA cert PEM if loaded
//     pub async fn get_ca_cert_pem(&self) -> Option<String> {
//         let guard = self.inner.read().await;
//         guard.as_ref().map(|c| String::from_utf8_lossy(&c.cert.to_pem().unwrap()).into_owned())
//     }
//
//     /// Get CA key PEM (PKCS8) if loaded
//     pub async fn get_ca_key_pem(&self) -> Option<String> {
//         let guard = self.inner.read().await;
//         guard.as_ref().map(|c| String::from_utf8_lossy(&c.key.private_key_to_pem_pkcs8().unwrap()).into_owned())
//     }
//
//     /// Rotate CA by replacing with new CA cert/key PEMs
//     pub async fn rotate_ca(&self, new_ca_cert_pem: &str, new_ca_key_pem: &str) -> Result<(), CertError> {
//         self.load_ca_from_pem(new_ca_cert_pem, new_ca_key_pem).await
//     }
// }
//
// /// Utility: check if an X509 cert is CA by looking at BasicConstraints
// fn is_cert_ca(cert: &X509Ref) -> bool {
//     if let Ok(Some(bc)) = cert.basic_constraints() {
//         return bc.ca();
//     }
//     false
// }
