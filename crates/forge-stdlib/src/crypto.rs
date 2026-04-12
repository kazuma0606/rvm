use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256, Sha512};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgo {
    Sha256,
    Sha512,
    Blake3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPair {
    pub public_key: String,
    pub private_key: String,
}

pub fn hash(input: impl AsRef<[u8]>, algo: HashAlgo) -> String {
    let input = input.as_ref();
    match algo {
        HashAlgo::Sha256 => hex_encode(&Sha256::digest(input)),
        HashAlgo::Sha512 => hex_encode(&Sha512::digest(input)),
        HashAlgo::Blake3 => blake3::hash(input).to_hex().to_string(),
    }
}

pub fn hmac(input: impl AsRef<[u8]>, secret: impl AsRef<[u8]>, algo: HashAlgo) -> String {
    let input = input.as_ref();
    let secret = secret.as_ref();
    match algo {
        HashAlgo::Sha256 => {
            let mut mac =
                Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key size");
            mac.update(input);
            hex_encode(&mac.finalize().into_bytes())
        }
        HashAlgo::Sha512 => {
            let mut mac =
                Hmac::<Sha512>::new_from_slice(secret).expect("HMAC accepts any key size");
            mac.update(input);
            hex_encode(&mac.finalize().into_bytes())
        }
        HashAlgo::Blake3 => {
            let derived_key = blake3::hash(secret);
            blake3::keyed_hash(derived_key.as_bytes(), input)
                .to_hex()
                .to_string()
        }
    }
}

pub fn hmac_verify(
    input: impl AsRef<[u8]>,
    mac: impl AsRef<str>,
    secret: impl AsRef<[u8]>,
    algo: HashAlgo,
) -> bool {
    let input = input.as_ref();
    let secret = secret.as_ref();
    let provided = match hex_decode(mac.as_ref()) {
        Some(bytes) => bytes,
        None => return false,
    };

    match algo {
        HashAlgo::Sha256 => {
            let mut expected =
                Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key size");
            expected.update(input);
            expected.verify_slice(&provided).is_ok()
        }
        HashAlgo::Sha512 => {
            let mut expected =
                Hmac::<Sha512>::new_from_slice(secret).expect("HMAC accepts any key size");
            expected.update(input);
            expected.verify_slice(&provided).is_ok()
        }
        HashAlgo::Blake3 => {
            let derived_key = blake3::hash(secret);
            let expected = blake3::keyed_hash(derived_key.as_bytes(), input);
            constant_time_eq(expected.as_bytes(), &provided)
        }
    }
}

pub fn generate_keypair() -> Result<KeyPair, String> {
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let signing_key = SigningKey::from_bytes(&secret);
    let verifying_key = signing_key.verifying_key();
    Ok(KeyPair {
        public_key: hex_encode(verifying_key.as_bytes()),
        private_key: hex_encode(&signing_key.to_bytes()),
    })
}

pub fn sign(payload: impl AsRef<[u8]>, private_key: impl AsRef<str>) -> Result<String, String> {
    let private_key = decode_fixed::<32>(private_key.as_ref(), "private key")?;
    let signing_key = SigningKey::from_bytes(&private_key);
    let signature = signing_key.sign(payload.as_ref());
    Ok(hex_encode(&signature.to_bytes()))
}

pub fn verify(
    payload: impl AsRef<[u8]>,
    signature: impl AsRef<str>,
    public_key: impl AsRef<str>,
) -> Result<bool, String> {
    let public_key = decode_fixed::<32>(public_key.as_ref(), "public key")?;
    let signature = decode_fixed::<64>(signature.as_ref(), "signature")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|err| format!("invalid public key: {}", err))?;
    let signature = Signature::from_bytes(&signature);
    Ok(verifying_key.verify(payload.as_ref(), &signature).is_ok())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0u8;
    for (l, r) in left.iter().zip(right.iter()) {
        diff |= l ^ r;
    }
    diff == 0
}

fn hex_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(input: &str) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 {
        return None;
    }

    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut idx = 0;
    while idx < bytes.len() {
        let high = hex_value(bytes[idx])?;
        let low = hex_value(bytes[idx + 1])?;
        out.push((high << 4) | low);
        idx += 2;
    }
    Some(out)
}

fn decode_fixed<const N: usize>(input: &str, label: &str) -> Result<[u8; N], String> {
    let decoded = hex_decode(input).ok_or_else(|| format!("invalid {} hex", label))?;
    let bytes: [u8; N] = decoded
        .try_into()
        .map_err(|_| format!("invalid {} length: expected {} bytes", label, N))?;
    Ok(bytes)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
