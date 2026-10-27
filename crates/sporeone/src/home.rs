//! The first screen: every mod in the catalog as a tile, found by words or by kind, most
//! downloaded first. What keeps mods from being installed at all shows above them.

use crate::theme::{Theme, icon as glyph, layout, radius};
use crate::ui::{Have, MainView, Screen, Sort, banner, category_icon, category_name, needed_button, short_count};
use crate::widgets::{body, caption, chip, display, icon, segmented, title};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
    div, px,
};
use sporeone_core::catalog::{Category, Entry, Kind, Status};
use sporeone_core::i18n::{Text, fill};

const TILE_MIN_W: f32 = 264.;
const TILE_H: f32 = 236.;
const GAP: f32 = 16.;

/// The chip a tile shows in its corner, if any: installed, an update, or how it is installed.
pub fn state_chip(view: &MainView, e: &Entry, t: &Theme, text: &Text) -> Option<Div> {
    let tinted = |s: &'static str, fg, bg| Some(chip(s, fg, bg));
    match view.have(e) {
        Have::Outdated(_) => tinted(text.chip_update, t.on_accent, t.accent),
        Have::Installed(_) | Have::External => tinted(text.chip_installed, t.accent, t.accent_soft),
        Have::Not => match (e.kind, e.status) {
            (_, Status::Broken) => tinted(text.chip_broken, t.critical, t.control),
            (Kind::Page, _) => tinted(text.chip_elsewhere, t.text2, t.control),
            (Kind::Setup, _) => tinted(text.chip_setup, t.text2, t.control),
            (_, Status::Experimental) => tinted(text.chip_experimental, t.caution, t.caution_layer),
            _ => None,
        },
    }
}

