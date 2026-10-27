//! A mod's `ModInfo.xml`, the Launcher Kit's "XML Mod Identity": who the mod is, which Launcher
//! Kit it needs, and where each of its files goes. Every version the Launcher Kit installs is read
//! (1.0.0.0 to 1.0.1.3); a mod written for a newer one is refused, as the Launcher Kit does.

use crate::version::Version;
use serde::{Deserialize, Serialize};

/// The versions of the format the Launcher Kit knows.
pub const KNOWN_FORMATS: [Version; 5] =
    [Version::new(1, 0, 0, 0), Version::new(1, 0, 1, 0), Version::new(1, 0, 1, 1), Version::new(1, 0, 1, 2), Version::new(1, 0, 1, 3)];

/// Where a file goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    /// Galactic Adventures' `DataEP1` folder, where nearly every `.package` goes.
    GalacticAdventures,
    /// Core Spore's `Data` folder.
    Spore,
    /// The Launcher Kit's own folder (its `mLibs` folder from format 1.0.1.0 on): ModAPI `.dll`s.
    LauncherKit,
}

impl Target {
    /// The `game` attribute's value; anything else, or nothing, is the Launcher Kit's folder.
    pub fn from_attr(s: &str) -> Target {
        match s {
            "GalacticAdventures" => Target::GalacticAdventures,
            "Spore" => Target::Spore,
            _ => Target::LauncherKit,
        }
    }

    /// What the Launcher Kit writes in `InstalledMods.config`.
    pub fn record_name(self) -> &'static str {
        match self {
            Target::GalacticAdventures => "GalacticAdventures",
            Target::Spore => "Spore",
            Target::LauncherKit => "None",
        }
    }
}

/// Files and where each one goes, in pairs: `a.package?b.dll` with `GalacticAdventures?`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Files(pub Vec<(String, Target)>);

