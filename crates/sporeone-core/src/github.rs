//! A mod from its GitHub repository: what the repository says about itself, its `ModInfo.xml`,
//! and the `.sporemod` or `.package` of its latest release, as a catalog entry. The catalog's
//! builder makes one for every repository it lists, and SporeOne makes one for a link pasted in.

use crate::catalog::{Category, Entry, Kind, Release, Status};
use crate::{modinfo, net};
use serde_json::Value;

const API: &str = "https://api.github.com";
/// SporeOne's own repository, where mods are suggested.
pub const HOME: &str = "Brunovncs/SporeOne";
/// The form for suggesting a mod, opened by the app's "Suggest a mod" button.
pub const SUGGEST_URL: &str = "https://github.com/Brunovncs/SporeOne/issues/new?template=suggest-mod.yml";
/// A mod not released in this long is marked old.
const OLD_AFTER_DAYS: i64 = 4 * 365;
/// Files larger than this are listed without a hash rather than downloaded to hash them.
const HASH_LIMIT: u64 = 200 * 1024 * 1024;

fn headers(raw: bool, token: Option<&str>) -> Vec<String> {
    let mut h = vec![
        format!("Accept: application/vnd.github{}", if raw { ".raw" } else { "+json" }),
        "X-GitHub-Api-Version: 2022-11-28".to_string(),
    ];
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        h.push(format!("Authorization: Bearer {t}"));
    }
    h
}

/// A GitHub API answer, `path` relative to `https://api.github.com/`.
pub fn api(path: &str, token: Option<&str>) -> Result<Value, String> {
    let bytes = net::get(&format!("{API}/{path}"), &headers(false, token))?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// The HTTP status in curl's error for a failed request, when it gives one.
fn status(error: &str) -> Option<u16> {
    let (_, rest) = error.rsplit_once("error: ")?;
    rest.get(..3)?.parse().ok()
}

/// An API error as someone who pasted a link can read it.
fn readable(error: &str, repo: &str) -> String {
    match status(error) {
        Some(404) => format!("{repo} is not a public GitHub repository"),
        Some(403 | 429) => "GitHub is not taking more requests from this computer right now. Try again in an hour.".to_string(),
        _ => format!("Could not reach GitHub: {error}"),
    }
}

/// `owner/repo` from a repository given as `owner/repo` or as a link to it or to anything in it
/// (`https://github.com/owner/repo/releases/latest`, `github.com/owner/repo.git`,
/// `git@github.com:owner/repo.git`).
pub fn parse_repo(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches(['/', '.', ',', ')']);
    let lower = s.to_ascii_lowercase();
    let path = ["https://", "http://", ""]
        .iter()
        .find_map(|scheme| {
            let rest = lower.strip_prefix(scheme)?;
            let host = ["www.github.com/", "github.com/"].iter().find(|h| rest.starts_with(**h))?;
            Some(&s[scheme.len() + host.len()..])
        })
        .or_else(|| lower.strip_prefix("git@github.com:").map(|_| &s["git@github.com:".len()..]));
    let (owner, repo) = match path {
        Some(p) => {
            let p = p.split(['?', '#']).next().unwrap_or("");
            let mut parts = p.split('/');
            (parts.next()?, parts.next()?)
        }
        None => {
            let (owner, repo) = s.split_once('/')?;
            if repo.contains('/') {
                return None;
            }
            (owner, repo)
        }
    };
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let owner_ok = (1..=39).contains(&owner.len())
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !owner.starts_with('-')
        && !RESERVED.contains(&owner.to_ascii_lowercase().as_str());
    let repo_ok = (1..=100).contains(&repo.len())
        && repo.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && repo != "."
        && repo != "..";
    (owner_ok && repo_ok).then(|| format!("{owner}/{repo}"))
}

/// First parts of github.com paths that are not users.
const RESERVED: [&str; 14] = [
    "about",
    "apps",
    "collections",
    "explore",
    "features",
    "issues",
    "login",
    "marketplace",
    "notifications",
    "orgs",
    "pulls",
    "search",
    "settings",
    "topics",
];

/// Days from 1970 to a `2026-10-27T...` date, enough to compare two of them.
fn days(date: &str) -> Option<i64> {
    let (y, rest) = date.split_once('-')?;
    let (m, rest) = rest.split_once('-')?;
    let d = rest.get(..2)?;
    let (y, m, d): (i64, i64, i64) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    // Days from civil, after Howard Hinnant.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Now, as an ISO 8601 date and time in UTC.
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let t = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}

/// In a release's assets, the one named `wanted`, else the first `.sporemod` or `.package`.
fn asset<'a>(release: &'a Value, wanted: Option<&str>) -> Option<&'a Value> {
    release["assets"].as_array()?.iter().find(|a| {
        let n = a["name"].as_str().unwrap_or("").to_lowercase();
        match wanted {
            Some(w) => n == w.to_lowercase(),
            None => n.ends_with(".sporemod") || n.ends_with(".package"),
        }
    })
}

