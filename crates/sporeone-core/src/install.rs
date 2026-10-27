//! Installing and uninstalling, worked out as a plan before anything is touched: the files to copy
//! and delete, and the Launcher Kit's list written last. The plan follows what the Launcher Kit's
//! Easy Installer and Easy Uninstaller do (`EasyInstaller.cs`, `XmlInstallerWindow.xaml.cs`,
//! `EasyUninstaller.cs`), so either program can undo what the other did.
//!
//! The game usually sits in Program Files, so the plan is checked and then applied either here or
//! by `sporeone.exe --apply` started with administrator rights. Both check every path against the
//! game's data folders and the Launcher Kit's folder, and both refuse to delete or replace a file
//! the game itself ships.

use crate::installed::{InstalledFile, InstalledMod, InstalledMods};
use crate::modinfo::{self, Item, ModInfo, Target};
use crate::version::Version;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// The folders a plan may write in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roots {
    pub ga_data: Option<PathBuf>,
    pub spore_data: Option<PathBuf>,
    pub lk: PathBuf,
}

impl Roots {
    fn dir(&self, t: Target, legacy: bool) -> Option<PathBuf> {
        match t {
            Target::GalacticAdventures => self.ga_data.clone(),
            Target::Spore => self.spore_data.clone(),
            Target::LauncherKit if legacy => Some(self.lk.clone()),
            Target::LauncherKit => Some(self.lk.join("mLibs")),
        }
    }

    fn data_dirs(&self) -> impl Iterator<Item = &PathBuf> {
        self.ga_data.iter().chain(self.spore_data.iter())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Copy {
        from: PathBuf,
        to: PathBuf,
    },
    /// A file, if it is there.
    Delete {
        path: PathBuf,
    },
    /// A folder and what is in it, if it is there.
    DeleteDir {
        path: PathBuf,
    },
    Write {
        path: PathBuf,
        contents: Vec<u8>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub ops: Vec<Op>,
}

/// Why a mod cannot be installed as it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    NoGalacticAdventures,
    /// Its `ModInfo.xml` is of a format newer than the Launcher Kit knows.
    UnknownFormat(Version),
    /// It needs ModAPI DLLs at least this new.
    NeedsDlls(Version),
    /// Mods it needs that are not installed, by unique name and oldest version.
    Missing(Vec<modinfo::Dependency>),
    /// An old `.sporemod` with its own `Installer.exe`, which the Launcher Kit dropped.
    OldInstaller,
    /// Not a `.sporemod` or `.package`.
    NotAMod,
    Broken(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NoGalacticAdventures => write!(f, "Galactic Adventures is not installed"),
            Refusal::UnknownFormat(v) => write!(f, "made for a newer Launcher Kit (mod format {v})"),
            Refusal::NeedsDlls(v) => write!(f, "needs ModAPI DLLs {v} or later"),
            Refusal::Missing(d) => {
                let names: Vec<String> =
                    d.iter().map(|d| d.min.map_or_else(|| d.unique.clone(), |v| format!("{} {v}", d.unique))).collect();
                write!(f, "needs {}", names.join(", "))
            }
            Refusal::OldInstaller => write!(f, "uses an old installer the Launcher Kit no longer runs"),
            Refusal::NotAMod => write!(f, "is neither a .sporemod nor a .package"),
            Refusal::Broken(e) => write!(f, "{e}"),
        }
    }
}

/// Choices for a mod with options, as the Launcher Kit keeps them in `ModSettings`: a component's
/// unique name to `True` or `False`, a group's to the unique name of the one picked.
pub type Choices = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Package,
    SporeMod(Option<Box<ModInfo>>),
}

