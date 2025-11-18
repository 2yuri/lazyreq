use async_recursion::async_recursion;
use colored::*;
use core::panic;
use mime_guess::from_path;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::multipart::{self, Part};
use reqwest::{Client, Url};
use serde_json::to_string_pretty;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::path::Path;
use std::{env, fs};

use crate::cache::Cache;
use crate::request::Request;

pub struct LazyReq {
    variables: HashMap<String, String>,
    hooks: HashMap<String, String>,
    requests: HashMap<String, Request>,
    filename: String,
}

impl LazyReq {
    pub fn new() -> LazyReq {
        LazyReq {
            variables: HashMap::new(),
            hooks: HashMap::new(),
            requests: HashMap::new(),
            filename: "".to_string(),
        }
    }

    pub async fn do_request(&self, id: String) {
        match self.requests.get(&id) {
            Some(req) => {
                print!(
                    "{}{}{}",
                    "[".bold().green(),
                    req.method.clone().bold().green(),
                    "]".bold().green()
                );
                let (status, url, result) = self.execute(req).await.unwrap();
                print!(" {}\n", url.bold().green());

                println!("{} {}", "Status:".bold().green(), status.bold().green());
                let pretty_json: Value =
                    serde_json::from_str(&result.as_str()).unwrap_or(Value::Null);
                if !pretty_json.is_null() {
                    println!("{}", to_string_pretty(&pretty_json).unwrap());
                } else {
                    println!("{}", result.bold().green());
                }
            }
            None => {
                println!("Request not found");
            }
        }
    }

    #[async_recursion]
    pub async fn handle_macro(&self, macr: String, splits: Vec<&str>) -> (String, String) {
        if macr.starts_with("$req.") {
            let macro_parsed = &macr.replace("$req.", "");

            let mut cacher: Option<Cache> = None;
            if splits.len() > 1 {
                cacher = Some(Cache::new(&self.filename, macro_parsed));
                let has = cacher.as_mut().unwrap().get();
                if has.is_some() {
                    return (macro_parsed.clone(), has.unwrap());
                }
            }

            let req = self.requests.get(macro_parsed).unwrap();
            let (_, _, result) = self.execute(req).await.unwrap();
            if splits.len() > 1 && cacher.is_some() {
                cacher
                    .as_mut()
                    .unwrap()
                    .set(result.clone(), splits[1].parse::<u64>().unwrap());
            }

            return (macro_parsed.clone(), result);
        }

        return (String::new(), String::new());
    }

    #[async_recursion]
    async fn handle_variables_and_hooks(&self, data: String) -> String {
        let pattern = r"\$[\w.]+";
        let re = Regex::new(pattern).unwrap();

        let mut url = data.clone();
        for i in re.find_iter(&data) {
            if i.as_str().starts_with("$$") {
                continue;
            }

            let replace_value = i.clone().as_str();
            let mut item: String = i.as_str().chars().skip(1).collect();

            item = item.split(".").collect::<Vec<&str>>()[0].to_string();

            let is_hook = self.hooks.get(item.as_str().trim());
            if is_hook.is_some() {
                let hook = is_hook.unwrap().to_string();
                let parts: Vec<&str> = hook.split(" ").collect::<Vec<&str>>();

                let (macro_name, macro_result) =
                    self.handle_macro(parts[0].to_string(), parts).await;
                let mut parsed: Value = serde_json::from_str(&macro_result.as_str()).unwrap();

                let macro_name_parsed = "$".to_string() + macro_name.as_str() + ".";
                let replaced = replace_value.replace(macro_name_parsed.as_str(), "");
                let parts = replaced.split(".").collect::<Vec<&str>>();

                for part in parts.iter() {
                    if parsed.get(part).is_none() {
                        panic!("Macro {} not found", replace_value);
                    }

                    parsed = parsed.get(part).unwrap().clone();
                }

                url = url.replace(&replace_value, &parsed.as_str().unwrap())
            }

            let is_variable = self.variables.get(item.as_str());
            if is_variable.is_some() {
                url = url.replace(&replace_value, &is_variable.unwrap().to_string());
            }

            if !is_variable.is_some() && !is_hook.is_some() {
                panic!(
                    "Variable or hook not found: {}",
                    item.to_string().to_string()
                );
            }

            // url = url.replace(&item, &self.variables.get(&item).unwrap());
        }

        return url;
    }

