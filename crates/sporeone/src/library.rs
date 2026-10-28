//! What is installed: every mod in the Launcher Kit's list, wherever it came from, and the mods
//! with installers of their own that the game's folder shows. Updates are one click from here.

use crate::theme::{icon as glyph, radius};
use crate::ui::{Have, MainView, Screen, Work, category_icon};
use crate::widgets::{
    Kind as Button, body, button, caption, display, group, icon, icon_button, icon_label_button, row, status, strong, title,
};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div, px};
use sporeone_core::catalog::{Category, Entry};
use sporeone_core::i18n::fill;
use sporeone_core::installed::InstalledMod;

impl MainView {
    /// Catalog mods put in place by installers of their own.
    pub fn external(&self) -> Vec<&Entry> {
        self.catalog.mods.iter().filter(|e| matches!(self.have(e), Have::External)).collect()
    }

    /// The catalog's entry for an installed mod, if it has one.
    pub fn entry_of(&self, m: &InstalledMod) -> Option<&Entry> {
        self.catalog.by_unique(&m.unique).or_else(|| self.origins.get(&m.name).and_then(|o| self.catalog.get(&o.id)))
    }

    fn installed_row(&self, i: usize, m: &InstalledMod, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let entry = self.entry_of(m).cloned();
        let outdated = entry.as_ref().is_some_and(|e| matches!(self.have(e), Have::Outdated(_)));
        let key = entry.as_ref().map_or_else(|| m.name.clone(), |e| e.id.clone());
        let origin = if entry.is_some() && self.origins.contains_key(&m.name) { text.from_catalog } else { text.by_hand };
        let version = m
            .version
            .map(|v| v.to_string())
            .or_else(|| self.origins.get(&m.name).map(|o| o.tag.trim_start_matches(['v', 'V']).to_string()));
        let line = match version {
            Some(v) => format!("{} {v} · {origin}", text.version),
            None => origin.to_string(),
        };
        let category = entry.as_ref().map_or(Category::Gameplay, |e| e.category);
        let name = m.name.clone();
        let actions = if let Some(work) = self.busy.get(&key) {
            let label = match work {
                Work::Install => text.working_install,
                Work::Uninstall => text.working_uninstall,
                Work::Setup => text.working_setup,
            };
            div().child(caption(format!("{label}…"), t.text2))
        } else if self.confirm.as_deref() == Some(name.as_str()) {
            div().w(px(340.)).child(self.confirm_box(&name, &key, cx))
        } else {
            let open = entry.as_ref().map(|e| e.id.clone());
            let update = entry.as_ref().filter(|_| outdated).map(|e| e.id.clone());
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .children(update.map(|id| {
                    icon_label_button(("update", i), glyph::DOWNLOAD, text.update, Button::Primary, &t)
                        .on_click(cx.listener(move |this, _, _, cx| this.install(&id, cx)))
                }))
                .children(open.map(|id| {
                    icon_button(("open", i), glyph::CHEVRON_RIGHT, &t)
                        .on_click(cx.listener(move |this, _, _, cx| this.go(Screen::Mod(id.clone()), cx)))
                }))
                .child(icon_button(("remove", i), glyph::DELETE, &t).on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm = Some(name.clone());
                    cx.notify();
                })))
        };
        row(&t)
            .child(icon(category_icon(category), 20., t.text3))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(strong(m.shown_name().to_string(), t.text).truncate())
                            .when(outdated, |d| d.child(status(text.chip_update, t.accent, t.text))),
                    )
                    .child(caption(line, t.text3).truncate()),
            )
            .child(actions)
            .into_any_element()
    }

    fn external_row(&self, i: usize, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let id = e.id.clone();
        row(&t)
            .child(icon(category_icon(e.category), 20., t.text3))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .child(strong(e.name.clone(), t.text))
                    .child(caption(self.text.own_installer, t.text3)),
            )
            .child(
                icon_button(("external", i), glyph::CHEVRON_RIGHT, &t)
                    .on_click(cx.listener(move |this, _, _, cx| this.go(Screen::Mod(id.clone()), cx))),
            )
            .into_any_element()
    }

    pub fn render_library(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let mut mods: Vec<InstalledMod> = self.env.installed.mods.clone();
        mods.sort_by_key(|m| m.shown_name().to_lowercase());
        let external: Vec<Entry> = self.external().into_iter().cloned().collect();
        let updates = self.updates().len();
        let total = mods.len() + external.len();
        let mut list = group(&t);
        for (i, m) in mods.iter().enumerate() {
            list = list.child(self.installed_row(i, m, cx));
        }
        for (i, e) in external.iter().enumerate() {
            list = list.child(self.external_row(i, e, cx));
        }
        let empty = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .py(px(64.))
            .px(px(24.))
            .rounded(px(radius::PANEL))
            .border_1()
            .border_dashed()
            .border_color(t.stroke_strong)
            .child(title(text.installed_empty, t.text))
            .child(body(text.installed_empty_body, t.text2).text_center().max_w(px(460.)))
            .child(
                button("to-catalog", text.catalog, Button::Standard, &t)
                    .mt(px(12.))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Screen::Catalog, cx))),
            );
        let banner = self.setup_problem().map(|p| crate::ui::banner(p, Some(crate::ui::needed_button(text, &t, cx)), &t));
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .pb(px(24.))
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(px(12.))
                            .flex_1()
                            .child(display(text.installed, t.text))
                            .child(caption(if total == 0 { String::new() } else { fill(text.mods_count, total) }, t.text3).pb(px(8.)))
                            .when(updates > 0, |d| {
                                d.child(
                                    caption(
                                        if updates == 1 { text.update_one.to_string() } else { fill(text.updates_count, updates) },
                                        t.accent,
                                    )
                                    .pb(px(8.)),
                                )
                            }),
                    )
                    .when(updates > 1, |d| {
                        d.child(
                            icon_label_button("update-all", glyph::DOWNLOAD, text.update_all, Button::Primary, &t)
                                .on_click(cx.listener(|this, _, _, cx| this.update_all(cx))),
                        )
                    }),
            )
            .children(banner)
            .when(total == 0, |d| d.child(empty))
            .when(total > 0, |d| d.child(list))
            .child(caption(text.drop_hint, t.text3).pt(px(24.)))
            .into_any_element()
    }
}
