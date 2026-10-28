//! The window: a bar on top, and under it one of four screens: the catalog as a list or as tiles,
//! one mod's page, what is installed, or the settings with what the game and the Launcher Kit
//! need. Files dropped anywhere on it are installed.

use crate::text_field::{TextField, TextFieldEvent};
use crate::theme::{FONT, Theme, icon as glyph, layout, radius};
use crate::widgets::{Kind, body, button, caption, icon, icon_button, icon_label_button, title};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, Context, Entity, ExternalPaths, FocusHandle, Focusable, Image, ImageFormat, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, PathPromptOptions, Render, StatefulInteractiveElement, Styled, Subscription, Window,
    WindowAppearance, div, img, px,
};
use sporeone_core::actions::{self, Failure};
use sporeone_core::catalog::{Catalog, Category, Entry};
use sporeone_core::compat::{self, Env, Issue};
use sporeone_core::github;
use sporeone_core::i18n::{self, Lang, Text, fill};
use sporeone_core::install::Choices;
use sporeone_core::installed::InstalledMod;
use sporeone_core::state::{Origin, State};
use sporeone_core::update::{self, Release};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

/// The theme for the window's appearance. `SPOREONE_THEME=light` or `dark` overrides Windows,
/// for screenshots.
fn theme(window: &Window) -> Theme {
    match std::env::var("SPOREONE_THEME").as_deref() {
        Ok("light") => Theme::new(WindowAppearance::Light),
        Ok("dark") => Theme::new(WindowAppearance::Dark),
        _ => Theme::new(window.appearance()),
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum Screen {
    Catalog,
    /// A mod's page, by catalog id.
    Mod(String),
    Installed,
    Tweaks,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Popular,
    Recent,
    Name,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Grid,
}

/// Where a newer SporeOne stands.
#[derive(Clone)]
pub enum AppUpdate {
    Checking,
    Latest,
    Available(Release),
    Working(Release),
    Failed(Release, String),
}

/// The dialog that adds a mod from a link or a file.
pub struct Adding {
    pub link: Entity<TextField>,
    pub working: bool,
    pub error: Option<String>,
}

/// What is being done to a mod right now.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Work {
    Install,
    Uninstall,
    Setup,
}

/// A GitHub mod's README, for its page.
pub enum ReadmeState {
    Loading,
    Loaded(Arc<Vec<crate::markdown::Block>>),
    /// Not reachable, not there, or nothing in it to show: the page keeps the short description.
    Failed,
}

pub struct Notice {
    pub ok: bool,
    pub text: String,
}

/// Where a catalog mod stands on this computer.
#[derive(Clone, Copy)]
pub enum Have<'a> {
    Not,
    Installed(&'a InstalledMod),
    Outdated(&'a InstalledMod),
    /// Put in place by its own installer, which the game's folder shows.
    External,
}

pub struct MainView {
    pub theme: Theme,
    pub focus: FocusHandle,
    pub screen: Screen,
    pub lang: Lang,
    pub text: &'static Text,
    /// The catalog with the mods added from a link.
    pub catalog: Arc<Catalog>,
    /// The catalog as downloaded.
    base: Arc<Catalog>,
    /// The mods added from a link, and which of them the catalog does not have.
    custom: Vec<Entry>,
    pub custom_ids: HashSet<String>,
    pub view: View,
    /// The page's scrolling, and where the catalog's list starts in it, so only the rows on
    /// screen are drawn.
    pub scroll: gpui::ScrollHandle,
    pub list_y: std::rc::Rc<std::cell::Cell<f32>>,
    pub adding: Option<Adding>,
    pub app_update: Option<AppUpdate>,
    pub env: Arc<Env>,
    pub origins: BTreeMap<String, Origin>,
    pub search: Entity<TextField>,
    pub query: String,
    pub category: Option<Category>,
    pub sort: Sort,
    /// By catalog id, or by the Launcher Kit's name for mods installed outside the catalog.
    pub busy: HashMap<String, Work>,
    pub notice: Option<Notice>,
    /// Options picked on a mod's page before installing it, by catalog id.
    pub choices: HashMap<String, Choices>,
    /// The installed mod whose uninstall waits for a second click.
    pub confirm: Option<String>,
    pub refreshing: bool,
    pub catalog_error: Option<String>,
    pub demo: bool,
    /// Fetched once a session, by catalog id.
    pub readmes: HashMap<String, ReadmeState>,
    brand: (f32, Arc<Image>),
    /// The bar on top was pressed where the window draws its own frame: moving the mouse now
    /// moves the window.
    pub moving: bool,
    _subscriptions: Vec<Subscription>,
}

pub fn read_env(demo: bool) -> Env {
    if demo { crate::demo::env() } else { Env::read() }
}

impl MainView {
    pub fn new(demo: bool, window: &mut Window, cx: &mut Context<Self>) -> MainView {
        let saved = State::load();
        let lang = std::env::var("SPOREONE_LANG")
            .ok()
            .and_then(|l| Lang::from_code(&l))
            .or_else(|| saved.lang.as_deref().and_then(Lang::from_code))
            .unwrap_or_else(Lang::system);
        let text = i18n::text(lang);
        let search = cx.new(|cx| TextField::new(cx, false, 80).placeholder(text.search).icon(glyph::SEARCH));
        let searched = cx.subscribe(&search, |this: &mut MainView, field, _: &TextFieldEvent, cx| {
            this.query = field.read(cx).text();
            if this.screen != Screen::Catalog {
                this.screen = Screen::Catalog;
            }
            cx.notify();
        });
        let theme_change = cx.observe_window_appearance(window, |this, window, cx| {
            this.theme = theme(window);
            cx.notify();
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            // GPUI tells of light and dark changing but not of the accent colour, which is picked
            // in Windows' settings: coming back from them makes the window active.
            let now = theme(window);
            if now.accent != this.theme.accent {
                crate::theme::accent_changed();
                this.theme = now;
                cx.notify();
            }
            // Back from Steam or an installer: the game or the Launcher Kit may have changed.
            if window.is_window_active() && this.busy.is_empty() {
                this.reread(cx);
            }
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let base = Arc::new(if demo { crate::demo::catalog() } else { sporeone_core::state::cached_catalog() });
        let (catalog, custom_ids) = merged(&base, &saved.custom);
        let mut view = MainView {
            theme: theme(window),
            focus,
            screen: Screen::Catalog,
            lang,
            text,
            catalog: Arc::new(catalog),
            base,
            custom: saved.custom,
            custom_ids,
            view: if saved.grid { View::Grid } else { View::List },
            scroll: gpui::ScrollHandle::new(),
            list_y: std::rc::Rc::new(std::cell::Cell::new(0.)),
            adding: None,
            app_update: None,
            env: Arc::new(read_env(demo)),
            origins: if demo { crate::demo::origins() } else { saved.origins },
            search,
            query: String::new(),
            category: None,
            sort: Sort::Popular,
            busy: HashMap::new(),
            notice: None,
            choices: HashMap::new(),
            confirm: None,
            refreshing: false,
            catalog_error: None,
            demo,
            readmes: HashMap::new(),
            brand: brand(window.scale_factor()),
            moving: false,
            _subscriptions: vec![searched, theme_change, activation],
        };
        // `--demo` with `SPOREONE_PAGE`: a screen other than the catalog, for screenshots.
        if demo && let Ok(page) = std::env::var("SPOREONE_PAGE") {
            match page.as_str() {
                "installed" => view.screen = Screen::Installed,
                "tweaks" => view.screen = Screen::Tweaks,
                "settings" => view.screen = Screen::Settings,
                "grid" => view.view = View::Grid,
                "add" => view.open_adding(window, cx),
                id => view.screen = Screen::Mod(id.to_string()),
            }
        }
        // `--demo` with `SPOREONE_UPDATE=x.y.z`: the bar for a newer SporeOne.
        if demo && let Ok(version) = std::env::var("SPOREONE_UPDATE") {
            let page = update::RELEASES_PAGE.to_string();
            view.app_update = Some(AppUpdate::Available(Release { version, page, setup: None }));
        }
        if !demo {
            view.refresh_catalog(cx);
            view.check_app_update(cx);
        }
        view
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.catalog.get(id)
    }

    pub fn have<'a>(&'a self, e: &Entry) -> Have<'a> {
        if e.kind == sporeone_core::catalog::Kind::Setup {
            let there = e.marker.as_ref().zip(self.env.game()).is_some_and(|(m, g)| g.root.join(m).exists());
            return if there { Have::External } else { Have::Not };
        }
        let installed = &self.env.installed;
        let by_unique = e.unique().and_then(|u| installed.mods.iter().find(|m| m.unique == u));
        let by_origin = || self.origins.iter().find(|(_, o)| o.id == e.id).and_then(|(name, _)| installed.find(name));
        let by_file = || e.release.as_ref().and_then(|r| r.file.rsplit_once('.').map(|(stem, _)| stem)).and_then(|s| installed.find(s));
        match by_unique.or_else(by_origin).or_else(by_file) {
            None => Have::Not,
            Some(m) if compat::outdated(e, m, self.origins.get(&m.name).map(|o| o.tag.as_str())) => Have::Outdated(m),
            Some(m) => Have::Installed(m),
        }
    }

    /// Mods with a newer version in the catalog, by catalog id.
    pub fn updates(&self) -> Vec<String> {
        self.catalog.mods.iter().filter(|e| e.installable() && matches!(self.have(e), Have::Outdated(_))).map(|e| e.id.clone()).collect()
    }

    pub fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.screen = screen;
        self.confirm = None;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    pub fn set_lang(&mut self, lang: Lang, cx: &mut Context<Self>) {
        self.lang = lang;
        self.text = i18n::text(lang);
        let placeholder = self.text.search;
        self.search.update(cx, |f, cx| f.set_placeholder(placeholder, cx));
        let mut st = State::load();
        st.lang = Some(lang.code().to_string());
        let _ = st.save();
        cx.notify();
    }

    /// Looks at the game and the Launcher Kit again, away from the window's thread.
    pub fn reread(&mut self, cx: &mut Context<Self>) {
        let demo = self.demo;
        cx.spawn(async move |this, cx| {
            let env = cx.background_executor().spawn(async move { read_env(demo) }).await;
            let _ = this.update(cx, |this, cx| {
                this.env = Arc::new(env);
                if !this.demo {
                    this.origins = State::load().origins;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        if self.refreshing || self.demo {
            return;
        }
        self.refreshing = true;
        self.catalog_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx.background_executor().spawn(async { sporeone_core::state::refresh_catalog() }).await;
            let _ = this.update(cx, |this, cx| {
                this.refreshing = false;
                match got {
                    Ok(c) if c.generated >= this.base.generated => {
                        this.base = Arc::new(c);
                        this.remerge();
                    }
                    Ok(_) => {}
                    Err(e) => this.catalog_error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Does `job` away from the window's thread, then reads the game and the Launcher Kit again
    /// and says how it went.
    fn run(
        &mut self,
        key: String,
        work: Work,
        done: String,
        job: impl FnOnce() -> Result<(), Failure> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy.contains_key(&key) {
            return;
        }
        self.busy.insert(key.clone(), work);
        self.notice = None;
        self.confirm = None;
        cx.notify();
        let demo = self.demo;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { job() }).await;
            let env = cx.background_executor().spawn(async move { read_env(demo) }).await;
            let _ = this.update(cx, |this, cx| {
                this.busy.remove(&key);
                this.env = Arc::new(env);
                if !this.demo {
                    this.origins = State::load().origins;
                }
                let t = this.text;
                this.notice = Some(match result {
                    Ok(()) => Notice { ok: true, text: done },
                    Err(Failure::Declined) => Notice { ok: false, text: t.declined.to_string() },
                    Err(e) => Notice { ok: false, text: fill(t.failed, e) },
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn remerge(&mut self) {
        let (catalog, ids) = merged(&self.base, &self.custom);
        self.catalog = Arc::new(catalog);
        self.custom_ids = ids;
    }

    pub fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.view = view;
        let mut st = State::load();
        st.grid = view == View::Grid;
        let _ = st.save();
        cx.notify();
    }

    pub fn open_adding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = self.text.add_link;
        let link = cx.new(|cx| TextField::new(cx, false, 200).placeholder(placeholder).icon(glyph::LINK));
        let submitted = cx.subscribe(&link, |this: &mut MainView, _, ev: &TextFieldEvent, cx| {
            if matches!(ev, TextFieldEvent::Submit) {
                this.add_from_link(cx);
            }
        });
        submitted.detach();
        link.read(cx).focus_handle(cx).focus(window, cx);
        self.adding = Some(Adding { link, working: false, error: None });
        cx.notify();
    }

    /// Reads the repository the link points to and lists its mod with the catalog's.
    pub fn add_from_link(&mut self, cx: &mut Context<Self>) {
        let text = self.text;
        let Some(adding) = &mut self.adding else { return };
        if adding.working {
            return;
        }
        let input = adding.link.read(cx).text();
        let Some(repo) = github::parse_repo(&input) else {
            adding.error = Some(text.add_bad_link.to_string());
            cx.notify();
            return;
        };
        if let Some(e) = self.catalog.mods.iter().find(|e| e.id.eq_ignore_ascii_case(&repo)) {
            let id = e.id.clone();
            self.adding = None;
            self.go(Screen::Mod(id), cx);
            return;
        }
        adding.working = true;
        adding.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx.background_executor().spawn(async move { github::entry(&repo, None, false) }).await;
            let _ = this.update(cx, |this, cx| match got {
                Ok(e) => {
                    let id = e.id.clone();
                    let mut st = State::load();
                    st.custom.retain(|c| c.id != id);
                    st.custom.push(e.clone());
                    let _ = st.save();
                    this.custom.retain(|c| c.id != id);
                    this.custom.push(e);
                    this.remerge();
                    this.adding = None;
                    this.go(Screen::Mod(id), cx);
                }
                Err(err) => {
                    if let Some(a) = &mut this.adding {
                        a.working = false;
                        a.error = Some(err);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Asks for .sporemod and .package files and installs them.
    pub fn pick_files(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |this, cx| {
                    this.adding = None;
                    this.install_files(paths, cx);
                });
            }
        })
        .detach();
    }

    /// Asks for the folder of a game or Launcher Kit SporeOne did not find, and keeps it.
    pub fn locate(&mut self, lk: bool, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(dir) = paths.into_iter().next() else { return };
            let _ = this.update(cx, |this, cx| {
                let mut st = State::load();
                let ok = if lk {
                    let ok = sporeone_core::launcher_kit::is_launcher_kit(&dir);
                    if ok {
                        st.lk_dir = Some(dir);
                    }
                    ok
                } else {
                    let root = sporeone_core::game::game_root(&dir);
                    let ok = root.is_some();
                    if ok {
                        st.game_dir = root;
                    }
                    ok
                };
                if ok {
                    let _ = st.save();
                    this.notice = None;
                    this.reread(cx);
                } else {
                    let t = this.text;
                    this.notice = Some(Notice { ok: false, text: (if lk { t.lk_wrong } else { t.game_wrong }).to_string() });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Turns a tweak of the game's settings on or off.
    pub fn set_tweak(&mut self, id: &'static str, name: &'static str, on: bool, cx: &mut Context<Self>) {
        let (Some(t), Some(g)) = (sporeone_core::tweaks::by_id(id), self.env.game().cloned()) else { return };
        let done = fill(if on { self.text.tweak_on } else { self.text.tweak_off }, name);
        self.run(format!("tweak:{id}"), Work::Setup, done, move || actions::set_tweak(t, &g, on), cx);
    }

    /// Takes a mod added from a link off the list. What is installed stays.
    pub fn remove_custom(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut st = State::load();
        st.custom.retain(|c| c.id != id);
        let _ = st.save();
        self.custom.retain(|c| c.id != id);
        self.remerge();
        self.go(Screen::Catalog, cx);
    }

    /// Asks GitHub whether a newer SporeOne is out.
    pub fn check_app_update(&mut self, cx: &mut Context<Self>) {
        if matches!(self.app_update, Some(AppUpdate::Checking | AppUpdate::Working(_))) {
            return;
        }
        self.app_update = Some(AppUpdate::Checking);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx.background_executor().spawn(async { update::check() }).await;
            let _ = this.update(cx, |this, cx| {
                this.app_update = match got {
                    Ok(Some(r)) => Some(AppUpdate::Available(r)),
                    Ok(None) => Some(AppUpdate::Latest),
                    Err(_) => None,
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Downloads the new setup and starts it, which closes SporeOne and opens it again updated.
    /// Without a setup to run (SporeOne came from the zip), opens the release's page.
    pub fn start_app_update(&mut self, cx: &mut Context<Self>) {
        let (Some(AppUpdate::Available(r)) | Some(AppUpdate::Failed(r, _))) = self.app_update.clone() else { return };
        if !update::installed_by_setup() || r.setup.is_none() {
            cx.open_url(&r.page);
            return;
        }
        self.app_update = Some(AppUpdate::Working(r.clone()));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let job = r.clone();
            let got = cx.background_executor().spawn(async move { update::download_setup(&job).and_then(|f| update::run_setup(&f)) }).await;
            let _ = this.update(cx, |this, cx| match got {
                Ok(()) => cx.quit(),
                Err(e) => {
                    this.app_update = Some(AppUpdate::Failed(r, e));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn install(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.entry(id).cloned() else { return };
        let (catalog, env) = (self.catalog.clone(), self.env.clone());
        let choices = self.choices.get(id).cloned();
        let done = fill(self.text.done_installed, &entry.name);
        self.run(
            id.to_string(),
            Work::Install,
            done,
            move || actions::install_entry(&entry, &catalog, &env, choices.as_ref()).map(|_| ()),
            cx,
        );
    }

    pub fn update_all(&mut self, cx: &mut Context<Self>) {
        for id in self.updates() {
            self.install(&id, cx);
        }
    }

    /// Uninstalls the mod the Launcher Kit lists as `name`; `key` is what shows it busy.
    pub fn uninstall(&mut self, name: &str, key: &str, cx: &mut Context<Self>) {
        let env = self.env.clone();
        let shown = env.installed.find(name).map_or(name.to_string(), |m| m.shown_name().to_string());
        let name = name.to_string();
        let done = fill(self.text.done_uninstalled, shown);
        self.run(key.to_string(), Work::Uninstall, done, move || actions::uninstall(&name, &env), cx);
    }

    pub fn run_setup(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.entry(id).cloned() else { return };
        let done = fill(self.text.done_setup, &entry.name);
        #[cfg(windows)]
        self.run(id.to_string(), Work::Setup, done, move || actions::run_setup(&entry), cx);
        #[cfg(not(windows))]
        let _ = (entry, done);
    }

    pub fn install_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let env = self.env.clone();
            let done = fill(self.text.done_installed, &name);
            self.run(
                format!("file:{}", path.display()),
                Work::Install,
                done,
                move || actions::install_file(&path, &env, None).map(|_| ()),
                cx,
            );
        }
    }

    pub fn install_launcher_kit(&mut self, cx: &mut Context<Self>) {
        let done = fill(self.text.done_setup, "Launcher Kit");
        #[cfg(windows)]
        self.run("launcher-kit".into(), Work::Setup, done, actions::install_launcher_kit, cx);
        #[cfg(not(windows))]
        let _ = done;
    }

    /// Starts the game through the Launcher Kit and steps aside, minimized, until it is wanted
    /// again.
    pub fn play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(windows)]
        if let Some(lk) = self.env.lk.clone() {
            match lk.launch() {
                Ok(()) => window.minimize_window(),
                Err(e) => {
                    self.notice = Some(Notice { ok: false, text: fill(self.text.failed, e) });
                    cx.notify();
                }
            }
        }
        #[cfg(not(windows))]
        let _ = (window, cx);
    }

    /// Whether an installed mod carries code, which only the Launcher Kit's launcher loads.
    pub fn has_code(&self) -> bool {
        self.env.installed.mods.iter().any(|m| m.files.iter().any(|f| f.name.to_ascii_lowercase().ends_with(".dll")))
    }

    /// What stops mods from being installed at all, in a sentence for the banner.
    pub fn setup_problem(&self) -> Option<String> {
        let issues = compat::setup_issues(&self.env);
        issues.iter().find(|i| i.blocks()).map(|i| issue_text(i, self.text, &self.catalog))
    }

    fn key_down(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if ev.keystroke.key == "escape" {
            if self.adding.is_some() {
                self.adding = None;
                cx.notify();
            } else if self.confirm.is_some() {
                self.confirm = None;
                cx.notify();
            } else if self.screen != Screen::Catalog {
                self.go(Screen::Catalog, cx);
            }
        }
    }

    fn nav(&self, id: &'static str, label: String, screen: Screen, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let on = match (&self.screen, &screen) {
            (Screen::Mod(_), Screen::Catalog) => true,
            (a, b) => a == b,
        };
        div()
            .id(id)
            .flex()
            .items_center()
            .h(px(30.))
            .px(px(14.))
            .rounded(px(radius::CONTROL))
            .cursor_pointer()
            .when(on, |d| d.bg(t.layer).border_1().border_color(t.stroke_strong))
            .child(body(label, if on { t.text } else { t.text2 }).font_weight(gpui::FontWeight::MEDIUM))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| this.go(screen.clone(), cx)))
            .into_any_element()
    }

    /// The bar across the top: the brand, the screens, playing and the settings.
    fn top_bar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let framed = crate::frame::tiling(window).is_some();
        let mark = img(self.brand.1.clone()).size(px(BRAND)).flex_none();
        let home = div()
            .id("brand")
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(6.))
            .py(px(4.))
            .rounded(px(radius::CONTROL))
            .cursor_pointer()
            .hover(move |s| s.bg(t.control))
            .on_click(cx.listener(|this, _, _, cx| this.go(Screen::Catalog, cx)))
            .when(framed, |d| d.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()))
            .child(mark)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(title("SporeOne", t.text).line_height(px(18.)))
                    .child(div().text_size(px(10.)).line_height(px(12.)).text_color(t.text3).child(text.mod_manager)),
            );
        let count = self.env.installed.mods.len() + self.external().len();
        let installed = if count == 0 { text.installed.to_string() } else { format!("{} {count}", text.installed) };
        let updates = !self.updates().is_empty();
        let nav = div()
            .flex()
            .items_center()
            .gap(px(2.))
            .p(px(3.))
            .rounded(px(radius::TRACK))
            .bg(t.control)
            .child(self.nav("nav-catalog", text.catalog.to_string(), Screen::Catalog, cx))
            .child(
                div()
                    .relative()
                    .child(self.nav("nav-installed", installed, Screen::Installed, cx))
                    .when(updates, |d| d.child(div().absolute().top(px(4.)).right(px(6.)).size(px(7.)).rounded_full().bg(t.accent))),
            )
            .child(self.nav("nav-tweaks", text.tweaks.to_string(), Screen::Tweaks, cx));
        let can_play = self.env.lk.is_some() && self.env.game().is_some_and(|g| g.has_ga());
        let play = crate::widgets::with_tip(
            icon_label_button("play", glyph::PLAY, text.play, if can_play { Kind::Primary } else { Kind::Standard }, &t)
                .when(!can_play, |d| d.opacity(0.5).cursor_default())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    if can_play {
                        this.play(window, cx)
                    }
                })),
            text.play_hint,
            &t,
        );
        let attention = self.setup_problem().is_some();
        let on_settings = self.screen == Screen::Settings;
        let gear = div()
            .relative()
            .child(icon_button("settings", glyph::SETTINGS, &t).when(on_settings, |d| d.bg(t.control)).on_click(cx.listener(
                |this, _, _, cx| {
                    let next = if this.screen == Screen::Settings { Screen::Catalog } else { Screen::Settings };
                    this.go(next, cx)
                },
            )))
            .when(attention, |d| d.child(div().absolute().top(px(6.)).right(px(6.)).size(px(8.)).rounded_full().bg(t.caution)))
            .when(framed, |d| d.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()));
        let bar = div().flex().flex_none().justify_center().h(px(60.)).border_b_1().border_color(t.stroke);
        let row = div()
            .flex()
            .items_center()
            .gap(px(20.))
            .w_full()
            .max_w(px(layout::MAX_W))
            .px(px(layout::GUTTER - 6.))
            .child(home)
            .child(nav)
            .child(div().flex_1())
            .child(play)
            .child(gear);
        self.title_bar(bar, row, window, cx).into_any_element()
    }

    /// A newer SporeOne, under the bar on top, until it is dismissed.
    fn update_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let text = self.text;
        let (r, working, error) = match self.app_update.as_ref()? {
            AppUpdate::Available(r) => (r, false, None),
            AppUpdate::Working(r) => (r, true, None),
            AppUpdate::Failed(r, e) => (r, false, Some(e.clone())),
            _ => return None,
        };
        let page = r.page.clone();
        let label = if working {
            text.update_working
        } else if update::installed_by_setup() && r.setup.is_some() {
            text.update_now
        } else {
            text.update_download
        };
        let message = match error {
            Some(e) => fill(text.update_failed, e),
            None => fill(text.app_update, &r.version),
        };
        Some(
            div()
                .flex()
                .flex_none()
                .justify_center()
                .bg(t.layer)
                .border_b_1()
                .border_color(t.stroke)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .w_full()
                        .max_w(px(layout::MAX_W))
                        .px(px(layout::GUTTER))
                        .py(px(8.))
                        .child(div().flex_none().size(px(8.)).rounded_full().bg(t.accent))
                        .child(body(message, t.text).flex_1().min_w(px(0.)))
                        .child(button("update-notes", text.update_notes, Kind::Subtle, &t).on_click(move |_, _, cx| cx.open_url(&page)))
                        .child(
                            button("update-now", label, Kind::Primary, &t)
                                .when(working, |d| d.opacity(0.6).cursor_default())
                                .on_click(cx.listener(|this, _, _, cx| this.start_app_update(cx))),
                        )
                        .when(!working, |d| {
                            d.child(icon_button("update-close", glyph::CLOSE, &t).on_click(cx.listener(|this, _, _, cx| {
                                this.app_update = None;
                                cx.notify();
                            })))
                        }),
                )
                .into_any_element(),
        )
    }

    /// The dialog that adds a mod from a GitHub link or a file, over the screen.
    fn adding_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let text = self.text;
        let a = self.adding.as_ref()?;
        let working = a.working;
        let add = button("add-link", if working { text.add_working } else { text.add_from_link }, Kind::Primary, &t)
            .when(working, |d| d.opacity(0.6).cursor_default())
            .on_click(cx.listener(|this, _, _, cx| this.add_from_link(cx)));
        let panel = div()
            .id("adding")
            .flex()
            .flex_col()
            .gap(px(16.))
            .w(px(520.))
            .p(px(28.))
            .rounded(px(radius::SURFACE))
            .bg(t.layer)
            .border_1()
            .border_color(t.stroke)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(crate::widgets::heading(text.add_mod, t.text))
            .child(body(text.add_body, t.text2))
            .child(div().flex().items_center().gap(px(8.)).child(div().flex_1().min_w(px(0.)).child(a.link.clone())).child(add))
            .children(a.error.clone().map(|e| body(e, t.critical)))
            .child(div().h(px(1.)).bg(t.stroke).my(px(4.)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(caption(text.suggest_body, t.text2).flex_1().min_w(px(0.)))
                    .child(button("suggest", text.suggest, Kind::Subtle, &t).on_click(|_, _, cx| cx.open_url(github::SUGGEST_URL)))
                    .child(
                        icon_label_button("pick-file", glyph::FOLDER, text.add_pick_file, Kind::Standard, &t)
                            .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                    ),
            );
        Some(
            div()
                .id("scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(t.scrim)
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        if this.adding.as_ref().is_some_and(|a| !a.working) {
                            this.adding = None;
                            cx.notify();
                        }
                    }),
                )
                .child(panel)
                .into_any_element(),
        )
    }

    /// How the last install or uninstall went, under the bar on top.
    fn notice_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let n = self.notice.as_ref()?;
        let (glyph_name, color, bg) = if n.ok { (glyph::CHECK, t.success, t.layer) } else { (glyph::WARNING, t.caution, t.caution_layer) };
        Some(
            div()
                .flex()
                .flex_none()
                .justify_center()
                .bg(bg)
                .border_b_1()
                .border_color(t.stroke)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .w_full()
                        .max_w(px(layout::MAX_W))
                        .px(px(layout::GUTTER))
                        .py(px(8.))
                        .child(icon(glyph_name, 16., color))
                        .child(body(n.text.clone(), t.text).flex_1().min_w(px(0.)))
                        .child(icon_button("notice-close", glyph::CLOSE, &t).on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        }))),
                )
                .into_any_element(),
        )
    }
}

/// The catalog with the mods added from a link that it does not have, and their ids.
fn merged(base: &Catalog, custom: &[Entry]) -> (Catalog, HashSet<String>) {
    let mut catalog = base.clone();
    let mut ids = HashSet::new();
    for e in custom {
        if catalog.get(&e.id).is_none() {
            ids.insert(e.id.clone());
            catalog.mods.push(e.clone());
        }
    }
    (catalog, ids)
}

/// An issue, said plainly.
pub fn issue_text(i: &Issue, t: &Text, catalog: &Catalog) -> String {
    let names = |list: &[String]| list.join(", ");
    match i {
        Issue::NoGame => t.issue_no_game.into(),
        Issue::NoGalacticAdventures => t.issue_no_ga.into(),
        Issue::UnsupportedVersion => t.issue_unsupported.into(),
        Issue::NoLauncherKit => t.issue_no_lk.into(),
        Issue::OldDlls(v) => fill(t.issue_old_dlls, v),
        Issue::ModLoader => t.issue_mod_loader.into(),
        Issue::WrongStore(stores) => {
            let joined: Vec<&str> = stores.iter().map(|s| store_name(*s, t)).collect();
            fill(t.issue_wrong_store, joined.join(" / "))
        }
        Issue::Unavailable(deps) => fill(t.issue_unavailable, deps.iter().map(|d| d.unique.clone()).collect::<Vec<_>>().join(", ")),
        Issue::Broken => t.issue_broken.into(),
        Issue::SeveralCopies => t.issue_several.into(),
        Issue::StartOnce => t.issue_start_once.into(),
        Issue::Experimental => t.issue_experimental.into(),
        Issue::GalaxyReset => t.issue_galaxy_reset.into(),
        Issue::SaveDependency => t.issue_save_dependency.into(),
        Issue::Old => t.issue_old.into(),
        Issue::Conflicts(n) => fill(t.issue_conflicts, names(n)),
        Issue::Replaces(n) => fill(t.issue_replaces, names(n)),
        Issue::Overlaps(n) => fill(t.issue_overlaps, names(n)),
        Issue::Brings(ids) => {
            let shown: Vec<String> = ids.iter().map(|id| catalog.get(id).map_or_else(|| id.clone(), |e| e.name.clone())).collect();
            fill(t.issue_brings, names(&shown))
        }
    }
}

pub fn store_name(s: sporeone_core::game::Store, t: &Text) -> &'static str {
    use sporeone_core::game::Store;
    match s {
        Store::Steam => t.store_steam,
        Store::Gog => t.store_gog,
        Store::Ea => t.store_ea,
        Store::Disc => t.store_disc,
        Store::Unknown => t.store_unknown,
    }
}

pub fn category_name(c: Category, t: &Text) -> &'static str {
    match c {
        Category::Gameplay => t.cat_gameplay,
        Category::Fixes => t.cat_fixes,
        Category::Editors => t.cat_editors,
        Category::Visuals => t.cat_visuals,
        Category::Multiplayer => t.cat_multiplayer,
        Category::Tools => t.cat_tools,
    }
}