/// The latest release's file, `None` when there is no release or no such file in it.
fn release_of(full: &str, wanted: Option<&str>, token: Option<&str>, hash: bool) -> Result<Option<Release>, String> {
    let r = match api(&format!("repos/{full}/releases/latest"), token) {
        Ok(r) => r,
        Err(e) if status(&e) == Some(404) => return Ok(None),
        Err(e) => return Err(e),
    };
    let Some(asset) = asset(&r, wanted) else { return Ok(None) };
    let (Some(url), Some(file)) = (asset["browser_download_url"].as_str(), asset["name"].as_str()) else { return Ok(None) };
    let size = asset["size"].as_u64().unwrap_or(0);
    let (sha256, resources) = if hash && size <= HASH_LIMIT {
        let dir = std::env::temp_dir().join("sporeone-github").join(full.replace('/', "_"));
        let got = net::download(url, &dir, file, None).ok();
        let sum = got.as_deref().and_then(net::file_sha256);
        let resources = got.as_deref().and_then(|f| crate::dbpf::resources_of_mod(f).ok()).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        (sum, resources)
    } else {
        (None, Vec::new())
    };
    Ok(Some(Release {
        tag: r["tag_name"].as_str().unwrap_or("").to_string(),
        published: r["published_at"].as_str().unwrap_or("").to_string(),
        url: url.to_string(),
        file: file.to_string(),
        size,
        downloads: asset["download_count"].as_u64().unwrap_or(0),
        sha256,
        resources,
    }))
}

/// The entry for a repository whose API answer (`repos/owner/repo`, or an item of a search) is
/// `meta`, with `setup` the release file to pick instead of the first `.sporemod` or `.package`.
/// Its `ModInfo.xml` and release are left out when they cannot be read. About two requests, and a
/// download when `hash`.
pub fn entry_of(meta: &Value, setup: Option<&str>, token: Option<&str>, hash: bool) -> Option<Entry> {
    let full = meta["full_name"].as_str()?;
    let release = release_of(full, setup, token, hash).ok().flatten();
    Some(assemble(meta, full, release, token))
}

fn assemble(meta: &Value, full: &str, release: Option<Release>, token: Option<&str>) -> Entry {
    let info =
        net::get(&format!("{API}/repos/{full}/contents/ModInfo.xml"), &headers(true, token)).ok().and_then(|b| modinfo::parse(&b).ok());
    let repo_desc = meta["description"].as_str().unwrap_or("").trim().to_string();
    let name =
        info.as_ref().map(|i| i.display_name.clone()).unwrap_or_else(|| meta["name"].as_str().unwrap_or(full).replace(['-', '_'], " "));
    let description = info.as_ref().and_then(|i| i.description.clone()).unwrap_or_else(|| repo_desc.clone());
    let pushed = meta["pushed_at"].as_str().unwrap_or("").to_string();
    let latest = release.as_ref().map(|r| r.published.clone()).unwrap_or_else(|| pushed.clone());
    let age = days(&today()).zip(days(&latest)).map_or(0, |(a, b)| a - b);
    let status = if meta["archived"].as_bool() == Some(true) || age > OLD_AFTER_DAYS {
        Status::Old
    } else if info.as_ref().is_some_and(|i| i.experimental) {
        Status::Experimental
    } else {
        Status::Maintained
    };
    let homepage = meta["homepage"].as_str().filter(|h| h.starts_with("http")).map(String::from);
    Entry {
        id: full.to_string(),
        category: Category::guess(&format!("{name} {description} {repo_desc}")),
        name,
        author: meta["owner"]["login"].as_str().unwrap_or("").to_string(),
        description,
        kind: Kind::Release,
        status,
        page: format!("https://github.com/{full}"),
        homepage,
        info,
        release,
        stars: meta["stargazers_count"].as_u64().unwrap_or(0) as u32,
        updated: pushed,
        ..Default::default()
    }
}

