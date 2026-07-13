use async_recursion::async_recursion;
use colored::*;
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
use crate::functions;
use crate::request::Request;

const MAX_DEPTH: usize = 16;
const TOKEN_PATTERN: &str = r"\$\$|\$[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)*(?:\([^)]*\))?";

pub struct LazyReq {
    variables: HashMap<String, String>,
    hooks: HashMap<String, String>,
    requests: HashMap<String, Request>,
    order: Vec<String>,
    filename: String,
}

/// A resolved request, ready to send: everything interpolated.
struct Prepared {
    url: String,
    headers: HashMap<String, String>,
    body: String,
    multipart: Vec<(String, String)>,
}

impl LazyReq {
    pub fn new() -> LazyReq {
        LazyReq {
            variables: HashMap::new(),
            hooks: HashMap::new(),
            requests: HashMap::new(),
            order: Vec::new(),
            filename: "".to_string(),
        }
    }

    pub fn list(&self) {
        let width = self
            .order
            .iter()
            .map(|id| id.len())
            .max()
            .unwrap_or(0);

        for id in &self.order {
            let req = &self.requests[id];
            let description = if req.description.is_empty() {
                "".normal()
            } else {
                format!("  — {}", req.description).dimmed()
            };
            println!(
                "{:width$}  {:7} {}{}",
                id.bold().green(),
                req.method,
                req.path,
                description,
                width = width
            );
        }
    }

    pub async fn do_request(&self, id: String) -> Result<(), String> {
        let req = self.requests.get(&id).ok_or(format!(
            "request `{}` not found in {} (use --list to see available requests)",
            id, self.filename
        ))?;

        let (status, url, result) = self
            .execute(req, 0)
            .await
            .map_err(|e| format!("request `{}` failed:\n  {}", id, e))?;

        let status_colored = match status.chars().next() {
            Some('2') => status.bold().green(),
            Some('3') => status.bold().yellow(),
            _ => status.bold().red(),
        };

        println!(
            "{}{}{} {}",
            "[".bold().green(),
            req.method.bold().green(),
            "]".bold().green(),
            url.bold().green()
        );
        println!("{} {}", "Status:".bold().green(), status_colored);

        let pretty_json: Value = serde_json::from_str(result.as_str()).unwrap_or(Value::Null);
        if !pretty_json.is_null() {
            println!("{}", to_string_pretty(&pretty_json).unwrap());
        } else {
            println!("{}", result);
        }

        Ok(())
    }

    pub async fn export_curl(&self, id: String) -> Result<(), String> {
        let req = self.requests.get(&id).ok_or(format!(
            "request `{}` not found in {} (use --list to see available requests)",
            id, self.filename
        ))?;

        let prepared = self
            .prepare(req, 0)
            .await
            .map_err(|e| format!("cannot resolve request `{}`:\n  {}", id, e))?;

        let mut curl_parts = vec![format!("curl -X {}", req.method.to_uppercase())];

        for (key, value) in &prepared.headers {
            curl_parts.push(format!("-H \"{}: {}\"", key, value));
        }

        if !prepared.multipart.is_empty() {
            for (name, content) in &prepared.multipart {
                if let Some(path) = content.strip_prefix("file://") {
                    curl_parts.push(format!("-F \"{}=@{}\"", name, path));
                } else {
                    curl_parts.push(format!("-F \"{}={}\"", name, content));
                }
            }
        } else if !prepared.body.is_empty() {
            curl_parts.push(format!("-d '{}'", prepared.body));
        }

        curl_parts.push(format!("\"{}\"", prepared.url));

        println!("{}", curl_parts.join(" \\\n  "));
        Ok(())
    }

    /// Interpolates `$variables`, `$env.X`, `$hook.field` references, `$body`
    /// and `$function(...)` calls in `input`.
    ///
    /// `strict` mode (URLs, headers, multipart values) errors on unknown
    /// tokens; lenient mode (request bodies) passes them through untouched so
    /// natural dollar-words in JSON (`$gte`, `$set`, ...) keep working.
    #[async_recursion]
    async fn interpolate(
        &self,
        input: &str,
        strict: bool,
        body: Option<&str>,
        depth: usize,
    ) -> Result<String, String> {
        if depth > MAX_DEPTH {
            return Err("recursion limit reached (circular hook reference?)".to_string());
        }

        let re = Regex::new(TOKEN_PATTERN).unwrap();
        let mut output = String::with_capacity(input.len());
        let mut last_end = 0;

        for m in re.find_iter(input) {
            output.push_str(&input[last_end..m.start()]);
            last_end = m.end();

            let token = m.as_str();
            if token == "$$" {
                output.push('$');
                continue;
            }

            let resolved = self.resolve_token(token, strict, body, depth).await?;
            output.push_str(&resolved);
        }

        output.push_str(&input[last_end..]);
        Ok(output)
    }

