//! `--demo`: a made-up Spore and Launcher Kit in a temporary folder, with a few mods already in,
//! so the window shows every state for screenshots and can be tried without the game. Installing
//! from it is real, into that folder.

use sporeone_core::catalog::Catalog;
use sporeone_core::compat::Env;
use sporeone_core::game::{Edition, Store};
use sporeone_core::installed::{InstalledFile, InstalledMod, InstalledMods};
use sporeone_core::modinfo::Target;
use sporeone_core::state::Origin;
use sporeone_core::version::Version;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::temp_dir().join("sporeone-demo")
}

/// Mods shown as installed: one a version behind, one current, and one installed by hand.
const BEHIND: &str = "Rosalie241/SporeCrashFix";
const CURRENT: &str = "emd4600/EnhancedColorPickerMod";

/// Makes the folder and points SporeOne at it.
pub fn sandbox() {
    let root = root();
    let _ = std::fs::remove_dir_all(&root);
    let game = root.join("Spore");
    let lk = root.join("Spore ModAPI Launcher Kit");
    for d in [game.join("Data"), game.join("DataEP1"), game.join("SporebinEP1"), lk.join("coreLibs"), root.join("data")] {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(game.join("SporebinEP1/SporeApp.exe"), b"MZ");
    let _ = std::fs::write(game.join("SporebinEP1/sporekit.dll"), b"MZ");
    let _ = std::fs::write(game.join("Data/Spore_Content.package"), b"demo");
    let _ = std::fs::write(game.join("DataEP1/Spore_EP1_Data.package"), b"demo");
    let _ = std::fs::write(lk.join(sporeone_core::launcher_kit::LAUNCHER_EXE), b"MZ");
    let catalog = Catalog::built_in();
    let mut list = InstalledMods::default();
    for (id, behind) in [(BEHIND, true), (CURRENT, false)] {
        let Some(e) = catalog.get(id) else { continue };
        let name = e.release.as_ref().and_then(|r| r.file.rsplit_once('.')).map_or(e.name.clone(), |(s, _)| s.to_string());
        let mut m = InstalledMod::new(&name);
        m.unique = e.unique().unwrap_or(&name).to_string();
        m.display_name = Some(e.name.clone());
        // A version behind: no version of its own, and an older release noted as its origin.
        m.version = if behind { None } else { e.version() };
        m.files.push(InstalledFile::new(format!("{name}.dll"), Target::LauncherKit));
        list.put(m);
    }
    let mut own = InstalledMod::new("BiggerGalaxyNames");
    own.display_name = Some("Bigger Galaxy Names".into());
    own.files.push(InstalledFile::new("BiggerGalaxyNames.package", Target::GalacticAdventures));
    list.put(own);
    let _ = std::fs::write(lk.join(sporeone_core::installed::FILE_NAME), list.to_xml());
    // SAFETY: set once, before the window and its threads start.
    unsafe {
        std::env::set_var("SPOREONE_GAME_ROOT", &game);
        std::env::set_var("SPOREONE_LK_DIR", &lk);
        std::env::set_var("SPOREONE_DATA_DIR", root.join("data"));
    }
}

/// The folder as a real Steam copy of today's build would read.
pub fn env() -> Env {
    let mut env = Env::read();
    for g in &mut env.games {
        g.edition = Some(Edition::SteamOctober2024);
        g.exe_version = Some(Version::new(3, 1, 0, 29));
        g.store = Store::Steam;
        g.registered = true;
    }
    if let Some(lk) = &mut env.lk {
        lk.version = Some(Version::new(1, 5, 13, 0));
        lk.dlls_build = Some(Version::new(2, 5, 579, 0));
    }
    env
}

pub fn catalog() -> Catalog {
    Catalog::built_in()
}

pub fn origins() -> BTreeMap<String, Origin> {
    let catalog = Catalog::built_in();
    [BEHIND, CURRENT]
        .iter()
        .filter_map(|id| catalog.get(id))
        .filter_map(|e| {
            let r = e.release.as_ref()?;
            let name = r.file.rsplit_once('.').map_or(e.name.clone(), |(s, _)| s.to_string());
            let tag = if e.id == BEHIND { "v0.1".to_string() } else { r.tag.clone() };
            Some((name, Origin { id: e.id.clone(), tag }))
        })
        .collect()
}
