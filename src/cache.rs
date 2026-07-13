use crate::timest::{add_seconds, get_timestamp, is_older_than};
use std::fs::{self, File, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufRead, Seek, SeekFrom, Write};
use std::path::PathBuf;

pub struct Cache {
    file: File,
    pub data: Option<String>,
    pub expire: u64,
}

fn calculate_cache_name(filename: &str, req_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    filename.hash(&mut hasher);
    req_id.hash(&mut hasher);

    format!("{:x}", hasher.finish())
}

fn cache_dir() -> Result<PathBuf, String> {
    let dir = home::home_dir()
        .ok_or("cannot determine home directory for the cache".to_string())?
        .join(".lazyreq")
        .join("cache");

    fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create cache directory `{}`: {}", dir.display(), e))?;

    Ok(dir)
}

impl Cache {
    pub fn new(filename: &str, req_id: &str) -> Result<Cache, String> {
        let path = cache_dir()?.join(calculate_cache_name(filename, req_id));

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .map_err(|e| format!("cannot open cache file `{}`: {}", path.display(), e))?;

        // A missing or malformed cache file just means "no cached value".
        let reader = io::BufReader::new(file.try_clone().map_err(|e| e.to_string())?);
        let mut lines = reader.lines();
        let expire = lines
            .next()
            .and_then(|l| l.ok())
            .and_then(|l| l.parse::<u64>().ok());
        let data = lines.next().and_then(|l| l.ok());

        match (expire, data) {
            (Some(expire), Some(data)) => Ok(Cache {
                file,
                data: Some(data),
                expire,
            }),
            _ => Ok(Cache {
                file,
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

        let mut write = || -> io::Result<()> {
            self.file.set_len(0)?;
            self.file.seek(SeekFrom::Start(0))?;
            self.file
                .write_all(format!("{}\n{}\n", expired_at, value).as_bytes())
        };

        write().map_err(|e| format!("cannot write cache: {}", e))
    }
}
