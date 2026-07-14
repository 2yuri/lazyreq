//! Encrypted per-file run history under `~/.lazyreq/history/<hash>.jsonl`.
//! Every executed request (including hook-triggered runs) appends a record;
//! the last `KEEP_PER_ID` runs are kept per request id. Reading always goes
//! through the CLI (`--history`), which renders compact, token-friendly
//! summaries by default.

use crate::config::{HistoryFilter, HistoryOpts};
use crate::timest::format_timestamp;
use crate::vault;
use colored::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const KEEP_PER_ID: usize = 20;

#[derive(Serialize, Deserialize)]
pub struct Record {
    pub v: u8,
    pub ts: u64,
    pub id: String,
    pub method: String,
    pub url: String,
    /// HTTP status code; 0 means the request never got a response
    /// (DNS failure, timeout, connection refused, ...) — see `error`.
    pub status: u16,
    pub ms: u64,
    pub req_headers: HashMap<String, String>,
    pub req_body: String,
    pub resp_body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn history_path(filename: &str) -> Result<PathBuf, String> {
    let dir = vault::lazyreq_dir()?.join("history");

    fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create history directory `{}`: {}", dir.display(), e))?;

    Ok(dir.join(format!("{}.jsonl", vault::file_id(filename, ""))))
}

fn load(path: &PathBuf) -> Vec<Record> {
    // Missing, corrupt or foreign-key files just mean "no history".
    fs::read(path)
        .ok()
        .and_then(|bytes| vault::open(&bytes))
        .and_then(|plain| String::from_utf8(plain).ok())
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Appends a run, pruning to the newest `KEEP_PER_ID` per request id.
/// Best-effort by design: recording must never fail the request itself.
pub fn record(filename: &str, record: Record) {
    let result = (|| -> Result<(), String> {
        let path = history_path(filename)?;
        let mut records = load(&path);
        records.push(record);
        let kept = prune(records);

        let lines: Vec<String> = kept
            .iter()
            .filter_map(|r| serde_json::to_string(r).ok())
            .collect();
        let sealed = vault::seal(lines.join("\n").as_bytes())?;
        fs::write(&path, sealed).map_err(|e| format!("cannot write `{}`: {}", path.display(), e))
    })();

    if let Err(e) = result {
        eprintln!("{} could not record history: {}", "warning:".yellow(), e);
    }
}

/// Keeps the newest `KEEP_PER_ID` records per request id, preserving order.
fn prune(records: Vec<Record>) -> Vec<Record> {
    let mut per_id: HashMap<String, usize> = HashMap::new();
    for r in &records {
        *per_id.entry(r.id.clone()).or_insert(0) += 1;
    }

    let mut kept = Vec::with_capacity(records.len());
    for r in records.into_iter() {
        let remaining = per_id.get_mut(&r.id).unwrap();
        if *remaining <= KEEP_PER_ID {
            kept.push(r);
        }
        *remaining -= 1;
    }
    kept
}

fn matches(r: &Record, id: Option<&str>, filter: &HistoryFilter) -> bool {
    id.map_or(true, |id| r.id == id)
        && match filter {
            HistoryFilter::All => true,
            HistoryFilter::Success => (200..300).contains(&r.status),
            HistoryFilter::Failed => !(200..300).contains(&r.status),
            HistoryFilter::Status(code) => r.status == *code,
        }
}

pub fn show(filename: &str, id: Option<&str>, opts: &HistoryOpts) -> Result<(), String> {
    let records = load(&history_path(filename)?);

    let matches: Vec<&Record> = records
        .iter()
        .filter(|r| matches(r, id, &opts.filter))
        .collect();

    if matches.is_empty() {
        println!(
            "no matching history for {}{} yet",
            filename,
            id.map(|id| format!(" `{}`", id)).unwrap_or_default()
        );
        return Ok(());
    }

    let limit = opts.last.unwrap_or(usize::MAX);
    let shown = &matches[matches.len().saturating_sub(limit)..];
    let id_width = shown.iter().map(|r| r.id.len()).max().unwrap_or(0);

    for record in shown {
        print_line(record, id_width);
        if opts.verbose {
            print_details(record, opts.show_headers);
        } else if id.is_some() {
            println!("    {}", shape_of(&record.resp_body).dimmed());
        }
    }

    Ok(())
}

fn print_line(r: &Record, id_width: usize) {
    let status = match r.status {
        0 => "ERR".bold().red(),
        200..=299 => r.status.to_string().bold().green(),
        300..=399 => r.status.to_string().bold().yellow(),
        _ => r.status.to_string().bold().red(),
    };

    let tail = match &r.error {
        Some(e) => e.normal(),
        None => format!(
            "body:{}  headers:{}",
            human_size(r.resp_body.len()),
            r.req_headers.len()
        )
        .normal(),
    };

    println!(
        "{}  {:id_width$}  {:7} {}  {:>4}ms  {}",
        format_timestamp(r.ts).dimmed(),
        r.id.bold().green(),
        r.method,
        status,
        r.ms,
        tail,
    );
}

fn print_details(r: &Record, show_headers: bool) {
    println!("    {} {}", "url:".bold(), r.url);

    if show_headers {
        for (name, value) in &r.req_headers {
            println!("    {} {} = {}", "H:".bold(), name, value);
        }
    }

    if !r.req_body.is_empty() {
        println!("    {}", "request:".bold());
        println!("{}", indent(&pretty(&r.req_body), 6));
    }

    if let Some(e) = &r.error {
        println!("    {} {}", "error:".bold().red(), e);
    }

    if !r.resp_body.is_empty() {
        println!("    {}", "response:".bold());
        println!("{}", indent(&pretty(&r.resp_body), 6));
    }
    println!();
}

fn pretty(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| body.to_string())
}

fn indent(text: &str, spaces: usize) -> String {
    text.lines()
        .map(|l| format!("{:spaces$}{}", "", l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Token-frugal structural summary of a response body: keys and types
/// instead of values, e.g. `{token: str(212), user: {id: str, roles: [2 × str]}}`.
fn shape_of(body: &str) -> String {
    if body.is_empty() {
        return "(empty body)".to_string();
    }
    match serde_json::from_str::<Value>(body) {
        Ok(value) => shape(&value, 0),
        Err(_) => format!("text({})", body.len()),
    }
}

fn shape(value: &Value, depth: usize) -> String {
    if depth > 4 {
        return "…".to_string();
    }
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Number(n) => if n.is_f64() { "float" } else { "int" }.to_string(),
        Value::String(s) => format!("str({})", s.chars().count()),
        Value::Array(items) => match items.first() {
            None => "[]".to_string(),
            Some(first) => format!("[{} × {}]", items.len(), shape(first, depth + 1)),
        },
        Value::Object(map) => {
            let fields: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", k, shape(v, depth + 1)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
    }
}

fn human_size(bytes: usize) -> String {
    match bytes {
        0..=1023 => format!("{}b", bytes),
        1024..=1_048_575 => format!("{:.1}kb", bytes as f64 / 1024.0),
        _ => format!("{:.1}mb", bytes as f64 / 1_048_576.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, ts: u64, status: u16) -> Record {
        Record {
            v: 1,
            ts,
            id: id.to_string(),
            method: "GET".to_string(),
            url: "http://localhost/x".to_string(),
            status,
            ms: 1,
            req_headers: HashMap::new(),
            req_body: String::new(),
            resp_body: String::new(),
            error: if status == 0 {
                Some("connection refused".to_string())
            } else {
                None
            },
        }
    }

    #[test]
    fn prune_keeps_newest_per_id_without_starving_rare_ids() {
        let mut records: Vec<Record> = (0..30).map(|i| record("login", i, 200)).collect();
        records.insert(0, record("me", 1000, 200)); // old, rarely-run request

        let kept = prune(records);

        assert_eq!(kept.len(), KEEP_PER_ID + 1);
        assert!(kept.iter().any(|r| r.id == "me")); // hot id can't evict it
        let login_ts: Vec<u64> = kept.iter().filter(|r| r.id == "login").map(|r| r.ts).collect();
        assert_eq!(login_ts, (10..30).collect::<Vec<u64>>()); // newest 20, in order
    }

    #[test]
    fn prune_leaves_small_histories_alone() {
        let kept = prune(vec![record("a", 1, 200), record("b", 2, 500)]);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn filters_match_status_classes() {
        let ok = record("login", 1, 200);
        let redirect = record("login", 2, 302);
        let broken = record("login", 3, 500);
        let dead = record("login", 4, 0); // transport error

        assert!(matches(&ok, None, &HistoryFilter::Success));
        assert!(!matches(&redirect, None, &HistoryFilter::Success));
        assert!(matches(&broken, None, &HistoryFilter::Failed));
        assert!(matches(&dead, None, &HistoryFilter::Failed));
        assert!(matches(&broken, None, &HistoryFilter::Status(500)));
        assert!(!matches(&broken, None, &HistoryFilter::Status(501)));
        assert!(matches(&ok, Some("login"), &HistoryFilter::All));
        assert!(!matches(&ok, Some("me"), &HistoryFilter::All));
    }

    #[test]
    fn records_survive_a_jsonl_roundtrip() {
        let line = serde_json::to_string(&record("login", 42, 200)).unwrap();
        let back: Record = serde_json::from_str(&line).unwrap();
        assert_eq!(back.id, "login");
        assert_eq!(back.ts, 42);
        assert!(back.error.is_none());
        assert!(!line.contains("error")); // None is omitted, not serialized
    }

    #[test]
    fn shape_summarizes_structure_not_values() {
        let body = r#"{"token": "abcdefgh", "user": {"id": 7, "roles": ["admin", "dev"]}, "ok": true, "score": 1.5, "gone": null}"#;
        assert_eq!(
            shape_of(body),
            "{token: str(8), user: {id: int, roles: [2 × str(5)]}, ok: bool, score: float, gone: null}"
        );
        assert_eq!(shape_of("[]"), "[]");
        assert_eq!(shape_of("not json at all"), "text(15)");
        assert_eq!(shape_of(""), "(empty body)");
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(0), "0b");
        assert_eq!(human_size(1023), "1023b");
        assert_eq!(human_size(1536), "1.5kb");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0mb");
    }
}
