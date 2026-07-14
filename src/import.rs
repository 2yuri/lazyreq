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
        out.push_str(&prettify_body(&body_parts.join("&")));
        out.push('\n');
    }

    Ok(out)
}

/// JSON bodies come out of "Copy as cURL" compacted onto one line; reformat
/// them so the .lreq block is readable. Anything that isn't JSON (form
/// payloads, plain text) is left untouched.
fn prettify_body(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .filter(|v| v.is_object() || v.is_array())
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| body.to_string())
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
                    Some('\r') => {
                        // Windows line continuation: \ + CRLF.
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                    }
                    Some(' ' | '\t') if !has_word => {
                        // A continuation backslash with trailing whitespace,
                        // or one whose newline was eaten by a single-line
                        // paste (e.g. an editor input box): a standalone
                        // whitespace-only word is never meaningful in a curl
                        // command, so drop it instead of failing.
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_a_multiline_curl_with_continuations() {
        let block = curl_to_lreq(
            "curl -X POST \\\n  -H \"Content-Type: application/json\" \\\n  https://api.x.dev/users",
        )
        .unwrap();
        assert!(block.contains("ID: users"));
        assert!(block.contains("POST https://api.x.dev/users"));
        assert!(block.contains("H: Content-Type = application/json"));
    }

    #[test]
    fn tolerates_continuations_flattened_by_a_single_line_paste() {
        // Pasting a multiline command into a single-line input eats the
        // newlines, leaving `\ ` between words.
        let block = curl_to_lreq("curl -X POST \\ https://foo.com").unwrap();
        assert!(block.contains("POST https://foo.com"));

        // Trailing whitespace after the continuation backslash.
        let block = curl_to_lreq("curl -X POST \\ \nhttps://foo.com").unwrap();
        assert!(block.contains("POST https://foo.com"));

        // Windows CRLF continuations.
        let block = curl_to_lreq("curl -X POST \\\r\nhttps://foo.com").unwrap();
        assert!(block.contains("POST https://foo.com"));
    }

    #[test]
    fn escaped_spaces_inside_words_still_work() {
        let block = curl_to_lreq("curl https://x.dev -F file=@my\\ photo.png").unwrap();
        assert!(block.contains("M: file = file://my photo.png"));
    }

    #[test]
    fn json_bodies_are_pretty_printed() {
        let block =
            curl_to_lreq(r#"curl https://x.dev/orders -d '{"shop_id": 13733,"id": 12312}'"#)
                .unwrap();
        assert!(block.ends_with("{\n  \"shop_id\": 13733,\n  \"id\": 12312\n}\n"));
    }

    #[test]
    fn non_json_bodies_are_left_alone() {
        let block = curl_to_lreq("curl https://x.dev -d a=1 -d b=2").unwrap();
        assert!(block.ends_with("a=1&b=2\n"));

        let block = curl_to_lreq(r#"curl https://x.dev -d '"just a string"'"#).unwrap();
        assert!(block.ends_with("\"just a string\"\n"));
    }
}