impl MainView {
    fn tile(&self, i: usize, e: &Entry, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let id = e.id.clone();
        let faded = matches!(e.status, Status::Old | Status::Broken);
        let stage = crate::widgets::stage(&t)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(88.))
            .rounded_t(px(radius::TILE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(48.))
                    .rounded_full()
                    .bg(t.layer.opacity(0.7))
                    .border_1()
                    .border_color(t.stroke)
                    .child(icon(category_icon(e.category), 22., if faded { t.text3 } else { t.accent })),
            )
            .children(state_chip(self, e, &t, text).map(|c| div().absolute().top(px(12.)).right(px(12.)).child(c)));
        let mut facts = vec![];
        if e.downloads() > 0 {
            facts.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(icon(glyph::DOWNLOAD, 12., t.text3))
                    .child(caption(short_count(e.downloads()), t.text3)),
            );
        }
        if e.stars > 0 {
            facts.push(
                div().flex().items_center().gap(px(4.)).child(icon(glyph::STAR, 12., t.text3)).child(caption(e.stars.to_string(), t.text3)),
            );
        }
        let year = e.release.as_ref().map(|r| r.published.as_str()).filter(|p| !p.is_empty()).unwrap_or(&e.updated);
        let version = e.version_text().unwrap_or_default();
        div()
            .id(("tile", i))
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width))
            .h(px(TILE_H))
            .overflow_hidden()
            .rounded(px(radius::TILE))
            .bg(t.layer)
            .border_1()
            .border_color(t.stroke)
            .cursor_pointer()
            .hover(move |s| s.bg(t.layer_hover).border_color(t.stroke_strong))
            .on_click(cx.listener(move |this, _, _, cx| this.go(Screen::Mod(id.clone()), cx)))
            .child(stage)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.))
                    .gap(px(4.))
                    .px(px(16.))
                    .pt(px(12.))
                    .pb(px(14.))
                    .child(title(e.name.clone(), if faded { t.text2 } else { t.text }).truncate())
                    .child(caption(format!("{} · {}", fill(text.by, &e.author), category_name(e.category, text)), t.text3).truncate())
                    .child(caption(e.description.clone(), t.text2).line_clamp(2).pt(px(2.)))
                    .child(div().flex_1())
                    .child(div().flex().items_center().gap(px(12.)).children(facts).child(div().flex_1()).child(
                        caption(if version.is_empty() { year.get(..4).unwrap_or("").to_string() } else { version }, t.text3).truncate(),
                    )),
            )
            .into_any_element()
    }

    /// The mods the words and the category pick, in the order chosen.
    pub fn shown(&self) -> Vec<&Entry> {
        let mut list: Vec<&Entry> =
            self.catalog.mods.iter().filter(|e| self.category.is_none_or(|c| e.category == c) && e.matches(&self.query)).collect();
        match self.sort {
            Sort::Popular => {}
            Sort::Recent => {
                let when = |e: &Entry| {
                    e.release.as_ref().map(|r| r.published.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| e.updated.clone())
                };
                list.sort_by_key(|e| std::cmp::Reverse(when(e)));
            }
            Sort::Name => list.sort_by_key(|e| e.name.to_lowercase()),
        }
        list
    }

    fn empty(&self) -> AnyElement {
        let t = self.theme;
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.))
            .py(px(56.))
            .px(px(24.))
            .rounded(px(radius::TILE))
            .border_1()
            .border_dashed()
            .border_color(t.stroke_strong)
            .child(div().flex().items_center().justify_center().size(px(56.)).rounded_full().bg(t.control).child(icon(
                glyph::SEARCH,
                24.,
                t.text2,
            )))
            .child(title(self.text.nothing_found, t.text))
            .child(body(self.text.nothing_found_body, t.text2))
            .into_any_element()
    }

    pub fn render_home(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let banner = self.setup_problem().map(|p| banner(p, Some(needed_button(text, &t, cx)), &t));
        let mut cats: Vec<(Option<Category>, SharedString)> = vec![(None, text.all.into())];
        cats.extend(
            Category::ALL
                .iter()
                .filter(|c| self.catalog.mods.iter().any(|e| e.category == **c))
                .map(|c| (Some(*c), category_name(*c, text).into())),
        );
        let categories = segmented("category", cats, self.category, &t, cx, |this, c, cx| {
            this.category = c;
            cx.notify();
        });
        let sorts = segmented(
            "sort",
            vec![(Sort::Popular, text.sort_popular.into()), (Sort::Recent, text.sort_recent.into()), (Sort::Name, text.sort_name.into())],
            self.sort,
            &t,
            cx,
            |this, s, cx| {
                this.sort = s;
                cx.notify();
            },
        );
        let shown = self.shown();
        let count = fill(text.mods_count, shown.len());
        // As many columns as fit, the tiles stretched to fill the row.
        let width = (crate::frame::inner_width(window) - 2. * layout::GUTTER).min(layout::MAX_W - 2. * layout::GUTTER);
        let cols = ((width + GAP) / (TILE_MIN_W + GAP)).floor().max(1.);
        let tile_w = ((width - GAP * (cols - 1.)) / cols).floor();
        let entries: Vec<Entry> = shown.into_iter().cloned().collect();
        let mut grid = div().flex().flex_wrap().gap(px(GAP));
        for (i, e) in entries.iter().enumerate() {
            grid = grid.child(self.tile(i, e, tile_w, cx));
        }
        let search = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .w(px(300.))
            .child(icon(glyph::SEARCH, 16., t.text3))
            .child(div().flex_1().child(self.search.clone()));
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .pb(px(20.))
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(px(12.))
                            .flex_1()
                            .min_w(px(0.))
                            .child(display(text.catalog, t.text))
                            .child(caption(count, t.text3).pb(px(6.))),
                    )
                    .child(search),
            )
            .children(banner)
            .child(div().flex().flex_wrap().items_center().justify_between().gap(px(12.)).pb(px(20.)).child(categories).child(sorts))
            .when(entries.is_empty(), |d| d.child(self.empty()))
            .when(!entries.is_empty(), |d| d.child(grid))
            .child(caption(text.drop_hint, t.text3).pt(px(24.)))
            .into_any_element()
    }
}
