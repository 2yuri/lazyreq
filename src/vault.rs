//! Encrypted-at-rest storage for everything under `~/.lazyreq` (hook cache,
//! run history). Payloads are gzipped, then sealed with XChaCha20-Poly1305.
//! The key comes from `LAZYREQ_KEY` when set, otherwise from an
//! auto-generated `~/.lazyreq/key` (created 0600 on first use).

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

const NONCE_LEN: usize = 24;

pub fn lazyreq_dir() -> Result<PathBuf, String> {
    let dir = home::home_dir()
        .ok_or("cannot determine home directory".to_string())?
        .join(".lazyreq");

    fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create directory `{}`: {}", dir.display(), e))?;

    Ok(dir)
}

/// Stable identifier for a `.lreq` file: hash of its absolute path, so two
/// projects with the same filename never share cache or history entries.
pub fn file_id(filename: &str, extra: &str) -> String {
    let absolute = fs::canonicalize(filename)
        .unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap_or_default()
                .join(filename)
        });

    let mut hasher = Sha256::new();
    hasher.update(absolute.to_string_lossy().as_bytes());
    hasher.update([0]);
    hasher.update(extra.as_bytes());
    hex::encode(&hasher.finalize()[..16])
}

/// Any key material becomes a cipher key through SHA-256, so env values,
/// generated hex files and hand-edited key files all work.
fn derive_key(material: &[u8]) -> Key {
    *Key::from_slice(&Sha256::digest(material))
}

/// The raw key material is either `LAZYREQ_KEY` or the contents of
/// `~/.lazyreq/key`.
fn key() -> Result<Key, String> {
    let material = match std::env::var("LAZYREQ_KEY") {
        Ok(v) if !v.trim().is_empty() => v.into_bytes(),
        _ => {
            let path = lazyreq_dir()?.join("key");
            match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let mut fresh = [0u8; 32];
                    rand::rngs::OsRng.fill_bytes(&mut fresh);
                    let encoded = hex::encode(fresh);
                    fs::write(&path, &encoded)
                        .map_err(|e| format!("cannot write key file `{}`: {}", path.display(), e))?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
                    }
                    encoded.into_bytes()
                }
            }
        }
    };

    Ok(derive_key(&material))
}

/// gzip + encrypt; the output is `nonce || ciphertext`.
pub fn seal(plain: &[u8]) -> Result<Vec<u8>, String> {
    seal_with(&key()?, plain)
}

fn seal_with(key: &Key, plain: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let compressed = encoder
        .write_all(plain)
        .and_then(|_| encoder.finish())
        .map_err(|e| format!("cannot compress data: {}", e))?;

    let cipher = XChaCha20Poly1305::new(key);
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let mut out = nonce.to_vec();
    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), compressed.as_ref())
        .map_err(|_| "cannot encrypt data".to_string())?;
    out.extend(sealed);
    Ok(out)
}

/// decrypt + gunzip. Any failure (wrong key, plaintext leftovers from older
/// versions, corruption) yields `None`: treat as "no data", never an error.
pub fn open(data: &[u8]) -> Option<Vec<u8>> {
    open_with(&key().ok()?, data)
}

fn open_with(key: &Key, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() <= NONCE_LEN {
        return None;
    }

    let cipher = XChaCha20Poly1305::new(key);
    let compressed = cipher
        .decrypt(XNonce::from_slice(&data[..NONCE_LEN]), &data[NONCE_LEN..])
        .ok()?;

    let mut plain = Vec::new();
    GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut plain)
        .ok()?;
    Some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = derive_key(b"test-key");
        let plain = br#"{"token": "secret", "sizes": [1, 2, 3]}"#;

        let sealed = seal_with(&key, plain).unwrap();
        assert_ne!(&sealed[NONCE_LEN..], plain.as_slice());
        assert_eq!(open_with(&key, &sealed).unwrap(), plain);
    }

    #[test]
    fn open_rejects_wrong_key() {
        let sealed = seal_with(&derive_key(b"key-a"), b"data").unwrap();
        assert!(open_with(&derive_key(b"key-b"), &sealed).is_none());
    }

    #[test]
    fn open_treats_garbage_as_empty() {
        let key = derive_key(b"key");
        assert!(open_with(&key, b"").is_none());
        assert!(open_with(&key, b"short").is_none());
        // plaintext leftovers from pre-encryption versions
        assert!(open_with(&key, b"1783987601\n{\"cached\": \"response\"}\n").is_none());
    }

    #[test]
    fn sealing_twice_differs_but_opens_the_same() {
        let key = derive_key(b"key");
        let a = seal_with(&key, b"same input").unwrap();
        let b = seal_with(&key, b"same input").unwrap();

        assert_ne!(a, b); // fresh nonce every write
        assert_eq!(open_with(&key, &a), open_with(&key, &b));
    }

    #[test]
    fn compression_shrinks_repetitive_payloads() {
        let key = derive_key(b"key");
        let plain = "{\"id\": 1, \"name\": \"aaaa\"},".repeat(500);
        let sealed = seal_with(&key, plain.as_bytes()).unwrap();
        assert!(sealed.len() < plain.len() / 10);
    }

    #[test]
    fn file_id_distinguishes_paths_and_extras() {
        let a = file_id("/tmp/project-a/api.lreq", "");
        let b = file_id("/tmp/project-b/api.lreq", "");
        let c = file_id("/tmp/project-a/api.lreq", "login");

        assert_ne!(a, b); // same filename, different project
        assert_ne!(a, c);
        assert_eq!(a, file_id("/tmp/project-a/api.lreq", ""));
        assert_eq!(a.len(), 32);
    }
}