/// A downloaded mod, unpacked into a folder of its own, ready to be planned.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The file's name without its extension: the Launcher Kit's key for the mod.
    pub name: String,
    pub kind: Kind,
    pub staging: PathBuf,
    /// The files unpacked, by name: the Launcher Kit flattens a `.sporemod`'s folders.
    pub files: Vec<String>,
    /// For a `.sporemod` without `ModInfo.xml`: every file, with the folder it was in.
    pub entries: Vec<String>,
}

impl Prepared {
    pub fn info(&self) -> Option<&ModInfo> {
        match &self.kind {
            Kind::SporeMod(Some(i)) => Some(i),
            _ => None,
        }
    }
}

fn file_name_only(entry: &str) -> Option<String> {
    let name = entry.rsplit(['/', '\\']).next()?.trim();
    (!name.is_empty() && name != "." && name != ".." && !name.contains(':')).then(|| name.to_string())
}

/// Unpacks `file` into `staging_root\<name>`, emptied first.
pub fn prepare(file: &Path, staging_root: &Path) -> Result<Prepared, Refusal> {
    let fname = file.file_name().and_then(|n| n.to_str()).ok_or(Refusal::NotAMod)?;
    let lower = fname.to_ascii_lowercase();
    let name = Path::new(fname).file_stem().and_then(|s| s.to_str()).unwrap_or(fname).to_string();
    let staging = staging_root.join(&name);
    let broken = |e: std::io::Error| Refusal::Broken(e.to_string());
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(broken)?;
    if lower.ends_with(".package") {
        std::fs::copy(file, staging.join(fname)).map_err(broken)?;
        return Ok(Prepared { name, kind: Kind::Package, staging, files: vec![fname.to_string()], entries: Vec::new() });
    }
    if !lower.ends_with(".sporemod") && !lower.ends_with(".zip") {
        return Err(Refusal::NotAMod);
    }
    let reader = std::fs::File::open(file).map_err(broken)?;
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| Refusal::Broken(format!("not a .sporemod: {e}")))?;
    let mut files = Vec::new();
    let mut entries = Vec::new();
    let mut info_bytes = None;
    let mut installer = false;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Refusal::Broken(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let full = entry.name().replace('\\', "/");
        let Some(base) = file_name_only(&full) else { continue };
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).map_err(broken)?;
        let at_root = !full.contains('/');
        if at_root && base.eq_ignore_ascii_case("ModInfo.xml") {
            info_bytes = Some(bytes.clone());
        }
        installer |= at_root && base.eq_ignore_ascii_case("Installer.exe");
        std::fs::write(staging.join(&base), &bytes).map_err(broken)?;
        if !files.contains(&base) {
            files.push(base);
        }
        entries.push(full);
    }
    let info = match info_bytes {
        Some(b) => Some(Box::new(modinfo::parse(&b).map_err(|e| Refusal::Broken(format!("ModInfo.xml: {e}")))?)),
        None if installer => return Err(Refusal::OldInstaller),
        None => None,
    };
    Ok(Prepared { name, kind: Kind::SporeMod(info), staging, files, entries })
}

/// The choices a mod starts with: each component as its author set it, and in each group the
/// first one set on.
pub fn default_choices(info: &ModInfo) -> Choices {
    let mut c = Choices::new();
    for item in &info.items {
        match item {
            Item::Component(comp) => {
                c.insert(comp.unique.clone(), bool_text(comp.default_checked).into());
            }
            Item::Group { unique, components, .. } => {
                let picked = components.iter().find(|c| c.default_checked).map(|c| c.unique.clone()).unwrap_or_default();
                c.insert(unique.clone(), picked);
            }
            _ => {}
        }
    }
    c
}