    async fn resolve_token(
        &self,
        token: &str,
        strict: bool,
        body: Option<&str>,
        depth: usize,
    ) -> Result<String, String> {
        // Split "$name.path.to.field(args)" into its pieces.
        let inner = &token[1..];
        let (name_path, args) = match inner.find('(') {
            Some(idx) => (
                &inner[..idx],
                Some(inner[idx + 1..inner.len() - 1].to_string()),
            ),
            None => (inner, None),
        };
        let mut segments = name_path.split('.');
        let name = segments.next().unwrap();
        let path: Vec<&str> = segments.collect();

        // Function call: $uuid(), $hmac(data, key), ...
        if let Some(args_str) = args {
            if path.is_empty() && functions::is_function(name) {
                let mut resolved_args = Vec::new();
                if !args_str.trim().is_empty() {
                    for raw in args_str.split(',') {
                        let interpolated =
                            self.interpolate(raw.trim(), true, body, depth + 1).await?;
                        resolved_args.push(strip_quotes(&interpolated));
                    }
                }
                return functions::call(name, &resolved_args);
            }

            if strict {
                return Err(format!(
                    "unknown function `${}()` (available: $uuid, $fuzz_str, $fuzz_int, $hmac)",
                    name_path
                ));
            }
            return Ok(token.to_string());
        }

        // $env.VAR_NAME
        if name == "env" {
            let var = path.join(".");
            if var.is_empty() {
                return Err("`$env` needs a variable name, e.g. `$env.API_TOKEN`".to_string());
            }
            return env::var(&var)
                .map_err(|_| format!("environment variable `{}` is not set", var));
        }

        // $body — the interpolated body of the request being sent
        if name == "body" && path.is_empty() {
            match body {
                Some(b) => return Ok(b.to_string()),
                None => {
                    if strict {
                        return Err(
                            "`$body` is only available in URLs, headers and multipart values"
                                .to_string(),
                        );
                    }
                    return Ok(token.to_string());
                }
            }
        }

        // Hook: $login.token — executes the hooked request and drills into
        // its JSON response.
        if let Some(hook_def) = self.hooks.get(name) {
            let response = self.resolve_hook(name, hook_def, depth).await?;
            if path.is_empty() {
                return Ok(response);
            }

            let mut parsed: Value = serde_json::from_str(&response).map_err(|_| {
                format!(
                    "hook `{}` response is not JSON, cannot resolve `{}`",
                    name, token
                )
            })?;

            for part in &path {
                parsed = match parsed.get(part) {
                    Some(v) => v.clone(),
                    None => {
                        let available = match &parsed {
                            Value::Object(map) => format!(
                                " (available fields: {})",
                                map.keys().cloned().collect::<Vec<_>>().join(", ")
                            ),
                            _ => String::new(),
                        };
                        return Err(format!(
                            "field `{}` not found in response of hook `{}`{}",
                            part, name, available
                        ));
                    }
                };
            }

            return Ok(match parsed {
                Value::String(s) => s,
                other => other.to_string(),
            });
        }

        // Plain variable
        if let Some(value) = self.variables.get(name) {
            let mut result = value.clone();
            if !path.is_empty() {
                result.push('.');
                result.push_str(&path.join("."));
            }
            return Ok(result);
        }

        if strict {
            return Err(format!(
                "unknown variable or hook `{}` (use `$${}` for a literal `$`)",
                token,
                &token[1..]
            ));
        }
        Ok(token.to_string())
    }

    async fn resolve_hook(
        &self,
        name: &str,
        definition: &str,
        depth: usize,
    ) -> Result<String, String> {
        let parts: Vec<&str> = definition.split_whitespace().collect();
        let req_id = parts[0].strip_prefix("$req.").ok_or(format!(
            "invalid hook `{}`: expected `$req.<request-id> [cache-seconds]`, got `{}`",
            name, definition
        ))?;

        let ttl: Option<u64> = match parts.get(1) {
            Some(raw) => Some(raw.parse().map_err(|_| {
                format!(
                    "invalid cache duration `{}` on hook `{}` (expected seconds)",
                    raw, name
                )
            })?),
            None => None,
        };

        let mut cacher = match ttl {
            Some(_) => Some(Cache::new(&self.filename, req_id)?),
            None => None,
        };
        if let Some(c) = cacher.as_mut() {
            if let Some(cached) = c.get() {
                return Ok(cached);
            }
        }

        let req = self.requests.get(req_id).ok_or(format!(
            "hook `{}` references unknown request `{}`",
            name, req_id
        ))?;

        let (status, _, result) = self
            .execute(req, depth + 1)
            .await
            .map_err(|e| format!("hook `{}` (request `{}`) failed: {}", name, req_id, e))?;

        if !status.starts_with('2') {
            return Err(format!(
                "hook `{}` (request `{}`) returned status {}: {}",
                name, req_id, status, result
            ));
        }

        if let (Some(c), Some(seconds)) = (cacher.as_mut(), ttl) {
            c.set(result.clone(), seconds)?;
        }

        Ok(result)
    }

