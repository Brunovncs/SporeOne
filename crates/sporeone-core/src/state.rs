//! What SporeOne keeps for itself, in `%LOCALAPPDATA%\SporeOne`: the last catalog downloaded,
//! downloads, the folders mods are unpacked in, and which release each mod came from (the Launcher
//! Kit's list has no room for it, and mods without a version need it to know of an update).

use crate::catalog::{Catalog, Entry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SPOREONE_DATA_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("SporeOne")
}

pub fn downloads() -> PathBuf {
    data_dir().join("downloads")
}

pub fn staging() -> PathBuf {
    data_dir().join("staging")
}

/// A mod installed from the catalog: the Launcher Kit's name for it, and the release.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub id: String,
    pub tag: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// By the Launcher Kit's name for the mod.
    pub origins: BTreeMap<String, Origin>,
    pub lang: Option<String>,
    /// Mods added from a link, shown with the catalog's until the catalog has them.
    pub custom: Vec<Entry>,
    /// The catalog as tiles rather than a list.
    pub grid: bool,
    /// Folders picked by hand, for a game or a Launcher Kit SporeOne did not find.
    pub game_dir: Option<PathBuf>,
    pub lk_dir: Option<PathBuf>,
}

impl State {
    fn path() -> PathBuf {
        data_dir().join("state.json")
    }

    pub fn load() -> State {
        std::fs::read(State::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = State::path();
        std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }
}

/// The newest catalog at hand: the one downloaded last, or the one built in.
pub fn cached_catalog() -> Catalog {
    let built_in = Catalog::built_in();
    match std::fs::read_to_string(data_dir().join("catalog.json")).ok().and_then(|s| Catalog::parse(&s).ok()) {
        Some(c) if c.generated >= built_in.generated => c,
        _ => built_in,
    }
}

/// Downloads the catalog and keeps it for next time.
pub fn refresh_catalog() -> Result<Catalog, String> {
    let url = std::env::var("SPOREONE_CATALOG_URL").unwrap_or_else(|_| crate::catalog::URL.to_string());
    let bytes = crate::net::get(&url, &[])?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let catalog = Catalog::parse(&text)?;
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(data_dir().join("catalog.json"), text);
    Ok(catalog)
}