fn bool_text(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

/// The choices kept from the last time, for a mod installed before.
pub fn saved_choices(lk: &Path, unique: &str) -> Option<Choices> {
    let text = std::fs::read_to_string(lk.join("ModSettings").join(unique)).ok()?;
    let mut c = Choices::new();
    for line in text.lines() {
        if let Some((k, v)) = line.strip_prefix('<').and_then(|l| l.split_once('>')) {
            c.insert(k.to_string(), v.to_string());
        }
    }
    Some(c)
}

fn chosen(choices: &Choices, info: &ModInfo, comp: &modinfo::Component, group: Option<&str>) -> bool {
    // Without a custom installer the Launcher Kit installs the defaults.
    if !info.has_choices() {
        return comp.default_checked;
    }
    match group {
        Some(g) => choices.get(g).map(String::as_str) == Some(comp.unique.as_str()),
        None => choices.get(&comp.unique).map_or(comp.default_checked, |v| v.eq_ignore_ascii_case("true")),
    }
}

/// The installed mods a new one's dependencies are not met by.
pub fn missing(info: &ModInfo, installed: &InstalledMods) -> Vec<modinfo::Dependency> {
    info.depends
        .iter()
        .filter(|d| !installed.mods.iter().any(|m| m.unique == d.unique && d.min.is_none_or(|min| m.version.is_some_and(|v| v >= min))))
        .cloned()
        .collect()
}

/// Where each of a file's copies goes: a legacy ModAPI DLL has one per game build.
fn destinations(name: &str, target: Target, legacy: bool, roots: &Roots) -> Vec<(String, PathBuf)> {
    let Some(dir) = roots.dir(target, legacy) else { return Vec::new() };
    if target == Target::LauncherKit && legacy && name.to_ascii_lowercase().ends_with(".dll") {
        let lower = name.to_ascii_lowercase();
        if ["-disk.dll", "-steam.dll", "-steam_patched.dll"].iter().any(|s| lower.ends_with(s)) {
            return vec![(name.to_string(), dir.join(name))];
        }
        let stem = &name[..name.len() - 4];
        return ["-disk.dll", "-steam.dll", "-steam_patched.dll"]
            .iter()
            .map(|s| {
                let n = format!("{stem}{s}");
                (n.clone(), dir.join(n))
            })
            .collect();
    }
    vec![(name.to_string(), dir.join(name))]
}

/// `*` and `?` in a `<remove>` pattern, as .NET's `EnumerateFiles` reads them, without case.
pub fn wildcard(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.to_lowercase().chars().collect(), name.to_lowercase().chars().collect());
    let (mut pi, mut ni, mut star, mut mark) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// Installs `p` with `choices`, replacing whatever version of it is there.
pub fn plan_install(
    p: &Prepared,
    roots: &Roots,
    installed: &InstalledMods,
    dlls_build: Option<Version>,
    choices: Option<&Choices>,
) -> Result<Plan, Refusal> {
    if roots.ga_data.is_none() {
        return Err(Refusal::NoGalacticAdventures);
    }
    let mut list = installed.clone();
    let mut ops = Vec::new();
    let unique = p.info().map(|i| i.unique.clone()).unwrap_or_else(|| p.name.clone());
    // An update: the old version goes first, as the Launcher Kit delists it.
    for old in installed.mods.iter().filter(|m| m.name == p.name || m.unique == unique) {
        ops.extend(plan_uninstall(old, roots, &list).ops.into_iter().filter(|o| !is_list(o, roots)));
        list.remove(&old.name);
    }
    let src = |name: &str| p.staging.join(name);
    let record = match &p.kind {
        Kind::Package => {
            let name = &p.files[0];
            ops.push(Op::Copy { from: src(name), to: roots.ga_data.as_ref().unwrap().join(name) });
            let mut m = InstalledMod::new(&p.name);
            m.files.push(InstalledFile::new(name, Target::GalacticAdventures));
            m
        }
        Kind::SporeMod(None) => {
            let configs = roots.lk.join("ModConfigs").join(&p.name);
            let mut m = InstalledMod::new(&p.name);
            for name in &p.files {
                ops.push(Op::Copy { from: src(name), to: configs.join(name) });
                let lower = name.to_ascii_lowercase();
                let target = if lower.ends_with(".package") {
                    Target::GalacticAdventures
                } else if lower.ends_with(".dll") {
                    Target::LauncherKit
                } else {
                    continue;
                };
                // A plain `.sporemod`'s DLLs go to the Launcher Kit's folder itself.
                if let Some((_, to)) = destinations(name, target, true, roots).into_iter().next() {
                    ops.push(Op::Copy { from: src(name), to });
                    m.files.push(InstalledFile::new(name, target));
                }
            }
            m
        }
        Kind::SporeMod(Some(info)) => plan_xml(p, info, roots, &list, dlls_build, choices, &mut ops)?,
    };
    list.put(record);
    ops.push(write_list(&list, roots));
    Ok(Plan { ops })
}

fn is_list(op: &Op, roots: &Roots) -> bool {
    matches!(op, Op::Write { path, .. } if *path == roots.lk.join(crate::installed::FILE_NAME))
}

fn write_list(list: &InstalledMods, roots: &Roots) -> Op {
    Op::Write { path: roots.lk.join(crate::installed::FILE_NAME), contents: list.to_xml().into_bytes() }
}

fn plan_xml(
    p: &Prepared,
    info: &ModInfo,
    roots: &Roots,
    list: &InstalledMods,
    dlls_build: Option<Version>,
    choices: Option<&Choices>,
    ops: &mut Vec<Op>,
) -> Result<InstalledMod, Refusal> {
    if !modinfo::KNOWN_FORMATS.contains(&info.format) {
        return Err(Refusal::UnknownFormat(info.format));
    }
    if let (Some(need), Some(have)) = (info.dlls_build, dlls_build)
        && need.build3() > have
    {
        return Err(Refusal::NeedsDlls(need));
    }
    let lacking = missing(info, list);
    if !lacking.is_empty() {
        return Err(Refusal::Missing(lacking));
    }
    let legacy = info.legacy_dlls();
    let configs = roots.lk.join("ModConfigs").join(&p.name);
    for name in &p.files {
        ops.push(Op::Copy { from: p.staging.join(name), to: configs.join(name) });
    }
    let defaults = default_choices(info);
    let choices = choices.unwrap_or(&defaults);
    let mut planned: HashSet<PathBuf> = HashSet::new();
    let mut recorded = Vec::new();
    let copy = |files: &modinfo::Files, ops: &mut Vec<Op>, planned: &mut HashSet<PathBuf>, recorded: &mut Vec<InstalledFile>| {
        for (name, target) in &files.0 {
            for (n, to) in destinations(name, *target, legacy, roots) {
                let from = p.staging.join(&n);
                // As the Launcher Kit does: a file the mod names but does not carry is skipped.
                if from.is_file() {
                    planned.insert(to.clone());
                    ops.push(Op::Copy { from, to });
                }
            }
            recorded.push(InstalledFile::new(name, *target));
        }
    };
    for item in &info.items {
        if let Item::Remove(files) = item {
            for (pattern, target) in &files.0 {
                let dirs = match roots.dir(*target, false) {
                    Some(d) if *target == Target::LauncherKit => vec![roots.lk.clone(), d],
                    Some(d) => vec![d],
                    None => Vec::new(),
                };
                for dir in dirs {
                    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                        let n = e.file_name().to_string_lossy().to_string();
                        if e.path().is_file() && wildcard(pattern, &n) && !crate::game::is_game_file(&n) {
                            ops.push(Op::Delete { path: e.path() });
                        }
                    }
                }
            }
        }
    }
    for item in &info.items {
        match item {
            Item::Prerequisite(files) => copy(files, ops, &mut planned, &mut recorded),
            Item::Component(c) if chosen(choices, info, c, None) => copy(&c.files, ops, &mut planned, &mut recorded),
            Item::Group { unique, components, .. } => {
                for c in components.iter().filter(|c| chosen(choices, info, c, Some(unique))) {
                    copy(&c.files, ops, &mut planned, &mut recorded);
                }
            }
            _ => {}
        }
    }
    for item in &info.items {
        if let Item::Compat { files, targets, remove_targets } = item {
            let mut found = Vec::new();
            let all = targets.0.iter().all(|(name, target)| {
                let candidates: Vec<PathBuf> = match target {
                    Target::LauncherKit => vec![roots.lk.join(name), roots.lk.join("mLibs").join(name)],
                    t => roots.dir(*t, false).map(|d| vec![d.join(name)]).unwrap_or_default(),
                };
                let hit: Vec<PathBuf> = candidates.into_iter().filter(|c| c.is_file() || planned.contains(c)).collect();
                found.extend(hit.iter().cloned());
                !hit.is_empty()
            });
            if all {
                if *remove_targets {
                    ops.extend(found.into_iter().map(|path| Op::Delete { path }));
                }
                copy(files, ops, &mut planned, &mut recorded);
            }
        }
    }
    let mut settings = String::new();
    for item in &info.items {
        match item {
            Item::Component(c) => settings.push_str(&format!("<{}>{}\r\n", c.unique, bool_text(chosen(choices, info, c, None)))),
            Item::Group { unique, .. } => settings.push_str(&format!("<{unique}>{}\r\n", choices.get(unique).cloned().unwrap_or_default())),
            _ => {}
        }
    }
    if !settings.is_empty() {
        ops.push(Op::Write { path: roots.lk.join("ModSettings").join(&info.unique), contents: settings.into_bytes() });
    }
    let mut m = InstalledMod::new(&p.name);
    m.unique = info.unique.clone();
    m.display_name = Some(info.display_name.clone());
    m.version = info.version;
    m.dependencies = info.depends.iter().map(|d| d.unique.clone()).collect();
    if info.depends.iter().any(|d| d.min.is_some()) {
        m.dependencies_versions = info.depends.iter().map(|d| d.min.unwrap_or_default()).collect();
    }
    if info.has_choices() {
        // The Easy Uninstaller opens the mod's own installer again for these.
        m.configurator = Some(configs.join("ModInfo.xml").to_string_lossy().into_owned());
        m.files.push(InstalledFile::new(&p.name, Target::GalacticAdventures));
    } else {
        m.files = recorded;
    }
    Ok(m)
}

