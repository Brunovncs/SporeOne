//! The settings: the game SporeOne found and whether mods can run on it, the Launcher Kit, the
//! catalog, and the language.

use crate::theme::{Theme, icon as glyph};
use crate::ui::{MainView, date, store_name};
use crate::widgets::{Kind as Button, body, caption, display, group, icon, icon_label_button, row, section, segmented, strong};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Context, Div, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div, px};
use sporeone_core::i18n::{Lang, fill};
use std::path::PathBuf;

fn line(label: &'static str, value: String, ok: Option<bool>, t: &Theme) -> Div {
    row(t)
        .child(body(label, t.text2).w(px(200.)).flex_none())
        .child(body(value, t.text).flex_1().min_w(px(0.)).truncate())
        .children(ok.map(|ok| icon(if ok { glyph::CHECK } else { glyph::WARNING }, 16., if ok { t.success } else { t.caution })))
}

fn site(t: &Theme, text: &sporeone_core::i18n::Text) -> gpui::Stateful<Div> {
    icon_label_button("lk-site", glyph::OPEN, text.lk_site, Button::Subtle, t)
        .on_click(|_, _, cx| cx.open_url(sporeone_core::launcher_kit::SITE))
}

fn open_folder(path: PathBuf) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static {
    move |_, _, cx| cx.open_url(&format!("file:///{}", path.display().to_string().replace('\\', "/")))
}

impl MainView {
    pub fn render_settings(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let env = self.env.clone();
        let mut game = group();
        match env.game() {
            None => game = game.child(row(&t).child(icon(glyph::WARNING, 16., t.caution)).child(body(text.game_none, t.text).flex_1())),
            Some(g) => {
                let root = g.root.clone();
                game = game
                    .child(
                        row(&t)
                            .child(body(text.game_folder, t.text2).w(px(200.)).flex_none())
                            .child(body(g.root.display().to_string(), t.text).flex_1().min_w(px(0.)).truncate())
                            .child(
                                icon_label_button("game-folder", glyph::FOLDER, text.open_folder, Button::Subtle, &t)
                                    .on_click(open_folder(root)),
                            ),
                    )
                    .child(line(text.game_ga, if g.has_ga() { text.yes } else { text.no }.into(), Some(g.has_ga()), &t))
                    .child(line(
                        text.game_build,
                        match (g.edition, g.exe_version) {
                            (Some(e), Some(v)) => format!("{} ({v})", e.label()),
                            (Some(e), None) => e.label().to_string(),
                            (None, _) if !g.has_ga() => "".into(),
                            (None, _) => text.game_build_unknown.into(),
                        },
                        g.has_ga().then_some(g.edition.is_some()),
                        &t,
                    ))
                    .child(line(text.game_store, store_name(g.store, text).into(), None, &t));
                for i in
                    sporeone_core::compat::setup_issues(&env).iter().filter(|i| !matches!(i, sporeone_core::compat::Issue::NoLauncherKit))
                {
                    game = game.child(
                        row(&t)
                            .child(icon(
                                if i.blocks() { glyph::BLOCK } else { glyph::WARNING },
                                16.,
                                if i.blocks() { t.critical } else { t.caution },
                            ))
                            .child(body(crate::ui::issue_text(i, text, &self.catalog), t.text).flex_1()),
                    );
                }
            }
        }

        let mut lk = group().child(row(&t).child(caption(text.lk_about, t.text2).flex_1()));
        match &env.lk {
            None => {
                let busy = self.busy.contains_key("launcher-kit");
                lk = lk.child(
                    row(&t).child(body(text.lk_none, t.text).flex_1()).child(
                        icon_label_button(
                            "lk-install",
                            glyph::DOWNLOAD,
                            if busy { text.working_download } else { text.lk_install },
                            Button::Primary,
                            &t,
                        )
                        .when(busy, |d| d.opacity(0.7))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !busy {
                                this.install_launcher_kit(cx)
                            }
                        })),
                    ),
                );
            }
            Some(k) => {
                let dir = k.dir.clone();
                lk = lk
                    .child(
                        row(&t)
                            .child(body(text.lk_folder, t.text2).w(px(200.)).flex_none())
                            .child(body(k.dir.display().to_string(), t.text).flex_1().min_w(px(0.)).truncate())
                            .child(
                                icon_label_button("lk-folder", glyph::FOLDER, text.open_folder, Button::Subtle, &t)
                                    .on_click(open_folder(dir)),
                            ),
                    )
                    .child(line(text.lk_version, k.version.map(|v| v.to_string()).unwrap_or_default(), None, &t))
                    .child(line(text.lk_dlls, k.dlls_build.map(|v| v.to_string()).unwrap_or_default(), None, &t));
                #[cfg(windows)]
                let uninstaller = {
                    let k2 = k.clone();
                    icon_label_button("lk-uninstaller", glyph::OPEN, text.lk_uninstaller, Button::Standard, &t)
                        .on_click(move |_, _, _| drop(k2.open_uninstaller()))
                };
                #[cfg(windows)]
                {
                    lk = lk.child(row(&t).child(div().flex_1()).child(site(&t, text)).child(uninstaller));
                }
            }
        }
        if env.lk.is_none() {
            lk = lk.child(row(&t).child(div().flex_1()).child(site(&t, text)));
        }

        let built = date(&self.catalog.generated, self.lang);
        let refreshing = self.refreshing;
        let catalog =
            group()
                .child(
                    row(&t).child(caption(fill(text.catalog_built, built), t.text2).flex_1()).child(
                        icon_label_button(
                            "refresh",
                            glyph::REFRESH,
                            if refreshing { text.catalog_refreshing } else { text.catalog_refresh },
                            Button::Standard,
                            &t,
                        )
                        .when(refreshing, |d| d.opacity(0.7))
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_catalog(cx))),
                    ),
                )
                .children(self.catalog_error.clone().map(|e| {
                    row(&t).child(icon(glyph::WARNING, 16., t.caution)).child(body(fill(text.catalog_failed, e), t.text).flex_1())
                }));

        let language = group().child(row(&t).child(body(text.language, t.text).flex_1()).child(segmented(
            "lang",
            vec![(Lang::En, "English".into()), (Lang::Pt, "Português".into())],
            self.lang,
            &t,
            cx,
            |this, l, cx| this.set_lang(l, cx),
        )));

        let about = group().child(
            row(&t).child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .flex_1()
                    .child(strong(concat!("SporeOne ", env!("CARGO_PKG_VERSION")), t.text))
                    .child(caption(text.about_body, t.text2))
                    .child(
                        div().flex().child(
                            icon_label_button("source", glyph::OPEN, text.source_code, Button::Subtle, &t)
                                .on_click(|_, _, cx| cx.open_url(env!("CARGO_PKG_REPOSITORY"))),
                        ),
                    ),
            ),
        );

        div()
            .flex()
            .flex_col()
            .child(display(text.settings, t.text))
            .child(section(text.game, &t))
            .child(game)
            .child(section(text.launcher_kit, &t))
            .child(lk)
            .child(section(text.catalog_section, &t))
            .child(catalog)
            .child(section(text.language, &t))
            .child(language)
            .child(section(text.about, &t))
            .child(about)
            .into_any_element()
    }
}