pub fn category_icon(c: Category) -> &'static str {
    match c {
        Category::Gameplay => glyph::GAMEPLAY,
        Category::Fixes => glyph::FIXES,
        Category::Editors => glyph::EDITORS,
        Category::Visuals => glyph::VISUALS,
        Category::Multiplayer => glyph::MULTIPLAYER,
        Category::Tools => glyph::TOOLS,
    }
}

/// A count the way people read it: 19.4k.
pub fn short_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1000.).replace(".0k", "k"),
        _ => format!("{:.1}M", n as f64 / 1_000_000.),
    }
}

/// `2026-10-27T...` as `27/10/2026` or `Oct 27, 2026`.
pub fn date(iso: &str, lang: Lang) -> String {
    let (Some(y), Some(m), Some(d)) = (iso.get(0..4), iso.get(5..7).and_then(|m| m.parse::<usize>().ok()), iso.get(8..10)) else {
        return String::new();
    };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    match lang {
        Lang::Pt => format!("{d}/{m:02}/{y}"),
        Lang::En => format!("{} {}, {y}", MONTHS.get(m.wrapping_sub(1)).unwrap_or(&""), d.trim_start_matches('0')),
    }
}

pub fn size_text(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.0} KB", bytes as f64 / 1024.),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.),
    }
}