/// Removes `m`: with its `ModInfo.xml` kept under `ModConfigs`, every file any of its choices
/// could have put in place; otherwise the files the list names.
pub fn plan_uninstall(m: &InstalledMod, roots: &Roots, installed: &InstalledMods) -> Plan {
    let mut ops = Vec::new();
    let configs = roots.lk.join("ModConfigs").join(&m.name);
    let info = std::fs::read(configs.join("ModInfo.xml")).ok().and_then(|b| modinfo::parse(&b).ok());
    match info.filter(|_| m.configurator.is_some()) {
        Some(info) => {
            let legacy = info.legacy_dlls();
            let mut delete = |files: &modinfo::Files| {
                for (name, target) in &files.0 {
                    for (_, path) in destinations(name, *target, legacy, roots) {
                        ops.push(Op::Delete { path });
                    }
                }
            };
            for item in &info.items {
                match item {
                    Item::Prerequisite(f) | Item::Compat { files: f, .. } => delete(f),
                    Item::Component(c) => delete(&c.files),
                    Item::Group { components, .. } => components.iter().for_each(|c| delete(&c.files)),
                    Item::Remove(_) => {}
                }
            }
            ops.push(Op::Delete { path: roots.lk.join("ModSettings").join(&info.unique) });
        }
        None => {
            for f in &m.files {
                match f.target() {
                    Some(Target::LauncherKit) | None => {
                        ops.push(Op::Delete { path: roots.lk.join(&f.name) });
                        ops.push(Op::Delete { path: roots.lk.join("mLibs").join(&f.name) });
                    }
                    Some(t) => {
                        if let Some(d) = roots.dir(t, false) {
                            ops.push(Op::Delete { path: d.join(&f.name) });
                        }
                    }
                }
            }
        }
    }
    ops.push(Op::DeleteDir { path: configs });
    let mut list = installed.clone();
    list.remove(&m.name);
    ops.push(write_list(&list, roots));
    Plan { ops }
}

