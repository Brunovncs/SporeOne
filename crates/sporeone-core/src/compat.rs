//! Whether a mod can go on this game, said before the download: what stops it, what to know
//! first, and what comes with it. The same checks the Launcher Kit makes as it installs (game
//! version, ModAPI DLLs, dependencies) plus what only the catalog knows (stores, conflicts, mods
//! nobody maintains).

use crate::catalog::{Catalog, Entry, Kind, Status};
use crate::game::{Game, Store};
use crate::installed::InstalledMods;
use crate::launcher_kit::LauncherKit;
use crate::modinfo::{Dependency, Item};
use crate::version::Version;
use serde::{Deserialize, Serialize};

/// What SporeOne knows about this computer.
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// Every copy of the game found, the one used first.
    pub games: Vec<Game>,
    pub lk: Option<LauncherKit>,
    pub installed: InstalledMods,
}

impl Env {
    pub fn game(&self) -> Option<&Game> {
        self.games.first()
    }

    pub fn read() -> Env {
        let games = crate::game::find();
        let lk = crate::launcher_kit::find();
        let installed = lk.as_ref().and_then(|l| InstalledMods::read(&l.dir).ok()).unwrap_or_default();
        Env { games, lk, installed }
    }

    pub fn roots(&self) -> Option<crate::install::Roots> {
        let lk = self.lk.as_ref()?;
        let g = self.game();
        Some(crate::install::Roots {
            ga_data: g.and_then(|g| g.ga_data.clone()),
            spore_data: g.and_then(|g| g.spore_data.clone()),
            lk: lk.dir.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Issue {
    // Stops the install.
    NoGame,
    NoGalacticAdventures,
    /// The game's executable is not one of the builds mods work with.
    UnsupportedVersion,
    NoLauncherKit,
    /// The Launcher Kit's DLLs are older than the mod needs; starting the game through the
    /// Launcher Kit updates them.
    OldDlls(Version),
    /// SporeModLoader is installed, and the Launcher Kit will not start the game beside it.
    ModLoader,
    WrongStore(Vec<Store>),
    /// Needs mods the catalog does not have.
    Unavailable(Vec<Dependency>),
    Broken,
    // Worth knowing first.
    /// More than one copy of the game is installed.
    SeveralCopies,
    /// Steam has the game but never started it, so the registry does not say where it is.
    StartOnce,
    Experimental,
    GalaxyReset,
    SaveDependency,
    Old,
    /// Installed mods it is known not to work with, by name.
    Conflicts(Vec<String>),
    /// Installed mods whose files it would replace.
    Replaces(Vec<String>),
    /// Installed mods that change some of the same game resources, by name.
    Overlaps(Vec<String>),
    /// Mods installed with it as well, by catalog id.
    Brings(Vec<String>),
}

impl Issue {
    pub fn blocks(&self) -> bool {
        matches!(
            self,
            Issue::NoGame
                | Issue::NoGalacticAdventures
                | Issue::UnsupportedVersion
                | Issue::NoLauncherKit
                | Issue::OldDlls(_)
                | Issue::ModLoader
                | Issue::WrongStore(_)
                | Issue::Unavailable(_)
                | Issue::Broken
                | Issue::StartOnce
        )
    }
}

/// What stands between the game and mods at all, for the banner on the first screen.
pub fn setup_issues(env: &Env) -> Vec<Issue> {
    let mut out = Vec::new();
    let Some(g) = env.game() else {
        out.push(Issue::NoGame);
        return out;
    };
    if !g.has_ga() {
        out.push(Issue::NoGalacticAdventures);
    } else if g.edition.is_none() {
        out.push(Issue::UnsupportedVersion);
    }
    if !g.registered && g.has_ga() {
        out.push(Issue::StartOnce);
    }
    if env.games.len() > 1 {
        out.push(Issue::SeveralCopies);
    }
    if g.mod_loader {
        out.push(Issue::ModLoader);
    }
    if env.lk.is_none() {
        out.push(Issue::NoLauncherKit);
    }
    out
}

/// The dependencies not installed, and of those the ones the catalog can bring, in the order to
/// install them (a dependency's own first).
pub fn resolve(entry: &Entry, catalog: &Catalog, installed: &InstalledMods) -> (Vec<String>, Vec<Dependency>) {
    let mut bring: Vec<String> = Vec::new();
    let mut lacking = Vec::new();
    fn walk(e: &Entry, c: &Catalog, i: &InstalledMods, bring: &mut Vec<String>, lacking: &mut Vec<Dependency>, depth: usize) {
        let Some(info) = &e.info else { return };
        for d in crate::install::missing(info, i) {
            match c.by_unique(&d.unique).filter(|dep| dep.installable() && d.min.is_none_or(|m| dep.version().is_some_and(|v| v >= m))) {
                Some(dep) if !bring.contains(&dep.id) && depth < 8 => {
                    walk(dep, c, i, bring, lacking, depth + 1);
                    bring.push(dep.id.clone());
                }
                Some(_) => {}
                None => lacking.push(d),
            }
        }
    }
    walk(entry, catalog, installed, &mut bring, &mut lacking, 0);
    (bring, lacking)
}

/// The files a mod puts in the game with its default choices, by name.
pub fn files_of(entry: &Entry) -> Vec<String> {
    let Some(info) = &entry.info else {
        return entry.release.iter().filter(|r| r.file.to_lowercase().ends_with(".package")).map(|r| r.file.clone()).collect();
    };
    let defaults = crate::install::default_choices(info);
    let mut out = Vec::new();
    for item in &info.items {
        match item {
            Item::Prerequisite(f) => out.extend(f.0.iter().map(|(n, _)| n.clone())),
            Item::Component(c) if defaults.get(&c.unique).is_some_and(|v| v == "True") => {
                out.extend(c.files.0.iter().map(|(n, _)| n.clone()))
            }
            Item::Group { unique, components, .. } => {
                for c in components.iter().filter(|c| defaults.get(unique) == Some(&c.unique)) {
                    out.extend(c.files.0.iter().map(|(n, _)| n.clone()));
                }
            }
            _ => {}
        }
    }
    out
}

/// Everything to know before installing `entry`.
pub fn check(entry: &Entry, catalog: &Catalog, env: &Env) -> Vec<Issue> {
    let mut out = Vec::new();
    let game = env.game();
    match game {
        None if entry.needs.galactic_adventures || entry.needs.launcher_kit => out.push(Issue::NoGame),
        Some(g) if entry.needs.galactic_adventures && !g.has_ga() => out.push(Issue::NoGalacticAdventures),
        Some(g) if entry.needs.launcher_kit && g.has_ga() && g.edition.is_none() => out.push(Issue::UnsupportedVersion),
        _ => {}
    }
    if let Some(g) = game {
        if !entry.needs.stores.is_empty() && !entry.needs.stores.contains(&g.store) {
            // A March 2017 copy whose store could not be told apart is let through.
            if g.store != Store::Unknown {
                out.push(Issue::WrongStore(entry.needs.stores.clone()));
            }
        }
        if entry.needs.launcher_kit && g.mod_loader {
            out.push(Issue::ModLoader);
        }
        if entry.needs.launcher_kit && g.has_ga() && !g.registered {
            out.push(Issue::StartOnce);
        }
    }
    if env.games.len() > 1 {
        out.push(Issue::SeveralCopies);
    }
    if entry.needs.launcher_kit && entry.kind == Kind::Release {
        match &env.lk {
            None => out.push(Issue::NoLauncherKit),
            Some(lk) => {
                if let (Some(need), Some(have)) = (entry.info.as_ref().and_then(|i| i.dlls_build), lk.dlls_build)
                    && need.build3() > have
                {
                    out.push(Issue::OldDlls(need.build3()));
                }
            }
        }
    }
    match entry.status {
        Status::Broken => out.push(Issue::Broken),
        Status::Experimental => out.push(Issue::Experimental),
        Status::Old => out.push(Issue::Old),
        Status::Maintained => {}
    }
    if let Some(info) = &entry.info {
        if info.experimental && entry.status != Status::Experimental {
            out.push(Issue::Experimental);
        }
        if info.requires_galaxy_reset {
            out.push(Issue::GalaxyReset);
        }
        if info.save_data_dependency {
            out.push(Issue::SaveDependency);
        }
    }
    let (bring, lacking) = resolve(entry, catalog, &env.installed);
    if !lacking.is_empty() {
        out.push(Issue::Unavailable(lacking));
    }
    if !bring.is_empty() {
        out.push(Issue::Brings(bring));
    }
    let mine = |name: &str| entry.unique() == Some(name) || entry.release.as_ref().is_some_and(|r| r.file.starts_with(name));
    let conflicts: Vec<String> = env
        .installed
        .mods
        .iter()
        .filter(|m| {
            entry.conflicts.iter().any(|c| *c == m.unique || *c == m.name || catalog.get(c).and_then(Entry::unique) == Some(&m.unique))
        })
        .map(|m| m.shown_name().to_string())
        .collect();
    if !conflicts.is_empty() {
        out.push(Issue::Conflicts(conflicts));
    }
    let files = files_of(entry);
    let replaces: Vec<String> = env
        .installed
        .mods
        .iter()
        .filter(|m| !mine(&m.unique) && !mine(&m.name))
        .filter(|m| m.files.iter().any(|f| files.iter().any(|n| n.eq_ignore_ascii_case(&f.name))))
        .map(|m| m.shown_name().to_string())
        .collect();
    if !replaces.is_empty() {
        out.push(Issue::Replaces(replaces.clone()));
    }
    let overlaps: Vec<String> = env
        .installed
        .mods
        .iter()
        .filter(|m| !mine(&m.unique) && !replaces.iter().any(|r| r == m.shown_name()))
        .filter(|m| entry.overlaps.iter().any(|o| catalog.get(&o.id).and_then(Entry::unique) == Some(&m.unique)))
        .map(|m| m.shown_name().to_string())
        .collect();
    if !overlaps.is_empty() {
        out.push(Issue::Overlaps(overlaps));
    }
    out
}

/// Installed mods that need `unique`, which uninstalling it would leave without.
pub fn dependents(unique: &str, installed: &InstalledMods) -> Vec<String> {
    installed
        .mods
        .iter()
        .filter(|m| m.unique != unique && m.dependencies.iter().any(|d| d == unique))
        .map(|m| m.shown_name().to_string())
        .collect()
}

/// Whether the installed copy of `entry` is older than the catalog's.
pub fn outdated(entry: &Entry, installed: &crate::installed::InstalledMod, installed_tag: Option<&str>) -> bool {
    match (entry.info.as_ref().and_then(|i| i.version), installed.version) {
        (Some(new), Some(old)) => new > old,
        _ => match (entry.release.as_ref(), installed_tag) {
            (Some(r), Some(tag)) => r.tag != tag,
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Release;
    use crate::installed::InstalledMod;
    use crate::modinfo::ModInfo;

    fn info(unique: &str, depends: &[&str]) -> ModInfo {
        ModInfo {
            unique: unique.into(),
            display_name: unique.into(),
            description: None,
            format: Version::new(1, 0, 1, 3),
            version: Version::parse("1.0"),
            dlls_build: Version::parse("2.5.20"),
            depends: depends.iter().map(|d| Dependency { unique: (*d).into(), min: None }).collect(),
            has_custom_installer: false,
            experimental: false,
            requires_galaxy_reset: false,
            save_data_dependency: false,
            compat_only: false,
            items: vec![Item::Prerequisite(crate::modinfo::Files(vec![(
                format!("{unique}.package"),
                crate::modinfo::Target::GalacticAdventures,
            )]))],
        }
    }

    fn entry(id: &str, unique: &str, depends: &[&str]) -> Entry {
        Entry {
            id: id.into(),
            name: unique.into(),
            info: Some(info(unique, depends)),
            release: Some(Release { tag: "v1.0".into(), file: format!("{unique}.sporemod"), ..Default::default() }),
            ..Default::default()
        }
    }

    fn env() -> Env {
        let root = std::env::temp_dir().join(format!("sporeone-compat-{}", std::process::id()));
        Env {
            games: vec![Game {
                root: root.clone(),
                spore_data: Some(root.join("Data")),
                ga_data: Some(root.join("DataEP1")),
                ga_exe: Some(root.join("SporebinEP1/SporeApp.exe")),
                edition: Some(crate::game::Edition::SteamOctober2024),
                exe_version: None,
                store: Store::Steam,
                registered: true,
                mod_loader: false,
            }],
            lk: Some(LauncherKit { dir: root.join("lk"), version: None, dlls_build: Version::parse("2.5.300") }),
            installed: InstalledMods::default(),
        }
    }

    #[test]
    fn dependencies_come_along_in_order() {
        let catalog = Catalog {
            generated: String::new(),
            mods: vec![entry("a/top", "Top", &["Mid"]), entry("a/mid", "Mid", &["Base"]), entry("a/base", "Base", &[])],
        };
        let (bring, lacking) = resolve(&catalog.mods[0], &catalog, &InstalledMods::default());
        assert_eq!(bring, vec!["a/base", "a/mid"]);
        assert!(lacking.is_empty());
        let issues = check(&catalog.mods[0], &catalog, &env());
        assert!(issues.iter().all(|i| !i.blocks()), "{issues:?}");
        let lonely = Catalog { generated: String::new(), mods: vec![entry("a/top", "Top", &["Gone"])] };
        assert!(check(&lonely.mods[0], &lonely, &env()).iter().any(|i| matches!(i, Issue::Unavailable(_))));
    }

    #[test]
    fn stores_dlls_and_files() {
        let mut e = env();
        let mut coop = entry("w/coop", "Coop", &[]);
        coop.kind = Kind::Setup;
        coop.needs.stores = vec![Store::Steam, Store::Gog];
        let c = Catalog::default();
        assert!(check(&coop, &c, &e).is_empty());
        e.games[0].store = Store::Ea;
        assert!(check(&coop, &c, &e).contains(&Issue::WrongStore(vec![Store::Steam, Store::Gog])));

        let mut needy = entry("a/new", "New", &[]);
        needy.info.as_mut().unwrap().dlls_build = Version::parse("2.6.0");
        assert!(check(&needy, &c, &e).contains(&Issue::OldDlls(Version::parse("2.6.0").unwrap())));

        let mut other = InstalledMod::new("Other");
        other.files.push(crate::installed::InstalledFile::new("New.package", crate::modinfo::Target::GalacticAdventures));
        e.installed.put(other);
        let base = entry("a/base", "Base", &[]);
        assert!(check(&base, &c, &e).iter().all(|i| !matches!(i, Issue::Replaces(_))));
        let replacing = entry("a/new2", "New", &[]);
        assert!(check(&replacing, &c, &e).contains(&Issue::Replaces(vec!["Other".into()])));
        e.lk = None;
        assert!(check(&base, &c, &e).contains(&Issue::NoLauncherKit));
    }

    #[test]
    fn mods_that_change_the_same_resources() {
        let with = |id: &str, unique: &str, depends: &[&str], resources: &[(u32, u64)]| {
            let mut e = entry(id, unique, depends);
            e.release.as_mut().unwrap().resources = resources.iter().map(|(i, h)| ((1, 0, *i), *h)).collect();
            e
        };
        let mut catalog = Catalog {
            generated: String::new(),
            mods: vec![
                with("a/faces", "Faces", &[], &[(1, 1), (2, 2)]),
                with("b/other-faces", "OtherFaces", &[], &[(2, 9)]),
                with("c/same-bytes", "Same", &[], &[(1, 1)]),
                with("d/addon", "Addon", &["Faces"], &[(2, 7)]),
            ],
        };
        crate::catalog::mark_overlaps(&mut catalog.mods);
        let ids = |e: &Entry| e.overlaps.iter().map(|o| o.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&catalog.mods[0]), ["b/other-faces"]);
        assert_eq!(ids(&catalog.mods[1]), ["a/faces", "d/addon"]);
        assert!(catalog.mods[2].overlaps.is_empty());

        let mut e = env();
        e.installed.put(InstalledMod::new("OtherFaces"));
        assert!(check(&catalog.mods[0], &catalog, &e).contains(&Issue::Overlaps(vec!["OtherFaces".into()])));
        assert!(!check(&catalog.mods[2], &catalog, &e).iter().any(|i| matches!(i, Issue::Overlaps(_))));
    }

    #[test]
    fn what_depends_on_a_mod() {
        let mut list = InstalledMods::default();
        let mut paints = InstalledMod::new("Paints");
        paints.dependencies = vec!["UiFixer".into()];
        list.put(paints);
        list.put(InstalledMod::new("UiFixer"));
        assert_eq!(dependents("UiFixer", &list), vec!["Paints"]);
        assert!(dependents("Paints", &list).is_empty());
    }
}