    /// Resolves every dynamic part of a request: body first (lenient), then
    /// URL, headers and multipart values (strict, with `$body` available).
    async fn prepare(&self, req: &Request, depth: usize) -> Result<Prepared, String> {
        if depth > MAX_DEPTH {
            return Err("recursion limit reached (circular hook reference?)".to_string());
        }

        let body = if req.body.is_empty() {
            String::new()
        } else {
            self.interpolate(&req.body, false, None, depth).await?
        };

        let url = self.interpolate(&req.path, true, Some(&body), depth).await?;

        let mut headers = HashMap::new();
        for (key, value) in &req.headers {
            let resolved = self.interpolate(value, true, Some(&body), depth).await?;
            headers.insert(key.clone(), resolved);
        }

        let mut parts = Vec::new();
        for part in &req.multipart {
            let resolved = self
                .interpolate(&part.content, true, Some(&body), depth)
                .await?;
            parts.push((part.name.clone(), resolved));
        }

        Ok(Prepared {
            url,
            headers,
            body,
            multipart: parts,
        })
    }

    #[async_recursion]
    async fn execute(&self, req: &Request, depth: usize) -> Result<(String, String, String), String> {
        let prepared = self.prepare(req, depth).await?;

        let mut http_headers = HeaderMap::new();
        for (key, value) in &prepared.headers {
            let header_name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| format!("invalid header name `{}`", key))?;
            let header_value = HeaderValue::from_str(value)
                .map_err(|_| format!("invalid value for header `{}`: `{}`", key, value))?;
            http_headers.insert(header_name, header_value);
        }

        let mut form: Option<multipart::Form> = None;
        if !prepared.multipart.is_empty() {
            let mut m = multipart::Form::new();

            for (name, content) in &prepared.multipart {
                if let Some(download_url) = content.strip_prefix("download://") {
                    let response = reqwest::get(download_url).await.map_err(|e| {
                        format!("cannot download `{}`: {}", download_url, describe(&e))
                    })?;
                    let bytes = response.bytes().await.map_err(|e| {
                        format!("cannot download `{}`: {}", download_url, describe(&e))
                    })?;
                    let file_name = Url::parse(download_url)
                        .ok()
                        .and_then(|u| {
                            u.path_segments()
                                .and_then(|s| s.last().map(|s| s.to_string()))
                        })
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "file".to_string());

                    let mime = from_path(download_url).first_or_octet_stream();
                    let part = Part::bytes(bytes.to_vec())
                        .file_name(file_name)
                        .mime_str(mime.as_ref())
                        .map_err(|e| format!("invalid mime type for `{}`: {}", name, e))?;
                    m = m.part(name.clone(), part);
                } else if let Some(path_str) = content.strip_prefix("file://") {
                    let path = Path::new(path_str);
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .ok_or(format!("invalid file path `{}`", path_str))?
                        .to_string();
                    let content = fs::read(path)
                        .map_err(|e| format!("cannot read file `{}`: {}", path_str, e))?;

                    let mime = from_path(path).first_or_octet_stream();
                    let part = Part::bytes(content)
                        .file_name(file_name)
                        .mime_str(mime.as_ref())
                        .map_err(|e| format!("invalid mime type for `{}`: {}", name, e))?;
                    m = m.part(name.clone(), part);
                } else {
                    m = m.text(name.clone(), content.clone());
                }
            }