fn lexical(p: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => return None,
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    Some(out)
}

fn inside(path: &Path, root: &Path) -> bool {
    let key = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase();
    let (p, r) = (key(path), key(root));
    p.len() > r.len() && p.starts_with(&r) && p.as_bytes()[r.len()] == b'\\'
}

/// Every path the plan writes or deletes is inside the roots, and none is the game's own file.
pub fn check(plan: &Plan, roots: &Roots) -> Result<(), String> {
    let allowed: Vec<&PathBuf> = roots.data_dirs().chain(std::iter::once(&roots.lk)).collect();
    for op in &plan.ops {
        let target = match op {
            Op::Copy { to, .. } => to,
            Op::Delete { path } | Op::DeleteDir { path } | Op::Write { path, .. } => path,
        };
        let clean = lexical(target).ok_or_else(|| format!("{} leaves the folders a mod may change", target.display()))?;
        if !allowed.iter().any(|r| inside(&clean, r)) {
            return Err(format!("{} is outside the game's data folders and the Launcher Kit", target.display()));
        }
        let in_data = roots.data_dirs().any(|d| inside(&clean, d));
        let name = clean.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if in_data && !matches!(op, Op::DeleteDir { .. }) && crate::game::is_game_file(&name) {
            return Err(format!("{name} belongs to the game itself"));
        }
        if in_data && matches!(op, Op::DeleteDir { .. }) {
            return Err(format!("{} is a folder of the game", target.display()));
        }
    }
    Ok(())
}

