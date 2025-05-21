use crate::timest::{add_minutes, add_seconds, get_timestamp, is_older_than};
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::Path;

pub struct Cache {
    file: File,
    pub data: Option<String>,
    pub expire: u64,
}

fn calculate_cache_name(filename: &str, req_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    filename.hash(&mut hasher);
    req_id.hash(&mut hasher);

    return format!("{:x}", hasher.finish());
}

fn find_file(filename: &str, req_id: &str) -> (bool, File) {
    let cache_name = calculate_cache_name(filename, req_id);

    let path: String = "./.lazyreq/cache/".to_string() + &cache_name;

    if Path::new(&path).exists() {
        return (false, File::open(&path).unwrap());
    }

    if !Path::new("./.lazyreq").exists() {
        fs::create_dir("./.lazyreq").unwrap();
    }

    if !Path::new("./.lazyreq/cache").exists() {
        fs::create_dir("./.lazyreq/cache").unwrap();
    }

    return (true, File::create_new(&path).unwrap());
}

impl Cache {
    pub fn new(filename: &str, req_id: &str) -> Cache {
        let (is_new, f) = find_file(filename, req_id);
        if is_new {
            println!("new cache");
            return Cache {
                file: f,
                data: None,
                expire: 0,
            };
        }

        let reader = io::BufReader::new(f.try_clone().unwrap());
        let mut lines = reader.lines();
        let first_line = lines.next().unwrap();
        let second_line = lines.next().unwrap();

        Cache {
            file: f,
            data: Some(second_line.unwrap().to_string()),
            expire: first_line.unwrap().to_string().parse::<u64>().unwrap(),
        }
    }

    pub fn get(&mut self) -> Option<String> {
        if self.data.is_some() {
            if is_older_than(self.expire) {
                return None;
            }

            return self.data.clone();
        }

        return None;
    }

    pub fn set(&mut self, value: String, expire_in_minutes: u64) {
        let expired_at = add_seconds(get_timestamp(), expire_in_minutes);

        self.file.write_all(format!("{}\n", expired_at).as_bytes());
        self.file.write_all(format!("{}\n", value).as_bytes());
    }
}
