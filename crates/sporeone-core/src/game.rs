//! Where Spore is, which copy it is, and whether mods can run on it. The Launcher Kit finds the
//! game through the registry keys Spore's installers write (`Electronic Arts\SPORE` and
//! `SPORE_EP1`), which Steam writes only when the game is started once; SporeOne also looks in
//! Steam's and GOG's own records, so it can say that instead of "not found".

use crate::version::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The builds of Galactic Adventures' `SporeApp.exe` the Launcher Kit supports, told apart by the
/// executable's size as the Launcher Kit does (`GameVersion.cs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edition {
    /// 3.0.0.2818, July 2009: the disc with patch 5.1.
    Disc,
    OriginMarch2017,
    EaOctober2024,
    /// 3.1.0.22: Steam and GOG share this build.
    SteamGogMarch2017,
    GogOctober2024,
    SteamOctober2024,
}

impl Edition {
    pub fn from_size(size: u64) -> Option<Edition> {
        Some(match size {
            24_909_584 => Edition::Disc,
            24_898_224 => Edition::OriginMarch2017,
            24_906_040 => Edition::EaOctober2024,
            24_885_248 => Edition::SteamGogMarch2017,
            24_895_536 => Edition::GogOctober2024,
            25_066_744 => Edition::SteamOctober2024,
            _ => return None,
        })
    }

    pub fn store(self) -> Store {
        match self {
            Edition::Disc => Store::Disc,
            Edition::OriginMarch2017 | Edition::EaOctober2024 => Store::Ea,
            Edition::GogOctober2024 => Store::Gog,
            Edition::SteamOctober2024 => Store::Steam,
            Edition::SteamGogMarch2017 => Store::Unknown,
        }
    }

