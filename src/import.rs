use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

/// Converts a curl command (e.g. from a browser's "Copy as cURL") into a
/// .lreq request block.
pub fn curl_to_lreq(command: &str) -> Result<String, String> {
    let words = shell_split(command)?;
    if words.is_empty() || words[0] != "curl" {
        return Err("expected a command starting with `curl`".to_string());
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut body_parts: Vec<String> = Vec::new();
    let mut multipart: Vec<(String, String)> = Vec::new();

    let mut iter = words.iter().skip(1);
    while let Some(word) = iter.next() {
        let mut value = |flag: &str| -> Result<String, String> {
            iter.next()
                .cloned()
                .ok_or(format!("`{}` is missing its value", flag))
        };

        match word.as_str() {
            "-X" | "--request" => method = Some(value(word)?.to_uppercase()),
            "-H" | "--header" => {
                let header = value(word)?;
                match header.split_once(':') {
                    Some((name, val)) => {
                        headers.push((name.trim().to_string(), val.trim().to_string()));
                    }
                    None => return Err(format!("invalid header `{}`", header)),
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii"
            | "--data-urlencode" => {
                let mut data = value(word)?;
                if let Some(stripped) = data.strip_prefix('@') {
                    return Err(format!(
                        "`{} @{}` reads a file; paste the body inline instead",
                        word, stripped
                    ));
                }
                if data.starts_with('$') {
                    // ANSI-C quoted strings arrive as $'...' when copied from
                    // some browsers; shell_split already handled quotes, so a
                    // leading $ here is just data.
                    data = data.to_string();
                }
                body_parts.push(data);
            }
            "-F" | "--form" => {
                let field = value(word)?;
                match field.split_once('=') {
                    Some((name, val)) => {
                        let val = match val.strip_prefix('@') {
                            Some(path) => format!("file://{}", path),
                            None => val.to_string(),
                        };
                        multipart.push((name.trim().to_string(), val));
                    }
                    None => return Err(format!("invalid form field `{}`", field)),
                }
            }
            "-u" | "--user" => {
                let credentials = value(word)?;
                headers.push((
                    "Authorization".to_string(),
                    format!("Basic {}", BASE64.encode(credentials.as_bytes())),
                ));
            }
            "-b" | "--cookie" => {
                let cookie = value(word)?;
                headers.push(("Cookie".to_string(), cookie));
            }
            "-A" | "--user-agent" => {
                let agent = value(word)?;
                headers.push(("User-Agent".to_string(), agent));
            }
            "-e" | "--referer" => {
                let referer = value(word)?;
                headers.push(("Referer".to_string(), referer));
            }
            "--url" => url = Some(value(word)?),
            // Flags that take a value we don't map — consume and ignore it.
            "-o" | "--output" | "--connect-timeout" | "--max-time" | "-m" | "--retry"
            | "--cacert" | "--capath" | "-c" | "--cookie-jar" | "-w" | "--write-out" => {
                value(word)?;
            }
            // Boolean flags we can safely ignore.
            "-s" | "--silent" | "-S" | "--show-error" | "-L" | "--location" | "-k"
            | "--insecure" | "--compressed" | "-i" | "--include" | "-v" | "--verbose"
            | "-f" | "--fail" | "-g" | "--globoff" => {}
            "-G" | "--get" => method = Some("GET".to_string()),
            "-I" | "--head" => method = Some("HEAD".to_string()),
            other => {
                if other.starts_with('-') {
                    return Err(format!(
                        "unsupported curl flag `{}` — remove it and try again",
                        other
                    ));
                }
                if url.is_some() {
                    return Err(format!("unexpected argument `{}`", other));
                }
                url = Some(other.to_string());
            }
        }
    }

    let url = url.ok_or("no URL found in the curl command".to_string())?;
    let method = method.unwrap_or_else(|| {
        if body_parts.is_empty() && multipart.is_empty() {
            "GET".to_string()
        } else {
            "POST".to_string()
        }
    });

    let id = suggest_id(&url);
    let mut out = format!("ID: {}\n{} {}\n", id, method, escape(&url));

    for (name, value) in &headers {
        out.push_str(&format!("H: {} = {}\n", name, escape(value)));
    }
    for (name, value) in &multipart {
        out.push_str(&format!("M: {} = {}\n", name, escape(value)));
    }
    if !body_parts.is_empty() {
        out.push_str(&body_parts.join("&"));
        out.push('\n');
    }

    Ok(out)
}

/// `$` is special in .lreq strict contexts (URLs, headers, multipart), so
/// literal dollars from the imported command are escaped as `$$`.
fn escape(value: &str) -> String {
    value.replace('$', "$$")
}

fn suggest_id(url: &str) -> String {
    let no_query = url.split('?').next().unwrap_or(url);
    // Skip the scheme and host so `https://api.x.com` doesn't become `api-x-com`.
    let after_scheme = no_query.split("://").nth(1).unwrap_or(no_query);
    let mut segments = after_scheme.trim_end_matches('/').split('/');
    segments.next(); // host

    let last = segments.last().unwrap_or("");
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    if cleaned.trim_matches('-').is_empty() {
        "imported".to_string()
    } else {
        cleaned
    }
}

/// Splits a shell command into words, honoring single quotes, double quotes,
/// backslash escapes and ANSI-C `$'...'` quoting — enough for pasted curl
/// commands from browsers and docs.
fn shell_split(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut has_word = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if has_word {
                    words.push(std::mem::take(&mut current));
                    has_word = false;
                }
            }
            '\\' => {
                // A backslash at end of line is a line continuation.
                match chars.next() {
                    Some('\n') | None => {}
                    Some(next) => {
                        current.push(next);
                        has_word = true;
                    }
                }
            }
            '\'' => {
                has_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return Err("unclosed single quote".to_string()),
                    }
                }
            }
            '"' => {
                has_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '$' | '`')) => current.push(escaped),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => return Err("unclosed double quote".to_string()),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err("unclosed double quote".to_string()),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                // ANSI-C quoting: $'...' — treat like single quotes with
                // basic escape handling.
                chars.next();
                has_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => current.push('\n'),
                            Some('t') => current.push('\t'),
                            Some('r') => current.push('\r'),
                            Some(escaped) => current.push(escaped),
                            None => return Err("unclosed $' quote".to_string()),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err("unclosed $' quote".to_string()),
                    }
                }
            }
            _ => {
                current.push(c);
                has_word = true;
            }
        }
    }

    if has_word {
        words.push(current);
    }

    Ok(words)
}
