//! The Launcher Kit's list of installed mods, `InstalledMods.config` in its folder. SporeOne reads
//! and writes it the way the Easy Installer does, so a mod installed here shows in the Easy
//! Uninstaller, and one installed there shows here.

use crate::modinfo::Target;
use crate::version::Version;
use std::path::Path;

pub const FILE_NAME: &str = "InstalledMods.config";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledFile {
    pub name: String,
    /// `GalacticAdventures`, `Spore` or `None` (the Launcher Kit's folder or its `mLibs`), kept as
    /// written: an older Launcher Kit wrote other values.
    pub game: String,
}

impl InstalledFile {
    pub fn new(name: impl Into<String>, target: Target) -> InstalledFile {
        InstalledFile { name: name.into(), game: target.record_name().to_string() }
    }

    pub fn target(&self) -> Option<Target> {
        match self.game.as_str() {
            "GalacticAdventures" => Some(Target::GalacticAdventures),
            "Spore" => Some(Target::Spore),
            "None" => Some(Target::LauncherKit),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledMod {
    /// The installed file's name without its extension: what the Launcher Kit keys a mod by.
    pub name: String,
    pub unique: String,
    pub version: Option<Version>,
    pub display_name: Option<String>,
    /// Its `ModInfo.xml` under `ModConfigs`, for a mod with choices: the Easy Installer opens it
    /// again to change them or to uninstall.
    pub configurator: Option<String>,
    pub dependencies: Vec<String>,
    pub dependencies_versions: Vec<Version>,
    pub files: Vec<InstalledFile>,
}

impl InstalledMod {
    pub fn new(name: impl Into<String>) -> InstalledMod {
        let name = name.into();
        InstalledMod {
            unique: name.clone(),
            name,
            version: None,
            display_name: None,
            configurator: None,
            dependencies: Vec::new(),
            dependencies_versions: Vec::new(),
            files: Vec::new(),
        }
    }

    pub fn shown_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstalledMods {
    pub mods: Vec<InstalledMod>,
}

impl InstalledMods {
    pub fn read(dir: &Path) -> Result<InstalledMods, String> {
        match std::fs::read(dir.join(FILE_NAME)) {
            Ok(bytes) => InstalledMods::parse(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(InstalledMods::default()),
            Err(e) => Err(format!("could not read {FILE_NAME}: {e}")),
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<InstalledMods, String> {
        let text = crate::modinfo::decode(bytes)?;
        if text.trim().is_empty() {
            return Ok(InstalledMods::default());
        }
        let doc = roxmltree::Document::parse(&text).map_err(|e| format!("{FILE_NAME}: {e}"))?;
        let mut mods = Vec::new();
        for node in doc.root_element().children().filter(|n| n.has_tag_name("mod")) {
            let name = node.attribute("name").unwrap_or_default().to_string();
            let split = |a: &str| node.attribute(a).map(|v| v.split('?').map(String::from).collect::<Vec<_>>()).unwrap_or_default();
            // As the Launcher Kit does: one version it cannot read drops them all.
            let versions: Option<Vec<Version>> = split("dependenciesVersions").iter().map(|v| Version::parse(v)).collect();
            mods.push(InstalledMod {
                unique: node.attribute("unique").map(String::from).unwrap_or_else(|| name.clone()),
                version: node.attribute("version").and_then(Version::parse),
                display_name: node.attribute("displayName").map(String::from),
                configurator: node.attribute("configurator").map(String::from),
                dependencies: split("dependencies").into_iter().filter(|d| !d.is_empty()).collect(),
                dependencies_versions: versions.unwrap_or_default(),
                files: node
                    .children()
                    .filter(|n| n.has_tag_name("file"))
                    .map(|f| InstalledFile {
                        name: f.text().unwrap_or_default().to_string(),
                        game: f.attribute("game").unwrap_or("None").to_string(),
                    })
                    .collect(),
                name,
            });
        }
        Ok(InstalledMods { mods })
    }

    /// The file as the Easy Installer saves it: indented by two spaces, UTF-8 with its mark.
    pub fn to_xml(&self) -> String {
        let mut s = String::from("\u{feff}<InstalledMods>\r\n");
        for m in &self.mods {
            s.push_str(&format!("  <mod name=\"{}\" unique=\"{}\"", esc(&m.name), esc(&m.unique)));
            if let Some(v) = m.version {
                s.push_str(&format!(" version=\"{v}\""));
            }
            if !m.dependencies.is_empty() {
                s.push_str(&format!(" dependencies=\"{}\"", esc(&m.dependencies.join("?"))));
            }
            if !m.dependencies_versions.is_empty() {
                let v = m.dependencies_versions.iter().map(Version::to_string).collect::<Vec<_>>().join("?");
                s.push_str(&format!(" dependenciesVersions=\"{v}\""));
            }
            if let Some(c) = &m.configurator {
                s.push_str(&format!(" configurator=\"{}\"", esc(c)));
            }
            if let Some(d) = m.display_name.as_ref().filter(|d| **d != m.name) {
                s.push_str(&format!(" displayName=\"{}\"", esc(d)));
            }
            if m.files.is_empty() {
                s.push_str(" />\r\n");
                continue;
            }
            s.push_str(">\r\n");
            for f in &m.files {
                if f.game == "None" {
                    s.push_str(&format!("    <file>{}</file>\r\n", esc(&f.name)));
                } else {
                    s.push_str(&format!("    <file game=\"{}\">{}</file>\r\n", esc(&f.game), esc(&f.name)));
                }
            }
            s.push_str("  </mod>\r\n");
        }
        s.push_str("</InstalledMods>");
        s
    }

    pub fn find(&self, name_or_unique: &str) -> Option<&InstalledMod> {
        self.mods.iter().find(|m| m.name == name_or_unique).or_else(|| self.mods.iter().find(|m| m.unique == name_or_unique))
    }

    /// Takes the place of every mod with the same name or unique name, as an update does.
    pub fn put(&mut self, m: InstalledMod) {
        self.mods.retain(|o| o.name != m.name && o.unique != m.unique);
        self.mods.push(m);
    }

    pub fn remove(&mut self, name: &str) {
        self.mods.retain(|m| m.name != name);
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the Launcher Kit 1.5 writes it, with a mod with choices and two without.
    const WRITTEN: &str = "\u{feff}<InstalledMods>\r\n  <mod name=\"CrashFix\" unique=\"SporeCrashFix\" version=\"1.3.0\">\r\n    \
        <file>SporeCrashFix.dll</file>\r\n  </mod>\r\n  <mod name=\"Paints\" unique=\"Paints\" dependencies=\"AdvancedCreaturePaint?UiFixer\" \
        dependenciesVersions=\"2.1?2.1\" configurator=\"C:\\LK\\ModConfigs\\Paints\\ModInfo.xml\" displayName=\"Pride &amp; Pattern Paints\">\r\n    \
        <file>Paints</file>\r\n  </mod>\r\n  <mod name=\"old\" unique=\"old\">\r\n    <file game=\"GalacticAdventures\">old.package</file>\r\n  </mod>\r\n</InstalledMods>";

    #[test]
    fn reads_and_writes_the_same_file() {
        let list = InstalledMods::parse(WRITTEN.as_bytes()).unwrap();
        assert_eq!(list.mods.len(), 3);
        let paints = list.find("Paints").unwrap();
        assert_eq!(paints.shown_name(), "Pride & Pattern Paints");
        assert_eq!(paints.dependencies_versions, vec![Version::parse("2.1").unwrap(); 2]);
        assert_eq!(list.mods[0].files[0].target(), Some(Target::LauncherKit));
        assert_eq!(list.mods[2].files[0].target(), Some(Target::GalacticAdventures));
        assert_eq!(list.find("SporeCrashFix").unwrap().name, "CrashFix");
        assert_eq!(list.to_xml(), WRITTEN);
    }

    #[test]
    fn an_update_replaces_the_old_entry() {
        let mut list = InstalledMods::parse(WRITTEN.as_bytes()).unwrap();
        let mut m = InstalledMod::new("SporeCrashFix-1.4");
        m.unique = "SporeCrashFix".into();
        list.put(m);
        assert_eq!(list.mods.len(), 3);
        assert!(list.find("CrashFix").is_none());
        assert!(InstalledMods::parse(b"").unwrap().mods.is_empty());
    }
}
