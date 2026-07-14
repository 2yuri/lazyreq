//! `lazyreq update` — self-update from the latest GitHub release.

use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;

const REPO: &str = "2yuri/lazyreq";

async fn latest_tag(client: &reqwest::Client) -> Result<String, String> {
    let release: Value = client
        .get(format!("https://api.github.com/repos/{}/releases/latest", REPO))
        .header("User-Agent", "lazyreq")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("cannot check for updates: {}", e))?
        .json()
        .await
        .map_err(|e| format!("unexpected response from GitHub: {}", e))?;

    release["tag_name"]
        .as_str()
        .map(|t| t.to_string())
        .ok_or("unexpected response from GitHub: no tag_name".to_string())
}

const CHECK_TTL_SECS: u64 = 6 * 60 * 60;

fn check_cache_path() -> Option<std::path::PathBuf> {
    crate::vault::lazyreq_dir().ok().map(|d| d.join("update-check"))
}

/// Cache format: `unix-timestamp\nlatest-version`.
fn parse_check_cache(raw: &str, now: u64) -> Option<String> {
    let (ts, version) = raw.trim().split_once('\n')?;
    let ts: u64 = ts.trim().parse().ok()?;
    (now.saturating_sub(ts) < CHECK_TTL_SECS && !version.trim().is_empty())
        .then(|| version.trim().to_string())
}

fn cached_latest() -> Option<String> {
    let raw = fs::read_to_string(check_cache_path()?).ok()?;
    parse_check_cache(&raw, crate::timest::get_timestamp())
}

fn store_latest(version: &str) {
    if let Some(path) = check_cache_path() {
        let _ = fs::write(path, format!("{}\n{}", crate::timest::get_timestamp(), version));
    }
}

/// Quiet check for the TUI: Some(version) when a newer release exists,
/// None on same version or any failure (offline, rate-limited, ...).
/// GitHub is asked at most once every CHECK_TTL_SECS; the result is cached
/// in ~/.lazyreq/update-check.
pub async fn newer_version() -> Option<String> {
    let latest = match cached_latest() {
        Some(version) => version,
        None => {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(4))
                .build()
                .ok()?;
            let tag = latest_tag(&client).await.ok()?;
            let version = tag.trim_start_matches('v').to_string();
            store_latest(&version);
            version
        }
    };
    (latest != env!("CARGO_PKG_VERSION")).then_some(latest)
}

pub async fn self_update() -> Result<(), String> {
    let current = env!("CARGO_PKG_VERSION");
    let client = reqwest::Client::new();

    let tag = latest_tag(&client).await?;
    let latest = tag.trim_start_matches('v');

    if latest == current {
        println!("lazyreq {} is already the latest version", current);
        return Ok(());
    }

    let asset = asset_name().ok_or(format!(
        "self-update is not supported on this platform — grab a build from https://github.com/{}/releases/latest",
        REPO
    ))?;
    let base = format!("https://github.com/{}/releases/download/{}", REPO, tag);

    println!("updating lazyreq {} → {}...", current, latest);
    let archive = client
        .get(format!("{}/{}", base, asset))
        .header("User-Agent", "lazyreq")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("download failed: {}", e))?;

    let checksums = client
        .get(format!("{}/lazyreq_{}_checksums.txt", base, latest))
        .header("User-Agent", "lazyreq")
        .send()
        .await
        .and_then(|r| r.error_for_status());
    match checksums {
        Ok(response) => {
            let sums = response.text().await.map_err(|e| e.to_string())?;
            let expected = expected_sum(&sums, asset)
                .ok_or(format!("no checksum listed for {}", asset))?;
            let actual = hex::encode(Sha256::digest(&archive));
            if expected != actual {
                return Err(format!("checksum mismatch for {} — aborting", asset));
            }
        }
        Err(_) => eprintln!("warning: checksums not found, skipping verification"),
    }

    let binary = extract_binary(&archive)?;
    replace_current_exe(&binary)?;
    store_latest(latest);
    println!("updated to lazyreq {}", latest);
    Ok(())
}

fn asset_name() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("lazyreq_Darwin_all.tar.gz")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Some("lazyreq_Linux_x86_64.tar.gz")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Some("lazyreq_Linux_arm64.tar.gz")
    } else {
        None
    }
}

fn expected_sum(checksums: &str, asset: &str) -> Option<String> {
    checksums
        .lines()
        .find(|l| l.ends_with(asset))
        .and_then(|l| l.split_whitespace().next())
        .map(|s| s.to_string())
}

fn extract_binary(gz: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = tar::Archive::new(GzDecoder::new(gz));
    for entry in archive
        .entries()
        .map_err(|e| format!("cannot read archive: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("cannot read archive: {}", e))?;
        let is_lazyreq = entry
            .path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n == "lazyreq"))
            .unwrap_or(false);
        if is_lazyreq {
            let mut binary = Vec::new();
            entry
                .read_to_end(&mut binary)
                .map_err(|e| format!("cannot read archive: {}", e))?;
            return Ok(binary);
        }
    }
    Err("the release archive did not contain a lazyreq binary".to_string())
}

/// Writes next to the current executable, then renames over it — atomic on
/// the same filesystem, and safe while this process is still running.
fn replace_current_exe(binary: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate lazyreq: {}", e))?;
    let staged = exe.with_extension("update");

    fs::write(&staged, binary).map_err(|e| format!("cannot write `{}`: {}", staged.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("cannot set permissions: {}", e))?;
    }
    fs::rename(&staged, &exe).map_err(|e| {
        let _ = fs::remove_file(&staged);
        format!("cannot replace `{}`: {}", exe.display(), e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    #[test]
    fn this_platform_has_an_asset() {
        assert!(asset_name().is_some());
    }

    #[test]
    fn check_cache_honors_ttl() {
        let now = 1_784_059_765;
        let fresh = format!("{}\n0.3.0", now - 100);
        let stale = format!("{}\n0.3.0", now - CHECK_TTL_SECS - 1);

        assert_eq!(parse_check_cache(&fresh, now).as_deref(), Some("0.3.0"));
        assert!(parse_check_cache(&stale, now).is_none());
        assert!(parse_check_cache("garbage", now).is_none());
        assert!(parse_check_cache(&format!("{}\n", now), now).is_none());
    }

    #[test]
    fn checksum_lookup_matches_the_right_line() {
        let sums = "abc123  lazyreq_Darwin_all.tar.gz\ndef456  lazyreq_Linux_x86_64.tar.gz\n";
        assert_eq!(
            expected_sum(sums, "lazyreq_Darwin_all.tar.gz").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            expected_sum(sums, "lazyreq_Linux_x86_64.tar.gz").as_deref(),
            Some("def456")
        );
        assert!(expected_sum(sums, "lazyreq_Windows_x86_64.zip").is_none());
    }

    #[test]
    fn extracts_the_lazyreq_binary_from_a_tarball() {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (path, data) in [("README.md", b"docs".as_slice()), ("lazyreq", b"\x7fELF fake binary")] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, data).unwrap();
        }
        let gz = builder.into_inner().unwrap().finish().unwrap();

        assert_eq!(extract_binary(&gz).unwrap(), b"\x7fELF fake binary");
        assert!(extract_binary(b"not a tarball").is_err());
    }
}
