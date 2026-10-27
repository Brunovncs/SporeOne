//! Real mods from the built-in catalog, downloaded, checked against their hash, installed into a
//! made-up game and Launcher Kit, and uninstalled. Needs the internet, so it is left out of the
//! default run: `cargo test -p sporeone-core --test real_mods -- --ignored`.

use sporeone_core::catalog::{Catalog, Kind};
use sporeone_core::compat::Env;
use sporeone_core::install::Roots;
use sporeone_core::installed::InstalledMods;
use sporeone_core::launcher_kit::LauncherKit;
use sporeone_core::{actions, game, install};
use std::path::Path;

fn fake(root: &Path) -> Env {
    let _ = std::fs::remove_dir_all(root);
    let g = root.join("Spore");
    let lk = root.join("LK");
    for d in [g.join("Data"), g.join("DataEP1"), g.join("SporebinEP1"), lk.clone()] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(g.join("SporebinEP1/SporeApp.exe"), b"MZ").unwrap();
    std::fs::write(g.join("DataEP1/Spore_EP1_Data.package"), b"game").unwrap();
    std::fs::write(lk.join(sporeone_core::launcher_kit::LAUNCHER_EXE), b"MZ").unwrap();
    let mut game = game::Game::at(&g, None, None, true);
    game.edition = Some(game::Edition::SteamOctober2024);
    Env {
        games: vec![game],
        lk: Some(LauncherKit { dir: lk, version: None, dlls_build: sporeone_core::version::Version::parse("2.5.579") }),
        installed: InstalledMods::default(),
    }
}

#[test]
#[ignore]
fn installs_and_removes_popular_mods() {
    let root = std::env::temp_dir().join(format!("sporeone-real-{}", std::process::id()));
    // Downloads and unpacking go under the same folder.
    unsafe { std::env::set_var("SPOREONE_DATA_DIR", root.join("data")) };
    let mut env = fake(&root);
    let roots: Roots = env.roots().unwrap();
    let catalog = Catalog::built_in();
    let picks: Vec<_> = catalog.mods.iter().filter(|e| e.kind == Kind::Release && e.installable()).take(12).collect();
    assert!(picks.len() >= 10);
    let mut installed = Vec::new();
    for e in &picks {
        match actions::install_entry(e, &catalog, &env, None) {
            Ok(names) => {
                println!("installed {} as {names:?}", e.id);
                installed.extend(names);
            }
            Err(err) => println!("refused {}: {err}", e.id),
        }
        env.installed = InstalledMods::read(&roots.lk).unwrap();
    }
    assert!(installed.len() >= 8, "most popular mods install: {installed:?}");
    let list = InstalledMods::read(&roots.lk).unwrap();
    for name in &installed {
        let m = list.find(name).unwrap_or_else(|| panic!("{name} is in the Launcher Kit's list"));
        for f in &m.files {
            let at = match f.target() {
                Some(sporeone_core::modinfo::Target::GalacticAdventures) => roots.ga_data.clone().unwrap().join(&f.name),
                Some(sporeone_core::modinfo::Target::Spore) => roots.spore_data.clone().unwrap().join(&f.name),
                _ => roots.lk.join("mLibs").join(&f.name),
            };
            let exists = at.exists() || roots.lk.join(&f.name).exists() || m.configurator.is_some();
            assert!(exists, "{} of {name} is in place", f.name);
        }
    }
    for name in &installed {
        actions::uninstall(name, &env).unwrap();
        env.installed = InstalledMods::read(&roots.lk).unwrap();
    }
    assert!(InstalledMods::read(&roots.lk).unwrap().mods.is_empty());
    let left: Vec<_> = std::fs::read_dir(roots.ga_data.as_ref().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(left.len(), 1, "only the game's own file is left: {left:?}");
    let mlibs: Vec<_> = std::fs::read_dir(roots.lk.join("mLibs")).into_iter().flatten().flatten().map(|e| e.file_name()).collect();
    assert!(mlibs.is_empty(), "no DLL is left: {mlibs:?}");
    assert!(install::check(&install::Plan::default(), &roots).is_ok());
    let _ = std::fs::remove_dir_all(&root);
}
