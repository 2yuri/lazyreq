use crate::timest::{add_seconds, get_timestamp, is_older_than};
use crate::vault;
use std::fs;
use std::path::PathBuf;

pub struct Cache {
    path: PathBuf,
    pub data: Option<String>,
    pub expire: u64,
}

fn cache_dir() -> Result<PathBuf, String> {
    let dir = vault::lazyreq_dir()?.join("cache");

    fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create cache directory `{}`: {}", dir.display(), e))?;

    Ok(dir)
}

impl Cache {
    pub fn new(filename: &str, req_id: &str) -> Result<Cache, String> {
        let path = cache_dir()?.join(vault::file_id(filename, req_id));

        // A missing, plaintext (pre-encryption) or malformed cache file just
        // means "no cached value".
        let entry = fs::read(&path)
            .ok()
            .and_then(|bytes| vault::open(&bytes))
            .and_then(|plain| String::from_utf8(plain).ok())
            .and_then(|text| {
                let (expire, data) = text.split_once('\n')?;
                Some((expire.parse::<u64>().ok()?, data.to_string()))
            });

        match entry {
            Some((expire, data)) => Ok(Cache {
                path,
                data: Some(data),
                expire,
            }),
            None => Ok(Cache {
                path,
                data: None,
                expire: 0,
            }),
        }
    }

    pub fn get(&mut self) -> Option<String> {
        if self.data.is_some() {
            if is_older_than(self.expire) {
                return None;
            }

            return self.data.clone();
        }

        None
    }

    pub fn set(&mut self, value: String, expire_in_seconds: u64) -> Result<(), String> {
        let expired_at = add_seconds(get_timestamp(), expire_in_seconds);

        let sealed = vault::seal(format!("{}\n{}", expired_at, value).as_bytes())?;
        fs::write(&self.path, sealed).map_err(|e| format!("cannot write cache: {}", e))
    }
}
