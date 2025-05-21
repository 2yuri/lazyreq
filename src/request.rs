use std::collections::HashMap;

use reqwest::Method;

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub multipart: Vec<MultiPart>,
}

#[derive(Clone)]
pub struct MultiPart {
    pub name: String,
    pub content: String,
}

impl Request {
    pub fn default() -> Request {
        Request {
            method: "NOT SET".to_string(),
            path: "".to_string(),
            headers: HashMap::new(),
            body: "".to_string(),
            multipart: Vec::new(),
        }
    }

    pub fn format_method(&self) -> Method {
        match self.method.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "PATCH" => Method::PATCH,
            "HEAD" => Method::HEAD,
            "OPTIONS" => Method::OPTIONS,
            _ => Method::GET, // Default to GET if unknown
        }
    }

    pub fn new(method: String, path: String, body: String, multipart: Vec<MultiPart>) -> Request {
        Request {
            method,
            path,
            headers: HashMap::new(),
            body,
            multipart,
        }
    }

    pub fn add_header(&mut self, name: String, value: String) {
        self.headers.insert(name, value);
    }

    pub fn add_multipart(&mut self, name: String, value: String) {
        self.multipart.push(MultiPart {
            name,
            content: value,
        });
    }

    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        self.headers = headers;
    }

    pub fn set_method(&mut self, method: String) {
        self.method = method;
    }

    pub fn set_path(&mut self, path: String) {
        self.path = path;
    }

    pub fn set_body(&mut self, body: String) {
        self.body = body;
    }
}
