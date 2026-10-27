//! The catalog: every mod SporeOne offers, as one JSON file. `sporeone-catalog` builds it from the
//! GitHub repositories tagged `spore-mod` (the ones the Spore Mod Browser lists) and from
//! `catalog/curated.json`, kept by hand, which adds what GitHub cannot say: mods published
//! elsewhere, mods with their own installer, and what is known about compatibility. A copy built
//! into SporeOne is used until a newer one is downloaded.

use crate::game::Store;
use crate::modinfo::ModInfo;
use crate::version::Version;
use serde::{Deserialize, Serialize};

/// Where SporeOne downloads the catalog from: a branch the catalog workflow keeps up to date.
pub const URL: &str = "https://raw.githubusercontent.com/Brunovncs/SporeOne/catalog/catalog.json";
/// The copy built in, from the last time the catalog was built before this release.
pub const BUILT_IN: &str = include_str!("../../../catalog/catalog.json");

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    /// When it was built, as an ISO 8601 date and time.
    pub generated: String,
    pub mods: Vec<Entry>,
}

/// How a mod is installed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A `.sporemod` or `.package` from its GitHub release, installed into the Launcher Kit by
    /// SporeOne.
    #[default]
    Release,
    /// A mod with an installer of its own, which SporeOne downloads and starts.
    Setup,
    /// Published on a site SporeOne does not download from: its page opens, and the file
    /// downloaded there can be dropped on SporeOne's window.
    Page,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    #[default]
    Gameplay,
    Fixes,
    Editors,
    Visuals,
    Multiplayer,
    Tools,
}

impl Category {
    pub const ALL: [Category; 6] =
        [Category::Gameplay, Category::Fixes, Category::Editors, Category::Visuals, Category::Multiplayer, Category::Tools];

    /// A guess from what a mod says about itself, for mods the curated list does not place.
    pub fn guess(text: &str) -> Category {
        let t = text.to_lowercase();
        let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
        if any(&["multiplayer", "co-op", "coop", "online play"]) {
            Category::Multiplayer
        } else if any(&["fix", "crash", "bug", "borderless", "stability"]) {
            Category::Fixes
        } else if any(&["editor", "parts", "paint", "creature creator", "color", "colour"]) {
            Category::Editors
        } else if any(&["texture", "graphic", "shader", "visual", "hd ", "lighting"]) {
            Category::Visuals
        } else if any(&["tool", "library", "framework", "rich presence", "discord"]) {
            Category::Tools
        } else {
            Category::Gameplay
        }
    }
}

/// How much a mod can be trusted to work today.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Maintained,
    /// Work in progress, by its author's own account.
    Experimental,
    /// Not updated in years: it may still work.
    Old,
    /// Known not to work with the Launcher Kit or the game versions of today.
    Broken,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    /// The release's tag, as the author wrote it.
    pub tag: String,
    pub published: String,
    pub url: String,
    pub file: String,
    pub size: u64,
    pub downloads: u64,
    /// Filled in by the catalog's builder, which downloads every file once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// What a mod needs to run, beyond what its `ModInfo.xml` says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Needs {
    /// The stores whose copy it works with; any when empty.
    pub stores: Vec<Store>,
    pub galactic_adventures: bool,
    pub launcher_kit: bool,
}

impl Default for Needs {
    fn default() -> Needs {
        Needs { stores: Vec::new(), galactic_adventures: true, launcher_kit: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    /// `owner/repo` for a GitHub mod, a name of its own for the others.
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub kind: Kind,
    pub category: Category,
    pub status: Status,
    /// The GitHub repository, or the page it is published on.
    pub page: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Its `ModInfo.xml`, read from the repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<ModInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<Release>,
    pub stars: u32,
    /// When its repository last changed.
    pub updated: String,
    pub needs: Needs,
    /// Catalog ids or unique names of mods it does not work with.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
    /// Notes from the curated list, in English and Portuguese.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    /// For a mod with its own installer: a file, relative to the game's folder, that is there
    /// once it is installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// For a mod with its own installer: the file in its release to run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub en: String,
    pub pt: String,
}

impl Entry {
    /// The name the Launcher Kit knows it by, when it has a `ModInfo.xml`.
    pub fn unique(&self) -> Option<&str> {
        self.info.as_ref().map(|i| i.unique.as_str())
    }

    /// Its version: the one its `ModInfo.xml` gives, else its release's tag when it reads as one.
    pub fn version(&self) -> Option<Version> {
        self.info.as_ref().and_then(|i| i.version).or_else(|| self.release.as_ref().and_then(|r| Version::parse(&r.tag)))
    }

    pub fn version_text(&self) -> Option<String> {
        self.info.as_ref().and_then(|i| i.version).map(|v| v.to_string()).or_else(|| self.release.as_ref().map(|r| r.tag.clone()))
    }

    pub fn downloads(&self) -> u64 {
        self.release.as_ref().map_or(0, |r| r.downloads)
    }

    pub fn installable(&self) -> bool {
        match self.kind {
            Kind::Release => self.release.is_some(),
            Kind::Setup => self.release.is_some() && self.setup.is_some(),
            Kind::Page => false,
        }
    }

    /// Whether `query` is in its name, author or description, without case.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        q.is_empty()
            || [&self.name, &self.author, &self.description].iter().any(|s| s.to_lowercase().contains(&q))
            || self.unique().is_some_and(|u| u.to_lowercase().contains(&q))
    }
}

impl Catalog {
    pub fn parse(json: &str) -> Result<Catalog, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn built_in() -> Catalog {
        Catalog::parse(BUILT_IN).unwrap_or_default()
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.mods.iter().find(|m| m.id == id)
    }

    /// The entry a unique name belongs to.
    pub fn by_unique(&self, unique: &str) -> Option<&Entry> {
        self.mods.iter().find(|m| m.unique() == Some(unique))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_catalog_reads() {
        let c = Catalog::parse(BUILT_IN).unwrap();
        assert!(!c.mods.is_empty());
        let mut ids: Vec<&str> = c.mods.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.mods.len(), "every id is listed once");
        for m in &c.mods {
            assert!(!m.name.is_empty(), "{} has a name", m.id);
            if m.kind == Kind::Setup {
                assert!(m.setup.is_some() && m.marker.is_some(), "{} says what to run and how to tell it is in", m.id);
            }
        }
    }

    #[test]
    fn categories_from_descriptions() {
        assert_eq!(Category::guess("Fixes multiple crashes"), Category::Fixes);
        assert_eq!(Category::guess("Co-op multiplayer across all stages"), Category::Multiplayer);
        assert_eq!(Category::guess("Adds a color wheel to the editor"), Category::Editors);
        assert_eq!(Category::guess("Adds mining and crafting"), Category::Gameplay);
    }
}
