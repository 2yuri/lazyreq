use std::fs::File;
use std::collections::HashMap;
use std::path::Path;
use std::hash::{DefaultHasher, Hash, Hasher};

pub struct Cache {
    file: File,
    pub data: HashMap<String, CacheData>,
}

pub struct CacheData {
    pub data: String,
    pub expire: u64,
}

fn calculate_cache_name(filename: &str, req_id: &str) -> String {
        let mut hasher = DefaultHasher::new();
        filename.hash(&mut hasher);
        req_id.hash(&mut hasher);

        return format!("{:x}", hasher.finish());
    }

    fn find_file(filename: &str, req_id: &str) -> File {
        let cache_name = calculate_cache_name(filename, req_id);

        let path: String = "~/.lazyreq/cache/".to_string() + &cache_name;

        if Path::new(&path).exists() {
            return File::open(&path).unwrap();
        }

        return File::create_new(&path).unwrap();
    }

impl Cache {
    pub fn new(filename: &str, req_id: &str) -> Cache {
        let f = find_file(filename, req_id);
        let data = HashMap::new();
        Cache { file: f, data }
    }

    pub fn set(&mut self, key: &str, value: String) {
        let on_cache = self.data.get(key);
        let current_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if on_cache.is_some() && on_cache.unwrap().expire > {
            panic!("expired");
        }
        if !self.data.contains_key(key) {
            self.data.insert(key.to_string(), value.to_string());
        }

        self.data.insert(key.to_string(), value.to_string());
    }
}


