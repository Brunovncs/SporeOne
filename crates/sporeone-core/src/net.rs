//! Downloads, through `curl`: it ships with Windows 10 and later and with every Linux, so no TLS
//! stack is built into SporeOne. Only HTTPS, redirects included.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

fn curl() -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let exe = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| "C:\\Windows".into()).join("System32\\curl.exe");
        let mut c = Command::new(exe);
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }
    #[cfg(not(windows))]
    Command::new("curl")
}

const SAFE: [&str; 8] = ["--silent", "--show-error", "--fail", "--location", "--proto", "=https", "--proto-redir", "=https"];

fn run(mut c: Command) -> Result<Vec<u8>, String> {
    let out = c.output().map_err(|e| format!("could not run curl: {e}"))?;
    if out.status.success() { Ok(out.stdout) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// A small answer, all of it within 30 seconds. `headers` are added as given.
pub fn get(url: &str, headers: &[String]) -> Result<Vec<u8>, String> {
    let mut c = curl();
    c.args(SAFE).args(["--max-time", "30", "--user-agent", "SporeOne"]);
    for h in headers {
        c.args(["--header", h]);
    }
    c.arg(url);
    run(c)
}

/// A file, into `dir`. Given up on when it stalls rather than after a set time, and checked
/// against `sha256` when the catalog knows it, so a swapped file is never installed.
pub fn download(url: &str, dir: &Path, name: &str, sha256: Option<&str>) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(name);
    if let Some(sum) = sha256
        && file_sha256(&file).as_deref() == Some(sum)
    {
        return Ok(file);
    }
    let part = dir.join(format!("{name}.part"));
    let mut c = curl();
    c.args(SAFE)
        .args(["--retry", "2", "--connect-timeout", "15", "--speed-limit", "1024", "--speed-time", "30", "--user-agent", "SporeOne"])
        .arg("--output")
        .arg(&part)
        .arg(url);
    run(c)?;
    if let Some(sum) = sha256
        && file_sha256(&part).as_deref() != Some(sum)
    {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{name} is not the file the catalog lists"));
    }
    std::fs::rename(&part, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn file_sha256(file: &Path) -> Option<String> {
    std::fs::read(file).ok().map(|b| sha256_hex(&b))
}
