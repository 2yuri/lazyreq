use crate::timest::{add_seconds, get_timestamp, is_older_than};
use std::fs::{self, File, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufRead, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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

fn get_lazyreq_dir() -> PathBuf {
    home::home_dir()
        .expect("Failed to retrieve home directory")
        .join(".lazyreq")
}

fn setup_directories() -> std::io::Result<PathBuf> {
    let base_dir = get_lazyreq_dir();

    let cache_dir = base_dir.join("cache");

    println!("Base directory: {:?}", base_dir);
    println!("Cache directory: {:?}", cache_dir);

    fs::create_dir_all(&cache_dir)?;

    Ok(base_dir)
}

fn find_file(filename: &str, req_id: &str) -> (bool, File) {
    let cache_name = calculate_cache_name(filename, req_id);

    let path: String = get_lazyreq_dir().to_string_lossy().to_string() + "/cache";
    let cache_file = format!("{}/{}", path.clone(), cache_name);

    if Path::new(&cache_file).exists() {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(cache_file)
            .unwrap();

        return (false, file);
    }

    match setup_directories() {
        Ok(path) => println!("Directories are set up at {:?}", path),
        Err(e) => eprintln!("Failed to set up directories: {}", e),
    }

    return (true, File::create_new(&cache_file).unwrap());
}

impl Cache {
    pub fn new(filename: &str, req_id: &str) -> Cache {
        let (is_new, f) = find_file(filename, req_id);
        if is_new {
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

    pub fn set(&mut self, value: String, expire_in_seconds: u64) {
        let expired_at = add_seconds(get_timestamp(), expire_in_seconds);

        self.file.set_len(0).unwrap();
        self.file.seek(SeekFrom::Start(0)).unwrap();
        self.file
            .write_all(format!("{}\n{}\n", expired_at, value).as_bytes())
            .unwrap();
    }
}
