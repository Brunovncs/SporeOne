//! Builds `catalog.json`: every GitHub repository tagged `spore-mod`, as the Spore Mod Browser
//! finds them, with its `ModInfo.xml` and the `.sporemod` or `.package` of its latest release
//! (hashed, so SporeOne can check its download), then `catalog/curated.json` laid over it.
//!
//! `sporeone-catalog [curated.json] [out.json]`, with a token in `GITHUB_TOKEN` or `GH_TOKEN`:
//! GitHub allows 60 requests an hour without one, and this makes about three per repository.

use serde_json::{Value, json};
use sporeone_core::catalog::{Category, Entry, Kind, Release, Status};
use sporeone_core::{modinfo, net};
use std::sync::Mutex;

const API: &str = "https://api.github.com";
/// A mod not released in this long is marked old.
const OLD_AFTER_DAYS: i64 = 4 * 365;
/// Files larger than this are listed without a hash rather than downloaded here.
const HASH_LIMIT: u64 = 200 * 1024 * 1024;

fn headers(raw: bool) -> Vec<String> {
    let mut h = vec![
        format!("Accept: application/vnd.github{}", if raw { ".raw" } else { "+json" }),
        "X-GitHub-Api-Version: 2022-11-28".to_string(),
    ];
    if let Some(t) = std::env::var("GITHUB_TOKEN").ok().or_else(|| std::env::var("GH_TOKEN").ok()).filter(|t| !t.is_empty()) {
        h.push(format!("Authorization: Bearer {t}"));
    }
    h
}