/// Folders the plan writes in that this process may not write in: then it has to run with
/// administrator rights.
pub fn needs_admin(plan: &Plan) -> bool {
    let mut seen = HashSet::new();
    plan.ops.iter().any(|op| {
        let target = match op {
            Op::Copy { to, .. } => to,
            Op::Delete { path } | Op::DeleteDir { path } | Op::Write { path, .. } => path,
        };
        // The nearest folder that exists is the one whose rights count.
        let mut dir = target.parent().map(Path::to_path_buf);
        while let Some(d) = dir.clone() {
            if d.is_dir() {
                break;
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        dir.is_some_and(|d| seen.insert(d.clone()) && !crate::win::writable(&d))
    })
}

/// Carries the plan out, after checking it. Stops at the first failure and says where.
pub fn apply(plan: &Plan, roots: &Roots) -> Result<(), String> {
    check(plan, roots)?;
    for op in &plan.ops {
        let result = match op {
            Op::Copy { from, to } => {
                to.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::copy(from, to).map(|_| ())).map_err(|e| (to, e))
            }
            Op::Delete { path } => match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err((path, e)),
                _ => Ok(()),
            },
            Op::DeleteDir { path } => match std::fs::remove_dir_all(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err((path, e)),
                _ => Ok(()),
            },
            Op::Write { path, contents } => {
                path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(path, contents)).map_err(|e| (path, e))
            }
        };
        result.map_err(|(p, e)| format!("{}: {e}", p.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("sporeone-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sporemod(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn setup(s: &Scratch) -> Roots {
        let roots = Roots { ga_data: Some(s.0.join("game/DataEP1")), spore_data: Some(s.0.join("game/Data")), lk: s.0.join("lk") };
        for d in [roots.ga_data.as_ref().unwrap(), roots.spore_data.as_ref().unwrap(), &roots.lk] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(roots.ga_data.as_ref().unwrap().join("Spore_EP1_Data.package"), b"game").unwrap();
        roots
    }

    const INFO: &str = r#"<mod unique="Paints" displayName="Paints" installerSystemVersion="1.0.1.3" dllsBuild="2.5.20"
        hasCustomInstaller="true" version="1.1">
        <prerequisite>Paints.dll</prerequisite>
        <component unique="extra" displayName="Extra" game="GalacticAdventures">Extra.package</component>
        <componentGroup unique="look" displayName="Look">
            <component unique="dark" displayName="Dark" game="GalacticAdventures" defaultChecked="true">Dark.package</component>
            <component unique="light" displayName="Light" game="GalacticAdventures">Light.package</component>
        </componentGroup>
        <remove game="GalacticAdventures">*</remove>
    </mod>"#;

    #[test]
    fn installs_a_mod_with_choices_and_takes_it_out_again() {
        let s = Scratch::new("xml");
        let roots = setup(&s);
        std::fs::write(roots.ga_data.as_ref().unwrap().join("Leftover.package"), b"old").unwrap();
        let file = s.0.join("Paints.sporemod");
        sporemod(
            &file,
            &[
                ("ModInfo.xml", INFO.as_bytes()),
                ("Paints.dll", b"dll"),
                ("parts/Extra.package", b"e"),
                ("Dark.package", b"d"),
                ("Light.package", b"l"),
            ],
        );
        let p = prepare(&file, &s.0.join("staging")).unwrap();
        assert_eq!(p.files.len(), 5);
        let none = InstalledMods::default();
        let refused = plan_install(&p, &roots, &none, Version::parse("2.5.19"), None).unwrap_err();
        assert_eq!(refused, Refusal::NeedsDlls(Version::parse("2.5.20").unwrap()));
        let plan = plan_install(&p, &roots, &none, Version::parse("2.5.300"), None).unwrap();
        assert!(!needs_admin(&plan));
        apply(&plan, &roots).unwrap();
        let ga = roots.ga_data.clone().unwrap();
        assert!(ga.join("Dark.package").is_file() && !ga.join("Light.package").exists() && !ga.join("Extra.package").exists());
        assert!(!ga.join("Leftover.package").exists(), "<remove> ran");
        assert!(ga.join("Spore_EP1_Data.package").is_file(), "the game's own file is kept");
        assert!(roots.lk.join("mLibs/Paints.dll").is_file());
        assert_eq!(std::fs::read_to_string(roots.lk.join("ModSettings/Paints")).unwrap(), "<extra>False\r\n<look>dark\r\n");
        let list = InstalledMods::read(&roots.lk).unwrap();
        let m = list.find("Paints").unwrap();
        assert!(m.configurator.as_deref().unwrap().ends_with("ModInfo.xml"));
        assert_eq!(saved_choices(&roots.lk, "Paints").unwrap().get("look").unwrap(), "dark");

        let mut choices = default_choices(p.info().unwrap());
        choices.insert("look".into(), "light".into());
        choices.insert("extra".into(), "True".into());
        apply(&plan_install(&p, &roots, &list, None, Some(&choices)).unwrap(), &roots).unwrap();
        assert!(ga.join("Light.package").is_file() && ga.join("Extra.package").is_file() && !ga.join("Dark.package").exists());
        let list = InstalledMods::read(&roots.lk).unwrap();
        assert_eq!(list.mods.len(), 1);

        apply(&plan_uninstall(list.find("Paints").unwrap(), &roots, &list), &roots).unwrap();
        assert!(!ga.join("Light.package").exists() && !roots.lk.join("mLibs/Paints.dll").exists());
        assert!(!roots.lk.join("ModConfigs/Paints").exists() && !roots.lk.join("ModSettings/Paints").exists());
        assert!(InstalledMods::read(&roots.lk).unwrap().mods.is_empty());
    }

    #[test]
    fn a_package_and_a_plain_sporemod() {
        let s = Scratch::new("plain");
        let roots = setup(&s);
        let pkg = s.0.join("Bigger Planets.package");
        std::fs::write(&pkg, b"pkg").unwrap();
        let p = prepare(&pkg, &s.0.join("staging")).unwrap();
        apply(&plan_install(&p, &roots, &InstalledMods::default(), None, None).unwrap(), &roots).unwrap();
        let list = InstalledMods::read(&roots.lk).unwrap();
        assert_eq!(list.find("Bigger Planets").unwrap().files[0].game, "GalacticAdventures");

        let zip = s.0.join("Old.sporemod");
        sporemod(&zip, &[("Old.package", b"a"), ("Old-steam.dll", b"b"), ("readme.txt", b"c")]);
        let p = prepare(&zip, &s.0.join("staging")).unwrap();
        apply(&plan_install(&p, &roots, &list, None, None).unwrap(), &roots).unwrap();
        assert!(roots.lk.join("Old-steam.dll").is_file() && roots.lk.join("ModConfigs/Old/readme.txt").is_file());
        let list = InstalledMods::read(&roots.lk).unwrap();
        let old = list.find("Old").unwrap();
        assert_eq!(old.files.len(), 2);
        apply(&plan_uninstall(old, &roots, &list), &roots).unwrap();
        assert!(!roots.lk.join("Old-steam.dll").exists() && !roots.ga_data.as_ref().unwrap().join("Old.package").exists());
    }

    #[test]
    fn dependencies_and_bad_files_are_refused() {
        let s = Scratch::new("refuse");
        let roots = setup(&s);
        let file = s.0.join("Needy.sporemod");
        let info = r#"<mod unique="Needy" installerSystemVersion="1.0.1.3" dllsBuild="2.5.20" depends="Base" dependsVersions="2.0"/>"#;
        sporemod(&file, &[("ModInfo.xml", info.as_bytes())]);
        let p = prepare(&file, &s.0.join("staging")).unwrap();
        let mut list = InstalledMods::default();
        let mut base = InstalledMod::new("Base");
        base.version = Version::parse("1.9");
        list.put(base.clone());
        assert!(matches!(plan_install(&p, &roots, &list, None, None), Err(Refusal::Missing(_))));
        base.version = Version::parse("2.0.1");
        list.put(base);
        assert!(plan_install(&p, &roots, &list, None, None).is_ok());

        let old = s.0.join("Exe.sporemod");
        sporemod(&old, &[("Installer.exe", b"MZ")]);
        assert_eq!(prepare(&old, &s.0.join("staging")).unwrap_err(), Refusal::OldInstaller);
        assert_eq!(prepare(&s.0.join("x.txt"), &s.0.join("staging")).unwrap_err(), Refusal::NotAMod);
    }

    #[test]
    fn plans_stay_inside_their_folders() {
        let s = Scratch::new("check");
        let roots = setup(&s);
        let ga = roots.ga_data.clone().unwrap();
        let bad = [
            Op::Delete { path: ga.join("Spore_EP1_Data.package") },
            Op::Delete { path: s.0.join("elsewhere.txt") },
            Op::Delete { path: ga.join("..").join("SporebinEP1").join("SporeApp.exe") },
            Op::DeleteDir { path: ga.clone() },
            Op::DeleteDir { path: ga.join("Locale") },
        ];
        for op in bad {
            assert!(check(&Plan { ops: vec![op.clone()] }, &roots).is_err(), "{op:?}");
        }
        assert!(check(&Plan { ops: vec![Op::Delete { path: ga.join("Mod.package") }] }, &roots).is_ok());
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("*", "a.package"));
        assert!(wildcard("Nothing*", "nothingAtAll.package"));
        assert!(wildcard("*Ends.package", "x_ends.PACKAGE"));
        assert!(wildcard("a?c", "abc"));
        assert!(!wildcard("a?c", "abbc"));
        assert!(!wildcard("*.dll", "a.package"));
    }
}
