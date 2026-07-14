use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::distributions::Alphanumeric;
use rand::Rng;
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use uuid::Uuid;

pub fn is_function(name: &str) -> bool {
    matches!(name, "uuid" | "fuzz_str" | "fuzz_int" | "hmac")
}

pub fn call(name: &str, args: &[String]) -> Result<String, String> {
    match name {
        "uuid" => uuid(args),
        "fuzz_str" => fuzz_str(args),
        "fuzz_int" => fuzz_int(args),
        "hmac" => hmac(args),
        _ => Err(format!("unknown function `${}()`", name)),
    }
}

fn uuid(args: &[String]) -> Result<String, String> {
    if !args.is_empty() {
        return Err(format!("$uuid() takes no arguments, got {}", args.len()));
    }
    Ok(Uuid::new_v4().to_string())
}

fn fuzz_str(args: &[String]) -> Result<String, String> {
    if args.len() != 1 {
        return Err(format!(
            "$fuzz_str(len) expects 1 argument, got {}",
            args.len()
        ));
    }
    let len: usize = args[0]
        .parse()
        .map_err(|_| format!("$fuzz_str(len): `{}` is not a valid length", args[0]))?;

    Ok(rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect())
}

fn fuzz_int(args: &[String]) -> Result<String, String> {
    if args.len() != 2 {
        return Err(format!(
            "$fuzz_int(min, max) expects 2 arguments, got {}",
            args.len()
        ));
    }
    let min: i64 = args[0]
        .parse()
        .map_err(|_| format!("$fuzz_int(min, max): `{}` is not a valid integer", args[0]))?;
    let max: i64 = args[1]
        .parse()
        .map_err(|_| format!("$fuzz_int(min, max): `{}` is not a valid integer", args[1]))?;
    if min > max {
        return Err(format!(
            "$fuzz_int(min, max): min ({}) is greater than max ({})",
            min, max
        ));
    }

    Ok(rand::thread_rng().gen_range(min..=max).to_string())
}

fn hmac(args: &[String]) -> Result<String, String> {
    if args.len() < 2 || args.len() > 4 {
        return Err(format!(
            "$hmac(data, key, algo?, encoding?) expects 2 to 4 arguments, got {}",
            args.len()
        ));
    }
    let data = args[0].as_bytes();
    let key = args[1].as_bytes();
    let algo = args.get(2).map(|s| s.as_str()).unwrap_or("sha256");
    let encoding = args.get(3).map(|s| s.as_str()).unwrap_or("base64");

    let digest: Vec<u8> = match algo {
        "sha1" => sign::<Hmac<Sha1>>(key, data)?,
        "sha256" => sign::<Hmac<Sha256>>(key, data)?,
        "sha512" => sign::<Hmac<Sha512>>(key, data)?,
        other => {
            return Err(format!(
                "$hmac(): unknown algorithm `{}` (expected sha1, sha256 or sha512)",
                other
            ))
        }
    };

    match encoding {
        "base64" => Ok(BASE64.encode(&digest)),
        "hex" => Ok(hex::encode(&digest)),
        other => Err(format!(
            "$hmac(): unknown encoding `{}` (expected base64 or hex)",
            other
        )),
    }
}

fn sign<M: Mac + hmac::digest::KeyInit>(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac = <M as Mac>::new_from_slice(key)
        .map_err(|_| "$hmac(): invalid key".to_string())?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn uuid_is_v4_shaped_and_unique() {
        let a = call("uuid", &[]).unwrap();
        let b = call("uuid", &[]).unwrap();
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
        assert_ne!(a, b);
        assert!(call("uuid", &args(&["nope"])).is_err());
    }

    #[test]
    fn fuzz_str_respects_length() {
        let s = call("fuzz_str", &args(&["12"])).unwrap();
        assert_eq!(s.len(), 12);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(call("fuzz_str", &args(&["banana"])).is_err());
    }

    #[test]
    fn fuzz_int_stays_in_range() {
        for _ in 0..50 {
            let n: i64 = call("fuzz_int", &args(&["18", "90"])).unwrap().parse().unwrap();
            assert!((18..=90).contains(&n));
        }
        assert!(call("fuzz_int", &args(&["9", "3"])).is_err());
    }

    #[test]
    fn hmac_matches_rfc_test_vector() {
        // RFC 4231-style vector: HMAC-SHA256("key", "The quick brown fox jumps over the lazy dog")
        let digest = call(
            "hmac",
            &args(&[
                "The quick brown fox jumps over the lazy dog",
                "key",
                "sha256",
                "hex",
            ]),
        )
        .unwrap();
        assert_eq!(
            digest,
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn hmac_defaults_to_sha256_base64() {
        let explicit = call("hmac", &args(&["data", "key", "sha256", "base64"])).unwrap();
        assert_eq!(call("hmac", &args(&["data", "key"])).unwrap(), explicit);
        assert!(call("hmac", &args(&["data", "key", "md5"])).is_err());
        assert!(call("hmac", &args(&["data", "key", "sha256", "morse"])).is_err());
        assert!(call("hmac", &args(&["data"])).is_err());
    }

    #[test]
    fn function_registry() {
        assert!(is_function("uuid") && is_function("hmac"));
        assert!(!is_function("body"));
        assert!(call("nope", &[]).is_err());
    }
}