            form = Some(m);
            // reqwest sets the multipart boundary itself
            http_headers.remove("Content-Type");
        }

        let client = Client::new();
        let request = client
            .request(req.format_method(), &prepared.url)
            .headers(http_headers);

        let request = match form {
            Some(f) => request.multipart(f),
            None => request.body(prepared.body.clone()),
        };

        let response = request.send().await.map_err(|e| describe(&e))?;
        let status = response.status();
        let body = response.text().await.map_err(|e| describe(&e))?;

        Ok((status.to_string(), prepared.url, body))
    }

    pub fn from_file(&mut self, filename: String) -> Result<(), String> {
        self.filename = filename.clone();

        let content = fs::read_to_string(&filename)
            .map_err(|e| format!("cannot read `{}`: {}", filename, e))?;

        let mut context = "VARS";
        let mut last_id: String = String::new();
        let mut request_body: String = String::new();

        for (index, raw_line) in content.lines().enumerate() {
            let ln = index + 1;
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line == "VARS" {
                context = "VARS";
            } else if line == "HOOKS" {
                context = "HOOKS";
            } else if let Some(id) = line.strip_prefix("ID:") {
                context = "REQUEST";
                if !request_body.is_empty() {
                    self.requests.get_mut(&last_id).unwrap().set_body(request_body);
                    request_body = String::new();
                }
                last_id = id.trim().to_string();
                if last_id.is_empty() {
                    return Err(self.parse_error(ln, raw_line, "requests need an id, e.g. `ID: login`"));
                }
                if self.requests.contains_key(&last_id) {
                    return Err(self.parse_error(
                        ln,
                        raw_line,
                        &format!("duplicate request id `{}`", last_id),
                    ));
                }
                self.requests.insert(last_id.clone(), Request::default());
                self.order.push(last_id.clone());
            } else if context == "VARS" {
                let (key, value) = split_key_value(line).ok_or_else(|| {
                    self.parse_error(ln, raw_line, "variables use `name = value`")
                })?;

                let mut value = value;
                if let Some(var) = value.strip_prefix("$env.") {
                    value = env::var(var).map_err(|_| {
                        self.parse_error(
                            ln,
                            raw_line,
                            &format!("environment variable `{}` is not set", var),
                        )
                    })?;
                }

                self.variables.insert(key, strip_quotes(&value));
            } else if context == "HOOKS" {
                let (key, value) = split_key_value(line).ok_or_else(|| {
                    self.parse_error(
                        ln,
                        raw_line,
                        "hooks use `name = $req.<request-id> [cache-seconds]`",
                    )
                })?;
                self.hooks.insert(key, value);
            } else {
                // context == "REQUEST"
                if let Some(rest) = line.strip_prefix("DESCRIPTION:") {
                    let req = self.requests.get_mut(&last_id).unwrap();
                    req.set_description(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("H:") {
                    let (key, value) = split_key_value(rest).ok_or_else(|| {
                        self.parse_error(ln, raw_line, "headers use `H: Name = value`")
                    })?;
                    let req = self.requests.get_mut(&last_id).unwrap();
                    req.add_header(key, strip_quotes(&value));
                } else if let Some(rest) = line.strip_prefix("M:") {
                    let (key, value) = split_key_value(rest).ok_or_else(|| {
                        self.parse_error(ln, raw_line, "multipart fields use `M: name = value`")
                    })?;
                    let req = self.requests.get_mut(&last_id).unwrap();
                    req.add_multipart(key, value);
                } else if is_method_line(line) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() != 2 {
                        return Err(self.parse_error(
                            ln,
                            raw_line,
                            "request lines use `METHOD url`, e.g. `GET $baseURL/users`",
                        ));
                    }
                    let req = self.requests.get_mut(&last_id).unwrap();
                    req.set_method(parts[0].to_string());
                    req.set_path(parts[1].to_string());
                } else {
                    if self.requests[&last_id].method.is_empty() {
                        return Err(self.parse_error(
                            ln,
                            raw_line,
                            &format!(
                                "request `{}` needs a `METHOD url` line before its body",
                                last_id
                            ),
                        ));
                    }
                    request_body.push_str(line);
                }
            }
        }

        if !request_body.is_empty() {
            self.requests.get_mut(&last_id).unwrap().set_body(request_body);
        }

        for id in &self.order {
            if self.requests[id].method.is_empty() {
                return Err(format!(
                    "request `{}` in {} has no `METHOD url` line",
                    id, filename
                ));
            }
        }

        Ok(())
    }

    fn parse_error(&self, line_number: usize, line: &str, hint: &str) -> String {
        format!(
            "invalid line {} of {}:\n  {}\n  {} {}",
            line_number,
            self.filename,
            line.trim(),
            "hint:".bold(),
            hint
        )
    }
}

/// Splits `key = value` on the FIRST `=` only, so values containing `=`
/// (base64, signatures, urls) survive intact. Rejects keys that aren't a
/// single bare word, which catches lines missing their `=` separator.
fn split_key_value(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some((key.to_string(), value.trim().to_string()))
}

/// Removes one pair of matching surrounding quotes, leaving inner quotes alone.
fn strip_quotes(value: &str) -> String {
    let bytes = value.as_bytes();
    if value.len() >= 2
        && (bytes[0] == b'"' && bytes[value.len() - 1] == b'"'
            || bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
    {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

fn is_method_line(line: &str) -> bool {
    ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"]
        .iter()
        .any(|m| line.starts_with(m))
}

fn describe(e: &reqwest::Error) -> String {
    // reqwest errors nest the same cause several times; report the message
    // plus the root cause only.
    let mut root = None;
    let mut source = e.source();
    while let Some(s) = source {
        root = Some(s.to_string());
        source = s.source();
    }

    let msg = e.to_string();
    match root {
        Some(cause) if !msg.contains(&cause) => format!("{}: {}", msg, cause),
        _ => msg,
    }
}
