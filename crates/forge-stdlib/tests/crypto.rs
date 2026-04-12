use forge_stdlib::crypto::{generate_keypair, hash, hmac, hmac_verify, sign, verify, HashAlgo};

#[test]
fn test_hash_sha256_known_value() {
    assert_eq!(
        hash("hello", HashAlgo::Sha256),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn test_hash_blake3_known_value() {
    assert_eq!(
        hash("hello", HashAlgo::Blake3),
        "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f"
    );
}

#[test]
fn test_hmac_sha256_known_value() {
    assert_eq!(
        hmac("hello", "secret", HashAlgo::Sha256),
        "88aab3ede8d3adf94d26ab90d3bafd4a2083070c3bcce9c014ee04a443847c0b"
    );
}

#[test]
fn test_hmac_verify_valid() {
    let mac = hmac("payload", "secret", HashAlgo::Sha256);
    assert!(hmac_verify("payload", &mac, "secret", HashAlgo::Sha256));
}

#[test]
fn test_hmac_verify_invalid() {
    let mac = hmac("payload", "secret", HashAlgo::Sha256);
    assert!(!hmac_verify("payload!", &mac, "secret", HashAlgo::Sha256));
}

#[test]
fn test_hmac_verify_timing_safe() {
    let mac = hmac("payload", "secret", HashAlgo::Sha256);
    let upper = mac.to_uppercase();
    assert!(hmac_verify("payload", &upper, "secret", HashAlgo::Sha256));
    assert!(!hmac_verify(
        "payload",
        "deadbeef",
        "secret",
        HashAlgo::Sha256
    ));
}

#[test]
fn test_generate_keypair_returns_valid_keys() {
    let keypair = generate_keypair().expect("keypair should generate");
    assert_eq!(keypair.public_key.len(), 64);
    assert_eq!(keypair.private_key.len(), 64);
}

#[test]
fn test_sign_and_verify_roundtrip() {
    let keypair = generate_keypair().expect("keypair should generate");
    let signature = sign("payload", &keypair.private_key).expect("sign should succeed");
    let verified = verify("payload", &signature, &keypair.public_key).expect("verify should run");
    assert!(verified);
}

#[test]
fn test_verify_wrong_key_returns_false() {
    let keypair = generate_keypair().expect("keypair should generate");
    let other = generate_keypair().expect("keypair should generate");
    let signature = sign("payload", &keypair.private_key).expect("sign should succeed");
    let verified = verify("payload", &signature, &other.public_key).expect("verify should run");
    assert!(!verified);
}

#[test]
fn test_verify_tampered_payload_returns_false() {
    let keypair = generate_keypair().expect("keypair should generate");
    let signature = sign("payload", &keypair.private_key).expect("sign should succeed");
    let verified = verify("payload!", &signature, &keypair.public_key).expect("verify should run");
    assert!(!verified);
}
