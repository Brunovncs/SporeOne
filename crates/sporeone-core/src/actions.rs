//! What the window's buttons do, from the download to the Launcher Kit's list: each runs away from
//! the window's thread and answers with what happened.

use crate::catalog::{Catalog, Entry, Kind};
use crate::compat::Env;
use crate::install::{self, Choices, Plan, Refusal, Roots};
use crate::installed::InstalledMods;
use crate::state::{self, Origin, State};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A plan for `sporeone --apply`, with the folders it was made for.
#[derive(Serialize, Deserialize)]
struct Elevated {
    plan: Plan,
    roots: Roots,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Refused(Refusal),
    /// The person said no to Windows' administrator prompt.
    Declined,
    Other(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Refused(r) => write!(f, "{r}"),
            Failure::Declined => write!(f, "administrator rights were not given"),
            Failure::Other(e) => write!(f, "{e}"),
        }
    }
}

impl From<String> for Failure {
    fn from(e: String) -> Failure {
        Failure::Other(e)
    }
}

impl From<Refusal> for Failure {
    fn from(r: Refusal) -> Failure {
        Failure::Refused(r)
    }
}

fn roots(env: &Env) -> Result<Roots, Failure> {
    env.roots().ok_or_else(|| Failure::Other("the Launcher Kit is not installed".into()))
}

/// Carries a plan out here, or through `sporeone --apply` with administrator rights when the
/// game's folders need them.
pub fn carry_out(plan: &Plan, roots: &Roots) -> Result<(), Failure> {
    install::check(plan, roots)?;
    if !install::needs_admin(plan) {
        return install::apply(plan, roots).map_err(Failure::Other);
    }
    elevated(plan, roots)
}

#[cfg(windows)]
fn elevated(plan: &Plan, roots: &Roots) -> Result<(), Failure> {
    let dir = state::data_dir().join("plans");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!(
        "plan-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
    ));
    let result = file.with_extension("result");
    let _ = std::fs::remove_file(&result);
    let body = serde_json::to_vec(&Elevated { plan: plan.clone(), roots: roots.clone() }).map_err(|e| e.to_string())?;
    std::fs::write(&file, body).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let ran = crate::win::shell_run(&exe, &format!("--apply \"{}\"", file.display()), None, true, true);
    let _ = std::fs::remove_file(&file);
    match ran {
        Ok(None) => Err(Failure::Declined),
        Err(e) => Err(Failure::Other(e)),
        Ok(Some(_)) => {
            let said = std::fs::read_to_string(&result).unwrap_or_else(|_| "the administrator step said nothing".into());
            let _ = std::fs::remove_file(&result);
            if said == "ok" { Ok(()) } else { Err(Failure::Other(said)) }
        }
    }
}

#[cfg(not(windows))]
fn elevated(_: &Plan, _: &Roots) -> Result<(), Failure> {
    Err(Failure::Other("the game's folders cannot be written".into()))
}

/// `sporeone --apply <file>`, run with administrator rights: checks the folders the plan names are
/// really the game's and the Launcher Kit's, carries it out, and writes "ok" or the error next to
/// it.
pub fn apply_file(file: &Path) -> i32 {
    let outcome = (|| -> Result<(), String> {
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        let job: Elevated = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let lk = &job.roots.lk;
        // A tweak of the game's settings has the game's data folder in the Launcher Kit's place.
        let data = job.roots.ga_data.iter().chain(job.roots.spore_data.iter()).any(|d| d == lk);
        if !data && !lk.join(crate::launcher_kit::LAUNCHER_EXE).is_file() {
            return Err(format!("{} is not the Launcher Kit's folder", lk.display()));
        }
        for d in job.roots.ga_data.iter().chain(job.roots.spore_data.iter()) {
            let game =
                std::fs::read_dir(d).into_iter().flatten().flatten().any(|e| crate::game::is_game_file(&e.file_name().to_string_lossy()));
            if !game {
                return Err(format!("{} is not one of Spore's data folders", d.display()));
            }
        }
        install::apply(&job.plan, &job.roots)
    })();
    let text = match &outcome {
        Ok(()) => "ok".to_string(),
        Err(e) => e.clone(),
    };
    let _ = std::fs::write(file.with_extension("result"), text);
    i32::from(outcome.is_err())
}