fn get_json(path: &str) -> Result<Value, String> {
    let bytes = net::get(&format!("{API}/{path}"), &headers(false))?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn search() -> Result<Vec<Value>, String> {
    let mut repos = Vec::new();
    for page in 1..=10 {
        let v = get_json(&format!("search/repositories?q=topic:spore-mod&per_page=100&page={page}"))?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        let done = items.len() < 100;
        repos.extend(items);
        if done {
            break;
        }
    }
    Ok(repos)
}

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

fn today() -> String {
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

/// The release's file: the one named `wanted`, else the first `.sporemod` or `.package`.
fn release_of(full: &str, wanted: Option<&str>) -> Option<Release> {
    let r = get_json(&format!("repos/{full}/releases/latest")).ok()?;
    let assets = r["assets"].as_array()?;
    let asset = assets.iter().find(|a| {
        let n = a["name"].as_str().unwrap_or("").to_lowercase();
        match wanted {
            Some(w) => n == w.to_lowercase(),
            None => n.ends_with(".sporemod") || n.ends_with(".package"),
        }
    })?;
    let url = asset["browser_download_url"].as_str()?.to_string();
    let size = asset["size"].as_u64().unwrap_or(0);
    let file = asset["name"].as_str()?.to_string();
    let sha256 = if size <= HASH_LIMIT {
        let dir = std::env::temp_dir().join("sporeone-catalog").join(full.replace('/', "_"));
        let got = net::download(&url, &dir, &file, None).ok().and_then(|f| net::file_sha256(&f));
        let _ = std::fs::remove_dir_all(&dir);
        got
    } else {
        None
    };
    Some(Release {
        tag: r["tag_name"].as_str().unwrap_or("").to_string(),
        published: r["published_at"].as_str().unwrap_or("").to_string(),
        url,
        file,
        size,
        downloads: asset["download_count"].as_u64().unwrap_or(0),
        sha256,
    })
}

fn entry_of(repo: &Value, curated: Option<&Value>) -> Option<Entry> {
    let full = repo["full_name"].as_str()?.to_string();
    let wanted = curated.and_then(|c| c["setup"].as_str());
    let info = net::get(&format!("{API}/repos/{full}/contents/ModInfo.xml"), &headers(true)).ok().and_then(|b| modinfo::parse(&b).ok());
    let release = release_of(&full, wanted);
    if info.is_none() && release.is_none() && curated.is_none() {
        return None;
    }
    let repo_desc = repo["description"].as_str().unwrap_or("").trim().to_string();
    let name =
        info.as_ref().map(|i| i.display_name.clone()).unwrap_or_else(|| repo["name"].as_str().unwrap_or(&full).replace(['-', '_'], " "));
    let description = info.as_ref().and_then(|i| i.description.clone()).unwrap_or_else(|| repo_desc.clone());
    let pushed = repo["pushed_at"].as_str().unwrap_or("").to_string();
    let latest = release.as_ref().map(|r| r.published.clone()).unwrap_or_else(|| pushed.clone());
    let age = days(&today()).zip(days(&latest)).map_or(0, |(a, b)| a - b);
    let status = if repo["archived"].as_bool() == Some(true) || age > OLD_AFTER_DAYS {
        Status::Old
    } else if info.as_ref().is_some_and(|i| i.experimental) {
        Status::Experimental
    } else {
        Status::Maintained
    };
    let homepage = repo["homepage"].as_str().filter(|h| h.starts_with("http")).map(String::from);
    Some(Entry {
        id: full.clone(),
        category: Category::guess(&format!("{name} {description} {repo_desc}")),
        name,
        author: repo["owner"]["login"].as_str().unwrap_or("").to_string(),
        description,
        kind: Kind::Release,
        status,
        page: format!("https://github.com/{full}"),
        homepage,
        info,
        release,
        stars: repo["stargazers_count"].as_u64().unwrap_or(0) as u32,
        updated: pushed,
        ..Default::default()
    })
}

/// The curated fields laid over the entry, key by key.
fn overlay(entry: Entry, curated: &Value) -> Result<Entry, String> {
    let mut v = serde_json::to_value(entry).map_err(|e| e.to_string())?;
    if let (Some(base), Some(top)) = (v.as_object_mut(), curated.as_object()) {
        for (k, val) in top {
            base.insert(k.clone(), val.clone());
        }
    }
    serde_json::from_value(v).map_err(|e| format!("{}: {e}", curated["id"]))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let curated_path = args.next().unwrap_or_else(|| "catalog/curated.json".into());
    let out_path = args.next().unwrap_or_else(|| "catalog/catalog.json".into());
    let curated: Vec<Value> = match std::fs::read_to_string(&curated_path).map(|s| serde_json::from_str(&s)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return fail(&format!("{curated_path}: {e}")),
        Err(e) => return fail(&format!("{curated_path}: {e}")),
    };
    let curated_of = |id: &str| curated.iter().find(|c| c["id"].as_str().is_some_and(|c| c.eq_ignore_ascii_case(id)));
    let mut repos = match search() {
        Ok(r) => r,
        Err(e) => return fail(&format!("searching GitHub: {e}")),
    };
    // Curated GitHub mods without the topic.
    for c in &curated {
        let Some(id) = c["id"].as_str().filter(|id| id.contains('/')) else { continue };
        if !repos.iter().any(|r| r["full_name"].as_str().is_some_and(|f| f.eq_ignore_ascii_case(id))) {
            match get_json(&format!("repos/{id}")) {
                Ok(r) => repos.push(r),
                Err(e) => eprintln!("{id}: {e}"),
            }
        }
    }
    eprintln!("{} repositories", repos.len());
    let entries = Mutex::new(Vec::new());
    let next = Mutex::new(0usize);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                loop {
                    let i = {
                        let mut n = next.lock().unwrap();
                        *n += 1;
                        *n - 1
                    };
                    let Some(repo) = repos.get(i) else { break };
                    let full = repo["full_name"].as_str().unwrap_or("");
                    let cur = curated_of(full);
                    if cur.is_some_and(|c| c["hidden"].as_bool() == Some(true)) {
                        continue;
                    }
                    let Some(e) = entry_of(repo, cur) else { continue };
                    let e = match cur {
                        Some(c) => match overlay(e, c) {
                            Ok(e) => e,
                            Err(err) => {
                                eprintln!("{err}");
                                continue;
                            }
                        },
                        None => e,
                    };
                    entries.lock().unwrap().push(e);
                }
            });
        }
    });
    let mut mods = entries.into_inner().unwrap();
    // Mods published elsewhere come from the curated list alone.
    for c in curated.iter().filter(|c| c["id"].as_str().is_some_and(|id| !id.contains('/')) && c["hidden"].as_bool() != Some(true)) {
        match overlay(Entry { kind: Kind::Page, ..Default::default() }, c) {
            Ok(e) => mods.push(e),
            Err(e) => eprintln!("{e}"),
        }
    }
    mods.sort_by(|a, b| b.downloads().cmp(&a.downloads()).then(b.stars.cmp(&a.stars)).then(a.name.cmp(&b.name)));
    let catalog = json!({ "generated": today(), "mods": mods });
    let text = serde_json::to_string_pretty(&catalog).unwrap_or_default() + "\n";
    if let Err(e) = std::fs::write(&out_path, text) {
        return fail(&format!("{out_path}: {e}"));
    }
    eprintln!("{} mods in {out_path}", mods.len());
}

fn fail(msg: &str) {
    eprintln!("sporeone-catalog: {msg}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_days() {
        assert_eq!(days("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(days("2026-10-27T10:00:00Z").zip(days("2025-10-27")).map(|(a, b)| a - b), Some(365));
        assert_eq!(days(&today()).map(|d| d > 20_000), Some(true));
    }
}