impl Files {
    fn parse(names: &str, games: Option<&str>) -> Files {
        let games: Vec<&str> = games.map(|g| g.split('?').collect()).unwrap_or_default();
        let list = names
            .split('?')
            .map(str::trim)
            .enumerate()
            .filter(|(_, n)| !n.is_empty())
            .map(|(i, n)| (n.to_string(), Target::from_attr(games.get(i).copied().unwrap_or(""))))
            .collect();
        Files(list)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub unique: String,
    pub display_name: String,
    pub description: Option<String>,
    pub default_checked: bool,
    pub files: Files,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Item {
    /// Always installed.
    Prerequisite(Files),
    /// Installed if chosen, when the mod has a custom installer.
    Component(Component),
    /// One of these, when the mod has a custom installer.
    Group { unique: String, display_name: String, components: Vec<Component> },
    /// Installed only when every target is there already, optionally removing the targets.
    Compat { files: Files, targets: Files, remove_targets: bool },
    /// Files removed before installing, by name with `*` wildcards.
    Remove(Files),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub unique: String,
    /// The oldest version that will do, or any when `None`.
    pub min: Option<Version>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModInfo {
    pub unique: String,
    pub display_name: String,
    pub description: Option<String>,
    pub format: Version,
    pub version: Option<Version>,
    /// The oldest ModAPI DLLs build the mod works with.
    pub dlls_build: Option<Version>,
    pub depends: Vec<Dependency>,
    pub has_custom_installer: bool,
    pub experimental: bool,
    pub requires_galaxy_reset: bool,
    pub save_data_dependency: bool,
    /// Format 1.0.0.0's `mode="compatOnly"`.
    pub compat_only: bool,
    pub items: Vec<Item>,
}

impl ModInfo {
    /// Format 1.0.1.0 and later put ModAPI DLLs in `mLibs`; 1.0.0.0 put three builds of each,
    /// `-disk`, `-steam` and `-steam_patched`, in the Launcher Kit's folder.
    pub fn legacy_dlls(&self) -> bool {
        self.format == Version::new(1, 0, 0, 0)
    }

    /// Whether the Launcher Kit would show its own installer window, with choices, for it:
    /// format 1.0.0.0 always does unless the mod is `compatOnly`.
    pub fn has_choices(&self) -> bool {
        if self.legacy_dlls() { !self.compat_only } else { self.has_custom_installer }
    }

    pub fn components(&self) -> impl Iterator<Item = &Component> {
        self.items.iter().flat_map(|i| match i {
            Item::Component(c) => std::slice::from_ref(c).iter(),
            Item::Group { components, .. } => components.iter(),
            _ => [].iter(),
        })
    }
}

/// Values the Mod Browser leaves out: placeholders a template left in.
fn present(v: Option<&str>) -> Option<&str> {
    v.filter(|v| !v.starts_with("$SPOREMOD"))
}

fn flag(v: Option<&str>) -> Result<bool, String> {
    match present(v).map(str::trim) {
        None => Ok(false),
        Some(v) if v.eq_ignore_ascii_case("true") => Ok(true),
        Some(v) if v.eq_ignore_ascii_case("false") => Ok(false),
        Some(v) => Err(format!("{v} is neither true nor false")),
    }
}

/// The text of a file as XML: UTF-8 with or without its mark, or UTF-16 with one.
pub fn decode(bytes: &[u8]) -> Result<String, String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(rest.to_vec()).map_err(|e| e.to_string());
    }
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> =
            rest.as_chunks::<2>().0.iter().map(|&c| if le { u16::from_le_bytes(c) } else { u16::from_be_bytes(c) }).collect();
        String::from_utf16(&units).map_err(|e| e.to_string())
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    // Not UTF-8: written in Windows' Western code page, which Notepad used to save in.
    Ok(String::from_utf8(bytes.to_vec()).unwrap_or_else(|e| e.into_bytes().iter().map(|&b| cp1252(b)).collect()))
}

/// A byte of Windows-1252 as the character it stands for.
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•',
        '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9f => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

pub fn parse(bytes: &[u8]) -> Result<ModInfo, String> {
    let text = decode(bytes)?;
    // An encoding declaration of utf-16 left in a file saved as UTF-8 would stop the parser.
    let text = match text.find("?>") {
        Some(end) if text.trim_start().starts_with("<?xml") => text[end + 2..].to_string(),
        _ => text,
    };
    let doc = roxmltree::Document::parse(&text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("mod") {
        return Err("ModInfo.xml has no <mod> element".into());
    }
    let attr = |n: &str| present(root.attribute(n));
    let format_text = attr("installerSystemVersion").ok_or("ModInfo.xml has no installerSystemVersion")?;
    let format = Version::parse(format_text).ok_or_else(|| format!("installerSystemVersion {format_text} is not a version"))?;
    let unique = attr("unique").map(str::trim).filter(|u| !u.is_empty()).ok_or("ModInfo.xml has no unique name")?.to_string();
    let display_name = attr("displayName").map(str::trim).filter(|d| !d.is_empty()).unwrap_or(&unique).to_string();
    let version_of = |name: &str| -> Result<Option<Version>, String> {
        match attr(name) {
            None => Ok(None),
            Some(v) => Version::parse(v).map(Some).ok_or_else(|| format!("{name} {v} is not a version")),
        }
    };
    let version = match version_of("version")? {
        Some(v) => Some(v),
        None => version_of("modVersion")?,
    };
    let dlls_build = version_of("dllsBuild")?;
    let mut depends = Vec::new();
    if let Some(list) = attr("depends") {
        let mins: Vec<&str> = attr("dependsVersions").map(|v| v.split('?').collect()).unwrap_or_default();
        for (i, unique) in list.split('?').map(str::trim).enumerate().filter(|(_, u)| !u.is_empty()) {
            let min = match mins.get(i).map(|m| m.trim()).filter(|m| !m.is_empty()) {
                None => None,
                Some(m) => Some(Version::parse(m).ok_or_else(|| format!("dependsVersions {m} is not a version"))?),
            };
            // 0.0.0.0 is how the Launcher Kit writes "any version".
            depends.push(Dependency { unique: unique.to_string(), min: min.filter(|v| *v != Version::default()) });
        }
    }
    let mut items = Vec::new();
    for node in root.children().filter(|n| n.is_element()) {
        let name = node.tag_name().name().to_ascii_lowercase();
        let files = || Files::parse(node.text().unwrap_or(""), node.attribute("game"));
        match name.as_str() {
            "prerequisite" => items.push(Item::Prerequisite(files())),
            "component" => items.push(Item::Component(component(node)?)),
            "componentgroup" => {
                let components = node
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("component"))
                    .map(component)
                    .collect::<Result<Vec<_>, _>>()?;
                let unique = node.attribute("unique").unwrap_or("").to_string();
                let display_name = node.attribute("displayName").unwrap_or(&unique).to_string();
                items.push(Item::Group { unique, display_name, components });
            }
            "compatfile" => items.push(Item::Compat {
                files: files(),
                targets: Files::parse(node.attribute("compatTargetFileName").unwrap_or(""), node.attribute("compatTargetGame")),
                remove_targets: flag(node.attribute("removeTargets")).unwrap_or(false),
            }),
            "remove" => items.push(Item::Remove(files())),
            _ => {}
        }
    }
    Ok(ModInfo {
        unique,
        display_name,
        description: attr("description").map(str::trim).filter(|d| !d.is_empty()).map(String::from),
        format,
        version,
        dlls_build,
        depends,
        has_custom_installer: flag(attr("hasCustomInstaller"))?,
        experimental: flag(attr("isExperimental"))?,
        requires_galaxy_reset: flag(attr("requiresGalaxyReset"))?,
        save_data_dependency: flag(attr("causesSaveDataDependency"))?,
        compat_only: format == Version::new(1, 0, 0, 0) && attr("mode") == Some("compatOnly"),
        items,
    })
}

fn component(node: roxmltree::Node) -> Result<Component, String> {
    let unique = node.attribute("unique").unwrap_or("").trim().to_string();
    if unique.is_empty() {
        return Err("a component has no unique name".into());
    }
    Ok(Component {
        display_name: node.attribute("displayName").unwrap_or(&unique).to_string(),
        description: node.attribute("description").map(str::trim).filter(|d| !d.is_empty()).map(String::from),
        default_checked: flag(node.attribute("defaultChecked")).unwrap_or(false),
        files: Files::parse(node.text().unwrap_or(""), node.attribute("game")),
        unique,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sample in the Launcher Kit's documentation.
    pub const SAMPLE: &str = r#"<mod displayName="My Mod"
		unique="MyMod"
		description="This is an example Custom XML Installer for a hypothetical mod."
		installerSystemVersion="1.0.1.3"
		hasCustomInstaller="true"
		dllsBuild="2.5.20"
		version="1.2.0"
		depends="ModAPIBase?Other"
		dependsVersions="1.1?">
	<componentGroup unique="groupOne" displayName="This is a Group">
		<component unique="optionOne" displayName="Option one"
					description="This is an option in a component group"
					game="GalacticAdventures?GalacticAdventures"
					defaultChecked="true">OptionOneA.package?OptionOneB.package</component>
		<component unique="optionTwo" displayName="Option two" description=" "
					imagePlacement="InsteadOf" game="GalacticAdventures">OptionTwo.package</component>
	</componentGroup>
	<component unique="togglableComponent" displayName="Togglable Component"
				game="GalacticAdventures" defaultChecked="true">ToggleComponent.package</component>
	<prerequisite game="GalacticAdventures">Component.package</prerequisite>
	<prerequisite>MyMod.dll</prerequisite>
	<compatFile compatTargetFileName="Other.package" compatTargetGame="GalacticAdventures" game="GalacticAdventures">Patch.package</compatFile>
	<remove game="Spore">*NothingActuallyEndsWithThis.package</remove>
</mod>"#;

    #[test]
    fn reads_the_documented_sample() {
        let m = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(m.unique, "MyMod");
        assert_eq!(m.format, Version::new(1, 0, 1, 3));
        assert_eq!(m.dlls_build, Version::parse("2.5.20"));
        assert_eq!(m.version, Version::parse("1.2.0"));
        assert!(m.has_choices());
        assert_eq!(
            m.depends,
            vec![Dependency { unique: "ModAPIBase".into(), min: Version::parse("1.1") }, Dependency { unique: "Other".into(), min: None }]
        );
        assert_eq!(m.items.len(), 6);
        let Item::Group { components, .. } = &m.items[0] else { panic!() };
        assert_eq!(
            components[0].files.0,
            vec![
                ("OptionOneA.package".to_string(), Target::GalacticAdventures),
                ("OptionOneB.package".to_string(), Target::GalacticAdventures)
            ]
        );
        assert!(components[0].default_checked && !components[1].default_checked);
        assert_eq!(components[1].description, None);
        let Item::Prerequisite(dll) = &m.items[3] else { panic!() };
        assert_eq!(dll.0, vec![("MyMod.dll".to_string(), Target::LauncherKit)]);
        let Item::Compat { targets, .. } = &m.items[4] else { panic!() };
        assert_eq!(targets.0[0].0, "Other.package");
        assert_eq!(m.components().count(), 3);
    }

    #[test]
    fn placeholders_and_marks_are_left_out() {
        let xml = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-16\"?><mod unique=\"A\" displayName=\"$SPOREMOD_NAME\" \
                   installerSystemVersion=\"1.0.1.2\" dllsBuild=\"2.5.200\" isExperimental=\"True\"/>";
        let m = parse(xml.as_bytes()).unwrap();
        assert_eq!(m.display_name, "A");
        assert!(m.experimental);
        assert!(!m.has_choices());
    }

    #[test]
    fn utf16_files_are_read() {
        let xml = r#"<mod unique="B" installerSystemVersion="1.0.0.0" mode="compatOnly"/>"#;
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        let m = parse(&bytes).unwrap();
        assert!(m.legacy_dlls() && m.compat_only && !m.has_choices());
    }

    #[test]
    fn old_code_page_files_are_read() {
        let m = parse(b"<mod unique=\"C\" displayName=\"Cam\x92s Pack\" installerSystemVersion=\"1.0.1.3\"/>").unwrap();
        assert_eq!(m.display_name, "Cam\u{2019}s Pack");
    }

    #[test]
    fn broken_files_say_why() {
        assert!(parse(b"<mod unique=\"A\"/>").unwrap_err().contains("installerSystemVersion"));
        assert!(parse(b"<mod installerSystemVersion=\"1.0.1.3\"/>").unwrap_err().contains("unique"));
        assert!(parse(b"<mod unique=\"A\" installerSystemVersion=\"1.0.1.3\" dllsBuild=\"x\"/>").is_err());
        assert!(parse(b"<notamod/>").is_err());
    }
}