fn safe_dir(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect()
}

/// Downloads an entry's release file.
pub fn fetch(entry: &Entry) -> Result<PathBuf, Failure> {
    let r = entry.release.as_ref().ok_or_else(|| Failure::Other(format!("{} has no download", entry.name)))?;
    let dir = state::downloads().join(safe_dir(&entry.id)).join(safe_dir(&r.tag));
    Ok(crate::net::download(&r.url, &dir, &r.file, r.sha256.as_deref())?)
}

/// Installs a file already on disk: one dropped on the window, or a download.
pub fn install_file(file: &Path, env: &Env, choices: Option<&Choices>) -> Result<String, Failure> {
    let roots = roots(env)?;
    let prepared = install::prepare(file, &state::staging())?;
    let installed = InstalledMods::read(&roots.lk)?;
    let plan = install::plan_install(&prepared, &roots, &installed, env.lk.as_ref().and_then(|l| l.dlls_build), choices)?;
    carry_out(&plan, &roots)?;
    let _ = std::fs::remove_dir_all(&prepared.staging);
    Ok(prepared.name)
}

/// Installs a catalog mod and what it needs that the catalog has, the needed ones first. Returns
/// the names installed.
pub fn install_entry(entry: &Entry, catalog: &Catalog, env: &Env, choices: Option<&Choices>) -> Result<Vec<String>, Failure> {
    if entry.kind != Kind::Release {
        return Err(Failure::Other(format!("{} is not installed by SporeOne", entry.name)));
    }
    let (bring, _) = crate::compat::resolve(entry, catalog, &env.installed);
    let mut env = env.clone();
    let mut done = Vec::new();
    let mut st = State::load();
    for e in bring.iter().filter_map(|id| catalog.get(id)).chain(std::iter::once(entry)) {
        let file = fetch(e)?;
        let mine = if e.id == entry.id { choices } else { None };
        let name = install_file(&file, &env, mine)?;
        if let Some(r) = &e.release {
            st.origins.insert(name.clone(), Origin { id: e.id.clone(), tag: r.tag.clone() });
        }
        env.installed = InstalledMods::read(&roots(&env)?.lk)?;
        done.push(name);
    }
    let _ = st.save();
    Ok(done)
}

pub fn uninstall(name: &str, env: &Env) -> Result<(), Failure> {
    let roots = roots(env)?;
    let installed = InstalledMods::read(&roots.lk)?;
    let m = installed.find(name).ok_or_else(|| Failure::Other(format!("{name} is not installed")))?;
    let plan = install::plan_uninstall(m, &roots, &installed);
    carry_out(&plan, &roots)?;
    let mut st = State::load();
    st.origins.remove(name);
    let _ = st.save();
    Ok(())
}

/// Turns a tweak of the game's settings on or off.
pub fn set_tweak(t: &crate::tweaks::Tweak, game: &crate::game::Game, on: bool) -> Result<(), Failure> {
    let (plan, roots) = crate::tweaks::plan(t, game, on, &state::data_dir().join("originals")).map_err(Failure::Other)?;
    carry_out(&plan, &roots)
}

/// Downloads a mod's own installer and starts it.
#[cfg(windows)]
pub fn run_setup(entry: &Entry) -> Result<(), Failure> {
    let file = fetch(entry)?;
    crate::win::shell_run(&file, "", file.parent(), false, false)?;
    Ok(())
}

/// Downloads the Launcher Kit's setup and starts it; it asks where to install by itself.
#[cfg(windows)]
pub fn install_launcher_kit() -> Result<(), Failure> {
    let dir = state::downloads().join("launcher-kit");
    let file = crate::net::download(crate::launcher_kit::SETUP_URL, &dir, "ModAPI.InterimSetup.exe", None)?;
    crate::win::shell_run(&file, "", file.parent(), false, false)?;
    Ok(())
}
