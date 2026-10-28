//! The Spore ModAPI Launcher Kit: where it is, which version, which ModAPI DLLs it carries, and
//! starting the game through it, which is what loads `.dll` mods. SporeOne installs mods into it
//! rather than beside it, so both always agree on what is installed.

use crate::version::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LAUNCHER_EXE: &str = "Spore ModAPI Launcher.exe";
pub const UNINSTALLER_EXE: &str = "Spore ModAPI Easy Uninstaller.exe";
/// The Launcher Kit's setup, as its site links it.
pub const SETUP_URL: &str = "https://github.com/Spore-Community/ModAPI-Launcher-Kit/releases/latest/download/ModAPI.InterimSetup.exe";
pub const SITE: &str = "https://launcherkit.sporecommunity.com/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherKit {
    pub dir: PathBuf,
    pub version: Option<Version>,
    /// The ModAPI DLLs' build, the oldest of those in `coreLibs` as the Launcher Kit counts it:
    /// a mod whose `dllsBuild` is newer needs an update of them first.
    pub dlls_build: Option<Version>,
}

/// `%APPDATA%\Spore ModAPI Launcher`, where the Launcher Kit notes where it is installed.
pub fn appdata() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Spore ModAPI Launcher"))
}

/// Where its setup puts it unless told otherwise. `path.info` is only written once the launcher
/// has run, so a Launcher Kit just installed is found here.
fn default_dirs() -> Vec<PathBuf> {
    ["ProgramData", "ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join("SPORE ModAPI Launcher Kit"))
        .collect()
}

pub fn find() -> Option<LauncherKit> {
    if let Some(dir) = std::env::var_os("SPOREONE_LK_DIR") {
        return Some(LauncherKit::at(Path::new(&dir)));
    }
    let noted = appdata().and_then(|a| std::fs::read_to_string(a.join("path.info")).ok()).map(|s| PathBuf::from(s.trim()));
    noted.into_iter().chain(default_dirs()).find(|dir| dir.join(LAUNCHER_EXE).is_file()).map(|dir| LauncherKit::at(&dir))
}

impl LauncherKit {
    pub fn at(dir: &Path) -> LauncherKit {
        LauncherKit { dir: dir.to_path_buf(), version: crate::win::file_version(&dir.join(LAUNCHER_EXE)), dlls_build: dlls_build(dir) }
    }

    pub fn mlibs(&self) -> PathBuf {
        self.dir.join("mLibs")
    }

    pub fn mod_configs(&self) -> PathBuf {
        self.dir.join("ModConfigs")
    }

    pub fn mod_settings(&self) -> PathBuf {
        self.dir.join("ModSettings")
    }

    /// Starts the game with mods. The launcher asks for administrator rights on its own when it
    /// needs them.
    #[cfg(windows)]
    pub fn launch(&self) -> Result<(), String> {
        let exe = self.dir.join(LAUNCHER_EXE);
        crate::win::shell_run(&exe, "", Some(&self.dir), false, false).map(|_| ())
    }

    #[cfg(windows)]
    pub fn open_uninstaller(&self) -> Result<(), String> {
        let exe = self.dir.join(UNINSTALLER_EXE);
        crate::win::shell_run(&exe, "", Some(&self.dir), false, false).map(|_| ())
    }
}

fn dlls_build(dir: &Path) -> Option<Version> {
    let core = std::fs::read_dir(dir.join("coreLibs")).ok()?;
    core.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")))
        .filter_map(|p| crate::win::file_version(&p))
        .min()
        .map(Version::build3)
}