    /// As the Launcher Kit names it.
    pub fn label(self) -> &'static str {
        match self {
            Edition::Disc => "Disc + Patch 5.1, July 2009",
            Edition::OriginMarch2017 => "Origin, March 2017",
            Edition::EaOctober2024 => "EA app, October 2024",
            Edition::SteamGogMarch2017 => "GOG/Steam, March 2017",
            Edition::GogOctober2024 => "GOG, October 2024",
            Edition::SteamOctober2024 => "Steam, October 2024",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Store {
    Steam,
    Gog,
    Ea,
    Disc,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    /// The folder with `Data`, `DataEP1` and `SporebinEP1`.
    pub root: PathBuf,
    pub spore_data: Option<PathBuf>,
    /// Galactic Adventures' `DataEP1`, where mods go. Missing without the expansion.
    pub ga_data: Option<PathBuf>,
    pub ga_exe: Option<PathBuf>,
    pub edition: Option<Edition>,
    pub exe_version: Option<Version>,
    pub store: Store,
    /// The registry says where the game is, as the Launcher Kit needs.
    pub registered: bool,
    /// SporeModLoader's `dinput8.dll` is in `SporebinEP1`: the Launcher Kit refuses to start the
    /// game while it is there.
    pub mod_loader: bool,
}

impl Game {
    pub fn has_ga(&self) -> bool {
        self.ga_data.is_some() && self.ga_exe.is_some()
    }

    /// Looks at one folder as a Spore install. `registered` data folders win over the usual names.
    pub fn at(root: &Path, spore_data: Option<PathBuf>, ga_data: Option<PathBuf>, registered: bool) -> Game {
        let dir = |name: &str| Some(root.join(name)).filter(|p| p.is_dir());
        let spore_data = spore_data.filter(|p| p.is_dir()).or_else(|| dir("Data"));
        let ga_data = ga_data.filter(|p| p.is_dir()).or_else(|| dir("DataEP1"));
        let bin = root.join("SporebinEP1");
        let exe = Some(bin.join("SporeApp.exe")).filter(|p| p.is_file());
        let edition = exe.as_ref().and_then(|e| std::fs::metadata(e).ok()).and_then(|m| Edition::from_size(m.len()));
        let exe_version = exe.as_deref().and_then(crate::win::file_version);
        let store = match edition.map(Edition::store) {
            Some(s) if s != Store::Unknown => s,
            _ => store_by_files(root),
        };
        Game {
            root: root.to_path_buf(),
            spore_data,
            ga_data,
            ga_exe: exe,
            edition,
            exe_version,
            store,
            registered,
            mod_loader: is_mod_loader(&bin.join("dinput8.dll")),
        }
    }
}

/// Which store a folder came from, by what each leaves next to the game.
fn store_by_files(root: &Path) -> Store {
    let has = |p: &str| root.join(p).exists();
    let lower = root.to_string_lossy().to_ascii_lowercase();
    if lower.contains("steamapps") || has("SporebinEP1/steam_api.dll") && has("SporebinEP1/steam_appid.txt") {
        return Store::Steam;
    }
    let gog = std::fs::read_dir(root).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().starts_with("goggame-"));
    if gog {
        return Store::Gog;
    }
    if has("__Installer") {
        return Store::Ea;
    }
    Store::Unknown
}

/// The Launcher Kit's own test: the version resource names SporeModLoader.
fn is_mod_loader(dll: &Path) -> bool {
    if !dll.is_file() {
        return false;
    }
    // Without a version resource reader the name inside the file still gives it away.
    std::fs::read(dll).is_ok_and(|b| {
        let needle: Vec<u8> = "SporeModLoader".encode_utf16().flat_map(u16::to_le_bytes).collect();
        b.windows(needle.len()).any(|w| w == needle.as_slice())
    })
}

/// Every Spore install found, the one the registry names first. More than one is a problem the
/// Launcher Kit warns about too.
pub fn find() -> Vec<Game> {
    if let Some(root) = std::env::var_os("SPOREONE_GAME_ROOT") {
        let root = PathBuf::from(root);
        return vec![Game::at(&root, None, None, true)];
    }
    let mut found: Vec<Game> = Vec::new();
    let mut add = |g: Game| {
        let key = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase();
        if !found.iter().any(|f| key(&f.root) == key(&g.root)) {
            found.push(g);
        }
    };
    #[cfg(windows)]
    {
        use crate::win::{Hive, reg_string};
        let data = |k: &str| {
            reg_string(Hive::Machine, &format!(r"SOFTWARE\Electronic Arts\{k}"), "DataDir")
                .map(|p| PathBuf::from(p.trim_matches('"').trim_end_matches(['\\', '/'])))
        };
        let (spore, ga) = (data("SPORE"), data("SPORE_EP1"));
        if let Some(root) = ga.as_ref().or(spore.as_ref()).and_then(|p| p.parent()) {
            add(Game::at(root, spore.clone(), ga.clone(), ga.is_some()));
        }
        for root in steam_roots().into_iter().chain(gog_roots()) {
            add(Game::at(&root, None, None, false));
        }
    }
    // A folder picked by hand comes first, as found above when it was there too.
    if let Some(dir) = crate::state::State::load().game_dir.filter(|d| is_game_root(d)) {
        let key = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase();
        let game = match found.iter().position(|g| key(&g.root) == key(&dir)) {
            Some(i) => found.remove(i),
            None => Game::at(&dir, None, None, false),
        };
        found.insert(0, game);
    }
    found
}

/// Whether `dir` is a Spore folder: the one holding `SporebinEP1` or `Data`.
pub fn is_game_root(dir: &Path) -> bool {
    dir.join("SporebinEP1").is_dir() || dir.join("Data").is_dir()
}

/// The game's folder from one picked inside it, such as `SporebinEP1` or `DataEP1`.
pub fn game_root(picked: &Path) -> Option<PathBuf> {
    picked.ancestors().take(2).find(|d| is_game_root(d)).map(Path::to_path_buf)
}

/// `steamapps\common\Spore` in every Steam library that has Spore or Galactic Adventures.
#[cfg(windows)]
fn steam_roots() -> Vec<PathBuf> {
    use crate::win::{Hive, reg_string};
    let Some(steam) = reg_string(Hive::User, r"Software\Valve\Steam", "SteamPath") else { return Vec::new() };
    let mut libraries = vec![PathBuf::from(&steam)];
    if let Ok(vdf) = std::fs::read_to_string(Path::new(&steam).join("steamapps/libraryfolders.vdf")) {
        libraries.extend(vdf_values(&vdf, "path").into_iter().map(PathBuf::from));
    }
    let mut roots = Vec::new();
    for lib in libraries {
        let apps = lib.join("steamapps");
        let owned = ["17390", "24720"].iter().any(|id| apps.join(format!("appmanifest_{id}.acf")).is_file());
        let root = apps.join("common").join("Spore");
        if owned && root.is_dir() && !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

#[cfg(windows)]
fn gog_roots() -> Vec<PathBuf> {
    use crate::win::{Hive, reg_string};
    // Galactic Adventures', Spore's and the SPORE Collection's GOG ids.
    ["1207659142", "1207659141", "1948823323"]
        .iter()
        .filter_map(|id| reg_string(Hive::Machine, &format!(r"SOFTWARE\GOG.com\Games\{id}"), "path"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect()
}

/// The values of every `"key" "value"` pair named `key` in a Valve KeyValues file, with its
/// escaped backslashes undone.
pub fn vdf_values(vdf: &str, key: &str) -> Vec<String> {
    let quoted = format!("\"{key}\"");
    vdf.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix(&quoted)?.trim();
            let v = rest.strip_prefix('"')?.strip_suffix('"')?;
            Some(v.replace("\\\\", "\\"))
        })
        .collect()
}

/// Files the game itself ships in its data folders. SporeOne never deletes or replaces one, even
/// when a mod's `ModInfo.xml` asks it to.
pub fn is_game_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("spore_")
        || n.starts_with("ep1_")
        || n.starts_with("boosterpack")
        || n == "patchdata.package"
        || n == "patchdataep1.package"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_picked_inside_the_game_finds_its_root() {
        let root = std::env::temp_dir().join(format!("sporeone-root-{}", std::process::id()));
        std::fs::create_dir_all(root.join("SporebinEP1")).unwrap();
        assert_eq!(game_root(&root), Some(root.clone()));
        assert_eq!(game_root(&root.join("SporebinEP1")), Some(root.clone()));
        assert_eq!(game_root(root.parent().unwrap()), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reads_steam_library_folders() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}";
        assert_eq!(vdf_values(vdf, "path"), vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
    }

    #[test]
    fn editions_by_size() {
        assert_eq!(Edition::from_size(25_066_744), Some(Edition::SteamOctober2024));
        assert_eq!(Edition::from_size(24_885_248).map(Edition::store), Some(Store::Unknown));
        assert_eq!(Edition::from_size(20_626_168), None);
    }

    #[test]
    fn the_games_own_files_are_known() {
        for f in ["Spore_Content.package", "PatchData.package", "EP1_PatchData.package", "Spore_EP1_Data.package"] {
            assert!(is_game_file(f), "{f}");
        }
        assert!(!is_game_file("SporeCrashFix.package"));
    }

    #[test]
    fn a_folder_without_galactic_adventures() {
        let root = std::env::temp_dir().join(format!("sporeone-game-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        std::fs::create_dir_all(root.join("SporeBin")).unwrap();
        let g = Game::at(&root, None, None, false);
        assert!(g.spore_data.is_some() && !g.has_ga());
        std::fs::create_dir_all(root.join("DataEP1")).unwrap();
        std::fs::create_dir_all(root.join("SporebinEP1")).unwrap();
        std::fs::write(root.join("SporebinEP1/SporeApp.exe"), b"MZ").unwrap();
        let g = Game::at(&root, None, None, true);
        assert!(g.has_ga() && g.edition.is_none() && !g.mod_loader);
        let _ = std::fs::remove_dir_all(&root);
    }
}
