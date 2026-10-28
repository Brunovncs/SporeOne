//! A mod's README from its GitHub repository, for its page. Read from raw.githubusercontent.com,
//! which has no API rate limit, and kept on disk for when the network is not there.

use crate::net;
use std::path::PathBuf;

const NAMES: [&str; 4] = ["README.md", "readme.md", "Readme.md", "README"];

/// `owner/repo` as a file name, or `None` when it is not a GitHub repository's name.
fn file_name(repo: &str) -> Option<String> {
    let (owner, name) = repo.split_once('/')?;
    let fits = |s: &str| !s.is_empty() && s != "." && s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (fits(owner) && fits(name)).then(|| format!("{owner}@{name}.md"))
}

fn cache_file(repo: &str) -> Option<PathBuf> {
    Some(crate::state::data_dir().join("readmes").join(file_name(repo)?))
}

/// The README kept from the last time it was downloaded.
pub fn cached(repo: &str) -> Option<String> {
    std::fs::read_to_string(cache_file(repo)?).ok()
}

/// Downloads `repo`'s README and keeps it, or gives the one kept when GitHub cannot be reached.
pub fn fetch(repo: &str) -> Result<String, String> {
    let file = cache_file(repo).ok_or_else(|| format!("{repo} is not a GitHub repository"))?;
    let mut error = String::new();
    for name in NAMES {
        match net::get(&format!("https://raw.githubusercontent.com/{repo}/HEAD/{name}"), &[]) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&file, &text);
                return Ok(text);
            }
            // Another name only when this one is missing; otherwise the network is the trouble.
            Err(e) if e.contains("error: 404") => error = e,
            Err(e) => {
                error = e;
                break;
            }
        }
    }
    cached(repo).ok_or(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(file_name("Rosalie241/SporeCrashFix").as_deref(), Some("Rosalie241@SporeCrashFix.md"));
        assert_eq!(file_name("a-b/c_d.e").as_deref(), Some("a-b@c_d.e.md"));
        assert_eq!(file_name("NoSlash"), None);
        assert_eq!(file_name("a/.."), None);
        assert_eq!(file_name("a/b/c"), None);
        assert_eq!(file_name("/b"), None);
    }

    /// Downloads a real README into the cache.
    #[test]
    #[ignore]
    fn fetches_a_real_readme() {
        let text = fetch("Rosalie241/SporeCrashFix").unwrap();
        assert!(!text.trim().is_empty());
        assert_eq!(cached("Rosalie241/SporeCrashFix").as_deref(), Some(text.as_str()));
    }
}
