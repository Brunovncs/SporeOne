//! Tweaks of the game's own settings, which players otherwise make by hand in the text files of
//! its `Config` folders: a `property` declared in `Properties.txt` and its value in
//! `ConfigManager.txt`. Turning one on writes its lines, after taking out any other line for the
//! same setting; turning it off takes them out, which leaves the game's default. The first time a
//! file is changed, SporeOne keeps a copy of it as it was.

use crate::game::Game;
use crate::install::{Op, Plan, Roots};
use std::path::{Path, PathBuf};

pub struct Tweak {
    pub id: &'static str,
    /// The file in `Config`, and the line it gets.
    pub lines: &'static [(&'static str, &'static str)],
}

pub const SKIP_INTRO: Tweak = Tweak {
    id: "skip-intro",
    lines: &[
        ("Properties.txt", "property playOpeningMovie (hash(playOpeningMovie)) bool"),
        ("ConfigManager.txt", "boolProp playOpeningMovie false"),
    ],
};

pub const ALL: [&Tweak; 1] = [&SKIP_INTRO];

pub fn by_id(id: &str) -> Option<&'static Tweak> {
    ALL.into_iter().find(|t| t.id == id)
}

/// The `Config` folders the game reads its settings from: Spore's and Galactic Adventures'.
pub fn config_dirs(g: &Game) -> Vec<PathBuf> {
    [&g.spore_data, &g.ga_data].into_iter().flatten().map(|d| d.join("Config")).filter(|d| d.is_dir()).collect()
}

/// What a line sets, to find other lines for the same setting: its first two words.
fn setting(line: &str) -> Option<String> {
    let line = line.split('#').next().unwrap_or("");
    let mut words = line.split_whitespace();
    Some(format!("{} {}", words.next()?, words.next()?).to_ascii_lowercase())
}

fn same(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

/// Whether the tweak is on in every `Config` folder of the game.
pub fn is_on(t: &Tweak, g: &Game) -> bool {
    let dirs = config_dirs(g);
    !dirs.is_empty()
        && dirs.iter().all(|d| {
            t.lines.iter().all(|(file, line)| std::fs::read_to_string(d.join(file)).is_ok_and(|s| s.lines().any(|l| same(l, line))))
        })
}

/// `text` with the tweak's line for it put in, when `on`, or taken out.
pub fn edited(text: &str, line: &str, on: bool) -> String {
    let crlf = text.contains("\r\n");
    let target = setting(line);
    let mut out: Vec<&str> = text.lines().filter(|l| setting(l).is_none() || setting(l) != target).collect();
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    if on {
        out.push(line);
    }
    let eol = if crlf { "\r\n" } else { "\n" };
    out.join(eol) + eol
}

/// The files to write to turn the tweak on or off, keeping in `originals` the files as they were
/// before SporeOne first changed them. The game's folders are where these files are, so they stand
/// in for the Launcher Kit's in the roots the plan is checked against.
pub fn plan(t: &Tweak, g: &Game, on: bool, originals: &Path) -> Result<(Plan, Roots), String> {
    let dirs = config_dirs(g);
    if dirs.is_empty() {
        return Err("the game has no Config folder".into());
    }
    let mut ops = Vec::new();
    for d in &dirs {
        for (file, line) in t.lines {
            let path = d.join(file);
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            keep_original(&path, &text, originals)?;
            let new = edited(&text, line, on);
            if new != text {
                ops.push(Op::Write { path, contents: new.into_bytes() });
            }
        }
    }
    let data = g.ga_data.clone().or_else(|| g.spore_data.clone()).ok_or("the game has no data folder")?;
    Ok((Plan { ops }, Roots { ga_data: g.ga_data.clone(), spore_data: g.spore_data.clone(), lk: data }))
}

/// Keeps `text`, the file at `path` before SporeOne first changed it.
fn keep_original(path: &Path, text: &str, originals: &Path) -> Result<(), String> {
    let folder =
        path.parent().and_then(Path::parent).and_then(Path::file_name).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let kept = originals.join(folder).join(name);
    if kept.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(kept.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    std::fs::write(&kept, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puts_lines_in_and_takes_them_out() {
        let text = "boolProp a true\r\nboolProp playOpeningMovie true # mine\r\n\r\n";
        let on = edited(text, "boolProp playOpeningMovie false", true);
        assert_eq!(on, "boolProp a true\r\nboolProp playOpeningMovie false\r\n");
        assert_eq!(edited(&on, "boolProp playOpeningMovie false", true), on);
        assert_eq!(edited(&on, "boolProp playOpeningMovie false", false), "boolProp a true\r\n");
        assert_eq!(edited("x 1\n", "property playOpeningMovie (hash(playOpeningMovie)) bool", true).lines().count(), 2);
    }

    #[test]
    fn turns_on_and_off_in_a_game() {
        let root = std::env::temp_dir().join(format!("sporeone-tweaks-{}", std::process::id()));
        let data = root.join("DataEP1");
        std::fs::create_dir_all(data.join("Config")).unwrap();
        std::fs::write(data.join("Config/Properties.txt"), "property a 0x1 bool\r\n").unwrap();
        std::fs::write(data.join("Config/ConfigManager.txt"), "boolProp a true\r\n").unwrap();
        let g = Game::at(&root, None, Some(data.clone()), true);
        assert!(!is_on(&SKIP_INTRO, &g));
        let originals = root.join("originals");
        let (plan, roots) = plan(&SKIP_INTRO, &g, true, &originals).unwrap();
        assert_eq!(plan.ops.len(), 2);
        crate::install::apply(&plan, &roots).unwrap();
        assert!(is_on(&SKIP_INTRO, &g));
        assert!(originals.join("DataEP1/ConfigManager.txt").is_file());
        let (off, roots) = super::plan(&SKIP_INTRO, &g, false, &originals).unwrap();
        crate::install::apply(&off, &roots).unwrap();
        assert!(!is_on(&SKIP_INTRO, &g));
        assert_eq!(std::fs::read_to_string(data.join("Config/ConfigManager.txt")).unwrap(), "boolProp a true\r\n");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
