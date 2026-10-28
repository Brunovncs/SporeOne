//! Whether a newer SporeOne is out, asked of GitHub's latest release, and fetching and starting
//! its installer, which updates the installation in place.

use crate::net;
use std::path::{Path, PathBuf};

pub const RELEASES_API: &str = "https://api.github.com/repos/Brunovncs/SporeOne/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/Brunovncs/SporeOne/releases/latest";

/// The latest release, as far as an update needs to know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// Its page on GitHub, with the notes and every download.
    pub page: String,
    /// The Windows installer and the file holding its SHA-256, when the release has them.
    pub setup: Option<(String, String)>,
}

/// This version, or `SPOREONE_PRETEND_VERSION` to try updating from an older one.
pub fn current() -> String {
    std::env::var("SPOREONE_PRETEND_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// A part of a pre-release name, compared as semantic versioning does: numbers by value and
/// before words.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Number(u64),
    Word(String),
}

/// A version in the order releases come in: its numbers without the zeros that end them (1.0 is
/// 1.0.0), then whether it is final rather than a pre-release, then the pre-release's parts. Build
/// metadata (`+...`) is left out.
fn version(v: &str) -> Option<(Vec<u64>, bool, Vec<Part>)> {
    let v = v.trim();
    let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
    let v = v.split('+').next().unwrap_or(v);
    let (numbers, pre) = v.split_once('-').map_or((v, None), |(n, p)| (n, Some(p)));
    let mut numbers = numbers.split('.').map(|n| n.parse().ok()).collect::<Option<Vec<u64>>>()?;
    while numbers.last() == Some(&0) {
        numbers.pop();
    }
    let pre: Vec<Part> =
        pre.map_or_else(Vec::new, |p| p.split('.').map(|w| w.parse().map_or_else(|_| Part::Word(w.into()), Part::Number)).collect());
    Some((numbers, pre.is_empty(), pre))
}

/// `candidate` is a later version than `current`: 1.10.0 after 1.9.2, 0.9.0 after 0.9.0-rc1.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    version(candidate).is_some_and(|a| Some(a) > version(current))
}

/// The release in GitHub's answer, with its x64 installer when it has one.
pub fn parse(json: &[u8]) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("the release has no tag")?;
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    let url_of = |suffix: &str| {
        assets
            .iter()
            .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(suffix)))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(String::from)
    };
    Ok(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        page: v["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string(),
        setup: url_of("-windows-x64-setup.exe").zip(url_of("-windows-x64-setup.exe.sha256")),
    })
}

/// The latest release, if it is newer than this one. A repository without releases yet has
/// nothing newer. `SPOREONE_RELEASES_URL` asks another address instead of GitHub's.
pub fn check() -> Result<Option<Release>, String> {
    let url = std::env::var("SPOREONE_RELEASES_URL").unwrap_or_else(|_| RELEASES_API.to_string());
    let json = match net::get(&url, &["Accept: application/vnd.github+json".into()]) {
        Ok(j) => j,
        Err(e) if e.contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
    let r = parse(&json)?;
    Ok(is_newer(&r.version, &current()).then_some(r))
}

/// Whether this copy came from the installer, which leaves its uninstaller next to it. A copy
/// unpacked from the zip cannot be updated in place, so it is sent to the release's page.
pub fn installed_by_setup() -> bool {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("unins000.exe").is_file())).unwrap_or(false)
}

/// The SHA-256 in a `.sha256` file: the first word of `<hex>  <name>`.
fn checksum(text: &[u8]) -> Option<String> {
    let sum = String::from_utf8_lossy(text).split_whitespace().next()?.to_lowercase();
    (sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit())).then_some(sum)
}