/// The small icon for the window's `scale`: the one made for the nearest size at or above what the
/// screen needs (`scripts/make_icon.py`), so it is never stretched up.
fn brand(scale: f32) -> (f32, Arc<Image>) {
    const SIZES: [(f32, &[u8]); 4] = [
        (28., include_bytes!("../../../assets/brand/brand-28.png")),
        (35., include_bytes!("../../../assets/brand/brand-35.png")),
        (42., include_bytes!("../../../assets/brand/brand-42.png")),
        (56., include_bytes!("../../../assets/brand/brand-56.png")),
    ];
    let want = BRAND * scale - 0.5;
    let bytes = SIZES.iter().find(|(side, _)| *side >= want).unwrap_or(&SIZES[3]).1;
    (scale, Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec())))
}

/// How big the icon is in the bar on top.
const BRAND: f32 = 28.;

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        if self.brand.0 != window.scale_factor() {
            self.brand = brand(window.scale_factor());
        }
        let max_w = if matches!(self.screen, Screen::Settings | Screen::Tweaks) { layout::SETTINGS_W } else { layout::MAX_W };
        let content: AnyElement = match self.screen.clone() {
            Screen::Catalog => self.render_home(window, cx),
            Screen::Mod(id) => match self.entry(&id).cloned() {
                Some(e) => self.render_page(&e, window, cx),
                None => self.render_home(window, cx),
            },
            Screen::Installed => self.render_library(window, cx),
            Screen::Tweaks => self.render_tweaks(window, cx),
            Screen::Settings => self.render_settings(window, cx),
        };
        let page = div()
            .id("root")
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(t.base)
            .text_color(t.text)
            .font_family(FONT)
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(t.accent_soft))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.install_files(paths.paths().to_vec(), cx)))
            .child(self.top_bar(window, cx))
            .children(self.update_bar(cx))
            .children(self.notice_bar(cx))
            .child(
                div().id("scroll").flex_1().min_h(px(0.)).overflow_y_scroll().track_scroll(&self.scroll).child(
                    div()
                        .flex()
                        .justify_center()
                        .w_full()
                        .child(div().w_full().max_w(px(max_w)).px(px(layout::GUTTER)).pt(px(32.)).pb(px(48.)).child(content)),
                ),
            )
            .children(self.adding_dialog(cx));
        crate::frame::frame(page.into_any_element(), &t, window)
    }
}