    #[async_recursion]
    async fn execute(&self, req: &Request) -> Result<(String, String, String), Box<dyn Error>> {
        let url = self.handle_variables_and_hooks(req.path.clone()).await;

        let mut headers = req.headers.clone();
        for (key, value) in &req.headers {
            let normalized = self.handle_variables_and_hooks(value.clone()).await;
            headers.insert(key.clone(), normalized.clone());
        }

        let mut new = Request::new(
            req.method.clone(),
            url.clone(),
            req.body.clone(),
            req.multipart.clone(),
        );

        if !headers.is_empty() {
            new.set_headers(headers);
        }

        let http_method = new.format_method();
        let mut http_headers = HeaderMap::new();
        for (key, value) in new.headers.clone() {
            http_headers.insert(
                HeaderName::from_bytes(key.as_bytes()).unwrap(),
                HeaderValue::from_str(value.as_str()).unwrap(),
            );
        }

        let mut multipart: Option<multipart::Form> = None;

        let new_multipart = new.multipart.clone();
        if new_multipart.len() > 0 {
            let mut m = multipart::Form::new();

            for part in new_multipart.iter() {
                if part.content.starts_with("download://") {
                    let path_str = part.content.clone().replace("download://", "");
                    let bytes = reqwest::get(path_str.clone()).await?.bytes().await?;
                    let file_name = Url::parse(path_str.clone().as_str())
                        .unwrap()
                        .path_segments()
                        .unwrap()
                        .last()
                        .unwrap_or("file")
                        .to_string();

                    let mime = from_path(&path_str.as_str()).first_or_octet_stream();

                    let file_part = Part::bytes(bytes.to_vec())
                        .file_name(file_name.clone())
                        .mime_str(mime.as_ref())
                        .ok();
                    m = m.part(part.name.clone(), file_part.unwrap());
                } else if part.content.starts_with("file://") {
                    let path_str = part.content.clone().replace("file://", "");
                    let path = Path::new(&path_str);
                    let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
                    let content: Vec<u8> = fs::read(path).unwrap();

                    let mime = from_path(&path).first_or_octet_stream();

                    let file_part = Part::bytes(content)
                        .file_name(file_name.clone())
                        .mime_str(mime.as_ref())
                        .ok();

                    m = m.part(part.name.clone(), file_part.unwrap());
                } else {
                    m = m.text(part.name.clone(), part.content.to_string());
                }
            }

            multipart = Some(m);
            http_headers.remove("Content-Type");
        }

        let client = Client::new();

        let response = if multipart.is_some() {
            client
                .request(http_method, new.path)
                .headers(http_headers)
                .multipart(multipart.unwrap())
                .send()
                .await?
        } else {
            client
                .request(http_method, new.path)
                .body(new.body)
                .headers(http_headers)
                .send()
                .await?
        };

        let status = response.status();

        let body = response.text().await?;
        Ok((status.to_string(), url, body))
    }

    pub fn from_file(&mut self, filename: String) {
        self.filename = filename.clone();
        let mut context = "VARS";
        let mut last_id: String = String::new();
        let mut request_body: String = String::new();
        for line in fs::read_to_string(filename).unwrap().lines() {
            let mut line = line.to_string();
            if line.trim().starts_with("#") {
                continue;
            }
            if line.starts_with("VARS") {
                context = "VARS";
            } else if line.starts_with("HOOKS") {
                context = "HOOKS";
            } else if line.starts_with("ID:") {
                context = "REQUEST";
                if request_body != "" {
                    let req = self.requests.get_mut(&last_id).unwrap();
                    req.set_body(request_body);
                    request_body = String::new();
                }
                last_id = line.split(":").collect::<Vec<&str>>()[1].trim().to_string();
                self.add_request(last_id.clone(), Request::default());
            } else {
                if line.trim() == "" {
                    continue;
                }

                if context == "VARS" {
                    let parts = line.split("=").collect::<Vec<&str>>();
                    if parts.len() != 2 {
                        panic!("invalid variable provided {}", line);
                    }
                    let mut value = parts[1].trim().to_string();
                    if value.starts_with("$env.") {
                        value = env::var(value.replace("$env.", "")).unwrap();
                    }

                    if value.starts_with('"') && value.ends_with('"')
                        || value.starts_with("'") && value.ends_with("'")
                    {
                        value = value.replace('"', "").replace("'", "");
                    }

                    self.add_variable(parts[0].trim().to_string(), value);
                }
                if context == "HOOKS" {
                    let parts = line.split("=").collect::<Vec<&str>>();
                    self.add_hook(parts[0].trim().to_string(), parts[1].trim().to_string());
                }

                if context == "REQUEST" {
                    let req = self.requests.get_mut(&last_id).unwrap();
                    if line.starts_with("H:") {
                        line = line.replace("H:", "");
                        let parts = line.split("=").collect::<Vec<&str>>();
                        if parts.len() != 2 {
                            panic!("invalid header provided {}", line);
                        }
                        let key = parts[0].trim().to_string();
                        let value = parts[1].trim().to_string();
                        req.add_header(key, value);
                        continue;
                    }

                    if line.starts_with("M:") {
                        line = line.replace("M:", "");
                        let parts = line.split("=").collect::<Vec<&str>>();
                        if parts.len() < 2 {
                            panic!("invalid multipart form provided {}", line);
                        }
                        let key = parts[0].trim().to_string();
                        let value = parts[1..].join("=").trim().to_string();
                        req.add_multipart(key.clone(), value);
                        continue;
                    }

                    if line.starts_with("GET")
                        || line.starts_with("POST")
                        || line.starts_with("PUT")
                        || line.starts_with("DELETE")
                        || line.starts_with("PATCH")
                        || line.starts_with("HEAD")
                        || line.starts_with("OPTIONS")
                    {
                        let parts: Vec<&str> = line.split(" ").collect::<Vec<&str>>();
                        if parts.len() != 2 {
                            panic!("invalid request provided {}", line);
                        }
                        req.set_method(parts[0].trim().to_string());
                        req.set_path(parts[1].trim().to_string());
                        continue;
                    }

                    request_body.push_str(&line.trim().to_string());
                }
            }
        }
        if request_body != "" {
            let req = self.requests.get_mut(&last_id).unwrap();
            req.set_body(request_body);
        }
    }

    fn add_request(&mut self, id: String, request: Request) {
        self.requests.insert(id, request);
    }

    fn add_variable(&mut self, name: String, value: String) {
        self.variables.insert(name, value);
    }

    fn add_hook(&mut self, id: String, value: String) {
        self.hooks.insert(id, value);
    }
}