/// Downloads the release's installer into `updates` in SporeOne's data folder and checks it
/// against the SHA-256 published with it, so a broken or swapped download never runs. Installers
/// of other releases left there are removed.
pub fn download_setup(r: &Release) -> Result<PathBuf, String> {
    let (url, sha_url) = r.setup.as_ref().ok_or("this release has no installer")?;
    let expected = checksum(&net::get(sha_url, &[])?).ok_or("the installer's checksum could not be read")?;
    let name = url.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("sporeone-setup.exe").to_string();
    let dir = crate::state::data_dir().join("updates");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten().filter(|e| e.file_name() != *name) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let file = dir.join(&name);
    if net::file_sha256(&file).as_deref() == Some(expected.as_str()) {
        return Ok(file);
    }
    let file = net::download(url, &dir, &name, None)?;
    if net::file_sha256(&file).as_deref() != Some(expected.as_str()) {
        let _ = std::fs::remove_file(&file);
        return Err("the downloaded installer does not match its checksum".into());
    }
    Ok(file)
}

/// Starts the installer without its wizard, only its progress window. It closes SporeOne if it
/// is still running and replaces it in place; the downloads and the catalog stay.
#[cfg(windows)]
pub fn run_setup(file: &Path) -> Result<(), String> {
    let args = "/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS";
    match crate::win::shell_run(file, args, file.parent(), false, false) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err("the installer was cancelled".into()),
        Err(e) => Err(format!("could not start the installer: {e}")),
    }
}

#[cfg(not(windows))]
pub fn run_setup(_: &Path) -> Result<(), String> {
    Err("updates install themselves only on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("0.0.2", "0.0.1"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.99.0"));
        assert!(!is_newer("0.0.1", "0.0.1"));
        assert!(!is_newer("0.0.1", "0.1.0"));
        assert!(!is_newer("garbage", "0.0.1"));
        assert!(!is_newer("1.0.0", "1.0"));
        assert!(!is_newer("V1.0.0", "v1.0.0"));
        assert!(!is_newer("1.0.0+build.5", "1.0.0"), "build metadata is left out");
    }

    #[test]
    fn pre_releases_come_before_their_final() {
        assert!(is_newer("0.9.0", "0.9.0-rc1"));
        assert!(!is_newer("0.9.0-rc1", "0.9.0"));
        assert!(is_newer("0.9.0-rc.10", "0.9.0-rc.9"), "numbers by value");
        assert!(is_newer("0.9.0-rc1", "0.8.8"));
    }

    #[test]
    fn reads_a_release() {
        let json = br#"{"tag_name":"v0.0.2","html_url":"https://example/v0.0.2","assets":[
            {"name":"sporeone-0.0.2-windows-x64.zip","browser_download_url":"https://example/zip"},
            {"name":"sporeone-0.0.2-windows-x64.zip.sha256","browser_download_url":"https://example/zip-sha"},
            {"name":"sporeone-0.0.2-windows-x64-setup.exe","browser_download_url":"https://example/setup"},
            {"name":"sporeone-0.0.2-windows-x64-setup.exe.sha256","browser_download_url":"https://example/sha"}]}"#;
        let r = parse(json).unwrap();
        assert_eq!(r.version, "0.0.2");
        assert_eq!(r.page, "https://example/v0.0.2");
        assert_eq!(r.setup, Some(("https://example/setup".into(), "https://example/sha".into())));
        let without = parse(br#"{"tag_name":"v0.0.2","assets":[]}"#).unwrap();
        assert_eq!(without.setup, None);
        assert_eq!(without.page, RELEASES_PAGE);
        assert!(parse(br#"{"message":"Not Found"}"#).is_err());
    }

    #[test]
    fn a_setup_without_its_checksum_is_left_out() {
        let json = br#"{"tag_name":"v0.0.2","assets":[
            {"name":"sporeone-0.0.2-windows-x64-setup.exe","browser_download_url":"https://example/setup"}]}"#;
        assert_eq!(parse(json).unwrap().setup, None);
    }

    #[test]
    fn reads_a_checksum_file() {
        let sum = "ab".repeat(32);
        assert_eq!(checksum(format!("{}  sporeone-0.0.2-windows-x64-setup.exe\r\n", sum.to_uppercase()).as_bytes()), Some(sum));
        assert_eq!(checksum(b"not a checksum"), None);
        assert_eq!(checksum(b""), None);
    }
}