/// The entry for a repository given as `owner/repo` or a link to it, for installing a mod that
/// is not in the catalog. Three requests, which GitHub allows twenty times an hour without a
/// token. Fails, in words for whoever pasted the link, when there is nothing to install.
pub fn entry(repo: &str, token: Option<&str>, hash: bool) -> Result<Entry, String> {
    let Some(full) = parse_repo(repo) else {
        return Err(format!("{} is not a link to a GitHub repository", repo.trim()));
    };
    let meta = api(&format!("repos/{full}"), token).map_err(|e| readable(&e, &full))?;
    let full = meta["full_name"].as_str().unwrap_or(&full).to_string();
    let release = release_of(&full, None, token, hash).map_err(|e| readable(&e, &full))?;
    if release.is_none() {
        return Err(format!("{full} has no release with a .sporemod or .package file"));
    }
    Ok(assemble(&meta, &full, release, token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn repositories_from_links() {
        let want = Some("Owner-1/Some.Mod_2".to_string());
        for input in [
            "Owner-1/Some.Mod_2",
            "  Owner-1/Some.Mod_2\n",
            "https://github.com/Owner-1/Some.Mod_2",
            "https://github.com/Owner-1/Some.Mod_2/",
            "https://github.com/Owner-1/Some.Mod_2.git",
            "http://www.github.com/Owner-1/Some.Mod_2",
            "HTTPS://GitHub.com/Owner-1/Some.Mod_2",
            "github.com/Owner-1/Some.Mod_2",
            "https://github.com/Owner-1/Some.Mod_2/releases/latest",
            "https://github.com/Owner-1/Some.Mod_2/releases/download/v1/mod.sporemod",
            "https://github.com/Owner-1/Some.Mod_2/tree/main/src",
            "https://github.com/Owner-1/Some.Mod_2?tab=readme-ov-file",
            "https://github.com/Owner-1/Some.Mod_2#readme",
            "git@github.com:Owner-1/Some.Mod_2.git",
        ] {
            assert_eq!(parse_repo(input), want, "{input:?}");
        }
    }

    #[test]
    fn not_repositories() {
        for input in [
            "",
            "spore",
            "owner/",
            "/repo",
            "a/b/c",
            "https://github.com/owner",
            "https://github.com/",
            "https://gitlab.com/owner/repo",
            "https://notgithub.com/owner/repo",
            "https://www.nexusmods.com/spore/mods/1",
            "https://github.com/topics/spore-mod",
            "https://github.com/orgs/someone/repositories",
            "owner name/repo",
            "-owner/repo",
            "owner/..",
            "owner/re po",
        ] {
            assert_eq!(parse_repo(input), None, "{input:?}");
        }
    }

    #[test]
    fn statuses_from_curl() {
        assert_eq!(status("curl: (22) The requested URL returned error: 404"), Some(404));
        assert_eq!(status("curl: (22) The requested URL returned error: 403 rate limit"), Some(403));
        assert_eq!(status("curl: (6) Could not resolve host: api.github.com"), None);
        assert_eq!(readable("curl: (22) The requested URL returned error: 404", "a/b"), "a/b is not a public GitHub repository");
        assert!(readable("curl: (22) The requested URL returned error: 429", "a/b").contains("Try again"));
    }

    #[test]
    fn picks_the_release_file() {
        let r = json!({ "assets": [
            { "name": "Source.zip" },
            { "name": "Setup.EXE" },
            { "name": "Mod.Package" },
            { "name": "mod.sporemod" },
        ]});
        assert_eq!(asset(&r, None).unwrap()["name"], "Mod.Package");
        assert_eq!(asset(&r, Some("setup.exe")).unwrap()["name"], "Setup.EXE");
        assert!(asset(&r, Some("other.exe")).is_none());
        assert!(asset(&json!({ "assets": [{ "name": "a.zip" }] }), None).is_none());
        assert!(asset(&json!({}), None).is_none());
    }

    #[test]
    fn counts_days() {
        assert_eq!(days("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(days("2026-10-27T10:00:00Z").zip(days("2025-10-27")).map(|(a, b)| a - b), Some(365));
        assert_eq!(days(&today()).map(|d| d > 20_000), Some(true));
    }

    /// `cargo test -p sporeone-core github::tests::a_real_repository -- --ignored`
    #[test]
    #[ignore = "asks GitHub"]
    fn a_real_repository() {
        let e = entry("https://github.com/Rosalie241/SporeCrashFix/releases", None, false).unwrap();
        eprintln!("{e:#?}");
        assert_eq!(e.id, "Rosalie241/SporeCrashFix");
        assert!(e.info.is_some() && e.release.as_ref().is_some_and(|r| r.file.ends_with(".sporemod")));
        // No ModInfo.xml at its root: named after the repository.
        let e = entry("Zarklord/Infinite-Part-Scaling", None, false).unwrap();
        assert!(e.info.is_none() && e.release.is_some());
        assert_eq!(e.name, "Infinite Part Scaling");
        let e = entry("Brunovncs/SporeOne", None, false).unwrap_err();
        assert_eq!(e, "Brunovncs/SporeOne has no release with a .sporemod or .package file");
        let e = entry("Brunovncs/no-such-repository", None, false).unwrap_err();
        assert_eq!(e, "Brunovncs/no-such-repository is not a public GitHub repository");
    }
}
