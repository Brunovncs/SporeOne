//! Builds `catalog.json`: every GitHub repository tagged `spore-mod`, as the Spore Mod Browser
//! finds them, and every one suggested in an issue of SporeOne's repository that a maintainer
//! labeled `approved`, with its `ModInfo.xml` and the `.sporemod` or `.package` of its latest
//! release (hashed, so SporeOne can check its download), then `catalog/curated.json` laid over it.
//!
//! `sporeone-catalog [curated.json] [out.json]`, with a token in `GITHUB_TOKEN` or `GH_TOKEN`:
//! GitHub allows 60 requests an hour without one, and this makes about three per repository.

use serde_json::Value;
use sporeone_core::catalog::{Catalog, Entry, Kind};
use sporeone_core::github::{self, HOME, api, parse_repo, today};
use std::sync::Mutex;

fn search(token: Option<&str>) -> Result<Vec<Value>, String> {
    let mut repos = Vec::new();
    for page in 1..=10 {
        let v = api(&format!("search/repositories?q=topic:spore-mod&per_page=100&page={page}"), token)?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        let done = items.len() < 100;
        repos.extend(items);
        if done {
            break;
        }
    }
    Ok(repos)
}

/// The repositories suggested in issues labeled both `mod-suggestion` and `approved`, open or
/// closed. Only maintainers can label issues, so the labels are the gate.
fn approved(token: Option<&str>) -> Result<Vec<String>, String> {
    let mut repos = Vec::new();
    for page in 1..=10 {
        let v = api(&format!("repos/{HOME}/issues?labels=mod-suggestion,approved&state=all&per_page=100&page={page}"), token)?;
        let items = v.as_array().cloned().unwrap_or_default();
        let done = items.len() < 100;
        for issue in items.iter().filter(|i| i.get("pull_request").is_none()) {
            match issue["body"].as_str().and_then(repo_in) {
                Some(r) => repos.push(r),
                None => eprintln!("issue #{}: no GitHub repository in it", issue["number"]),
            }
        }
        if done {
            break;
        }
    }
    Ok(repos)
}

/// The first GitHub repository linked in an issue's text, other than SporeOne's own.
fn repo_in(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || "()<>[]\"'`".contains(c))
        .filter_map(|word| word.to_ascii_lowercase().find("github.com/").map(|at| &word[at..]))
        .filter_map(parse_repo)
        .find(|r| !r.eq_ignore_ascii_case(HOME))
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
    let token = std::env::var("GITHUB_TOKEN").ok().or_else(|| std::env::var("GH_TOKEN").ok()).filter(|t| !t.is_empty());
    let token = token.as_deref();
    let curated: Vec<Value> = match std::fs::read_to_string(&curated_path).map(|s| serde_json::from_str(&s)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return fail(&format!("{curated_path}: {e}")),
        Err(e) => return fail(&format!("{curated_path}: {e}")),
    };
    let curated_of = |id: &str| curated.iter().find(|c| c["id"].as_str().is_some_and(|c| c.eq_ignore_ascii_case(id)));
    let mut repos = match search(token) {
        Ok(r) => r,
        Err(e) => return fail(&format!("searching GitHub: {e}")),
    };
    // Failing here rather than going on without them, which would take approved mods out.
    let suggested = match approved(token) {
        Ok(r) => r,
        Err(e) => return fail(&format!("reading the approved suggestions: {e}")),
    };
    // Curated GitHub mods and approved suggestions without the topic.
    let extra = curated.iter().filter_map(|c| c["id"].as_str().filter(|id| id.contains('/')).map(String::from)).chain(suggested);
    for id in extra {
        if !repos.iter().any(|r| r["full_name"].as_str().is_some_and(|f| f.eq_ignore_ascii_case(&id))) {
            match api(&format!("repos/{id}"), token) {
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
                    let Some(e) = github::entry_of(repo, cur.and_then(|c| c["setup"].as_str()), token, true) else { continue };
                    if e.info.is_none() && e.release.is_none() && cur.is_none() {
                        continue;
                    }
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
    sporeone_core::catalog::mark_overlaps(&mut mods);
    mods.sort_by(|a, b| b.downloads().cmp(&a.downloads()).then(b.stars.cmp(&a.stars)).then(a.name.cmp(&b.name)));
    let catalog = Catalog { generated: today(), mods };
    let text = serde_json::to_string_pretty(&catalog).unwrap_or_default() + "\n";
    if let Err(e) = std::fs::write(&out_path, text) {
        return fail(&format!("{out_path}: {e}"));
    }
    eprintln!("{} mods in {out_path}", catalog.mods.len());
}

fn fail(msg: &str) {
    eprintln!("sporeone-catalog: {msg}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repositories_in_suggestions() {
        let form = "### Link to the mod\n\nhttps://github.com/Rosalie241/SporeCrashFix/releases/latest\n\n### Name\n\nSpore Crash Fix\n";
        assert_eq!(repo_in(form).as_deref(), Some("Rosalie241/SporeCrashFix"));
        let markdown = "See [the mod](https://github.com/owner/repo.git), as asked in https://github.com/Brunovncs/SporeOne/issues/3";
        assert_eq!(repo_in(markdown).as_deref(), Some("owner/repo"));
        assert_eq!(repo_in("Read https://github.com/brunovncs/sporeone first, then github.com/a/b.").as_deref(), Some("a/b"));
        assert_eq!(repo_in("### Link to the mod\n\nhttps://www.nexusmods.com/spore/mods/12\n"), None);
        assert_eq!(repo_in("https://github.com/topics/spore-mod"), None);
    }
}
