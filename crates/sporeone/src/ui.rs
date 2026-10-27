//! The window: a bar on top, and under it one of four screens: the catalog as tiles, one mod's
//! page, what is installed, or the settings with what the game and the Launcher Kit need. Files
//! dropped anywhere on it are installed.

use crate::text_field::{TextField, TextFieldEvent};
use crate::theme::{FONT, Theme, icon as glyph, layout, radius};
use crate::widgets::{Kind, body, button, caption, icon, icon_button, icon_label_button, strong};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, Context, Entity, ExternalPaths, FocusHandle, Image, ImageFormat, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window, WindowAppearance, div, img, px,
};
use sporeone_core::actions::{self, Failure};
use sporeone_core::catalog::{Catalog, Category, Entry};
use sporeone_core::compat::{self, Env, Issue};
use sporeone_core::i18n::{self, Lang, Text, fill};
use sporeone_core::install::Choices;
use sporeone_core::installed::InstalledMod;
use sporeone_core::state::{Origin, State};
use std::collections::{BTreeMap, HashMap};
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
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Popular,
    Recent,
    Name,
}

/// What is being done to a mod right now.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Work {
    Install,
    Uninstall,
    Setup,
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
    pub catalog: Arc<Catalog>,
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
        let search = cx.new(|cx| TextField::new(cx, false, 80).placeholder(text.search));
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
        let catalog = if demo { crate::demo::catalog() } else { sporeone_core::state::cached_catalog() };
        let mut view = MainView {
            theme: theme(window),
            focus,
            screen: Screen::Catalog,
            lang,
            text,
            catalog: Arc::new(catalog),
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
            brand: brand(window.scale_factor()),
            moving: false,
            _subscriptions: vec![searched, theme_change, activation],
        };
        // `--demo` with `SPOREONE_PAGE`: a screen other than the catalog, for screenshots.
        if demo && let Ok(page) = std::env::var("SPOREONE_PAGE") {
            view.screen = match page.as_str() {
                "installed" => Screen::Installed,
                "settings" => Screen::Settings,
                id => Screen::Mod(id.to_string()),
            };
        }
        if !demo {
            view.refresh_catalog(cx);
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
                    Ok(c) if c.generated >= this.catalog.generated => this.catalog = Arc::new(c),
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

    pub fn play(&mut self, cx: &mut Context<Self>) {
        #[cfg(windows)]
        if let Some(lk) = self.env.lk.clone()
            && let Err(e) = lk.launch()
        {
            self.notice = Some(Notice { ok: false, text: fill(self.text.failed, e) });
            cx.notify();
        }
        #[cfg(not(windows))]
        let _ = cx;
    }

    /// What stops mods from being installed at all, in a sentence for the banner.
    pub fn setup_problem(&self) -> Option<String> {
        let issues = compat::setup_issues(&self.env);
        issues.iter().find(|i| i.blocks()).map(|i| issue_text(i, self.text, &self.catalog))
    }

    fn key_down(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if ev.keystroke.key == "escape" {
            if self.confirm.is_some() {
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
            .h(px(34.))
            .px(px(12.))
            .rounded(px(radius::CONTROL))
            .cursor_pointer()
            .when(on, |d| d.bg(t.control))
            .hover(move |s| s.bg(t.control))
            .child(body(label, if on { t.text } else { t.text2 }))
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
            .child(strong("SporeOne", t.text))
            .child(caption(concat!("v", env!("CARGO_PKG_VERSION")), t.text2));
        let count = self.env.installed.mods.len() + self.external().len();
        let installed = if count == 0 { text.installed.to_string() } else { format!("{} {count}", text.installed) };
        let updates = !self.updates().is_empty();
        let nav =
            div().flex().items_center().gap(px(4.)).child(self.nav("nav-catalog", text.catalog.to_string(), Screen::Catalog, cx)).child(
                div()
                    .relative()
                    .child(self.nav("nav-installed", installed, Screen::Installed, cx))
                    .when(updates, |d| d.child(div().absolute().top(px(6.)).right(px(4.)).size(px(7.)).rounded_full().bg(t.accent))),
            );
        let can_play = self.env.lk.is_some() && self.env.game().is_some_and(|g| g.has_ga());
        let play = crate::widgets::with_tip(
            icon_label_button("play", glyph::PLAY, text.play, if can_play { Kind::Primary } else { Kind::Standard }, &t)
                .when(!can_play, |d| d.opacity(0.5).cursor_default())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _, cx| {
                    if can_play {
                        this.play(cx)
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
        let bar = div().flex().flex_none().justify_center().h(px(56.)).border_b_1().border_color(t.stroke);
        let row = div()
            .flex()
            .items_center()
            .gap(px(16.))
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

/// The small icon for the window's `scale`. GPUI rasterizes an SVG image at twice its own size
/// whatever the scale, and this one is 16 by 16, blurry above 100%: given a size, it comes out
/// with one pixel for each of the screen's.
fn brand(scale: f32) -> (f32, Arc<Image>) {
    let side = BRAND * scale / 2.;
    let svg = include_str!("../../../assets/icon-small.svg").replacen("<svg ", &format!("<svg width=\"{side}\" height=\"{side}\" "), 1);
    (scale, Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())))
}

/// How big the icon is in the bar on top.
const BRAND: f32 = 28.;

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        if self.brand.0 != window.scale_factor() {
            self.brand = brand(window.scale_factor());
        }
        let max_w = if self.screen == Screen::Settings { layout::SETTINGS_W } else { layout::MAX_W };
        let content: AnyElement = match self.screen.clone() {
            Screen::Catalog => self.render_home(window, cx),
            Screen::Mod(id) => match self.entry(&id).cloned() {
                Some(e) => self.render_page(&e, window, cx),
                None => self.render_home(window, cx),
            },
            Screen::Installed => self.render_library(window, cx),
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
            .children(self.notice_bar(cx))
            .child(
                div().id("scroll").flex_1().min_h(px(0.)).overflow_y_scroll().child(
                    div()
                        .flex()
                        .justify_center()
                        .w_full()
                        .child(div().w_full().max_w(px(max_w)).px(px(layout::GUTTER)).pt(px(28.)).pb(px(40.)).child(content)),
                ),
            );
        crate::frame::frame(page.into_any_element(), &t, window)
    }
}

/// A banner over a screen: something to know, and what to do about it.
pub fn banner(message: String, action: Option<AnyElement>, t: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(16.))
        .py(px(12.))
        .mb(px(20.))
        .rounded(px(radius::CARD))
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
}