/// A banner over a screen: something to know, and what to do about it.
pub fn banner(message: String, action: Option<AnyElement>, t: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(20.))
        .py(px(12.))
        .mb(px(24.))
        .rounded(px(radius::PANEL))
        .bg(t.caution_layer)
        .border_1()
        .border_color(t.stroke)
        .child(icon(glyph::WARNING, 16., t.caution))
        .child(body(message, t.text).flex_1().min_w(px(0.)))
        .children(action)
}

/// The button that opens the settings, where what is missing can be installed.
pub fn needed_button(text: &'static Text, t: &Theme, cx: &mut Context<MainView>) -> AnyElement {
    button("needed", text.see_what_is_needed, Kind::Standard, t)
        .on_click(cx.listener(|this, _, _, cx| this.go(Screen::Settings, cx)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_dates() {
        assert_eq!(short_count(950), "950");
        assert_eq!(short_count(19_477), "19.5k");
        assert_eq!(short_count(2_000), "2k");
        assert_eq!(date("2026-10-07T21:33:52Z", Lang::En), "Oct 7, 2026");
        assert_eq!(date("2026-10-07T21:33:52Z", Lang::Pt), "07/10/2026");
        assert_eq!(date("", Lang::Pt), "");
        assert_eq!(size_text(3_312_270), "3.2 MB");
    }

    #[test]
    fn added_mods_join_the_catalog_until_it_has_them() {
        let entry = |id: &str| Entry { id: id.into(), name: id.into(), ..Default::default() };
        let base = Catalog { mods: vec![entry("a/listed")], ..Default::default() };
        let (catalog, ids) = merged(&base, &[entry("a/listed"), entry("b/mine")]);
        assert_eq!(catalog.mods.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["a/listed", "b/mine"]);
        assert_eq!(ids, HashSet::from(["b/mine".to_string()]));
    }
}
