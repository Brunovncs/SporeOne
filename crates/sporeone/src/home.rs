//! The first screen: every mod in the catalog, as a list or as tiles, found by words or by kind,
//! most downloaded first. A mod without options installs from its row; what keeps mods from being
//! installed at all shows above them.

use crate::theme::{Theme, icon as glyph, layout, radius};
use crate::ui::{Have, MainView, Screen, Sort, View, Work, banner, category_icon, category_name, needed_button, short_count};
use crate::widgets::{
    Kind as Button, body, button, caption, card, display, filters, icon, icon_label_button, icon_segmented, segmented, status, title,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
    div, px,
};
use sporeone_core::catalog::{Category, Entry, Kind, Status};
use sporeone_core::i18n::{Text, fill};

const TILE_MIN_W: f32 = 256.;
const TILE_H: f32 = 200.;
const GAP: f32 = 16.;
/// A row of the list, and how far apart rows start: each draws the line above it over the last
/// pixel of the one before.
const ROW_H: f32 = 72.;
const ROW_STEP: f32 = ROW_H - 1.;
/// Rows drawn beyond the screen's edges, so scrolling never shows a gap before the next frame.
const SPARE: usize = 4;

/// A word on where a mod stands, if there is one to say: installed, an update, or why it is
/// different from the rest.
pub fn state_of(view: &MainView, e: &Entry, t: &Theme, text: &Text) -> Option<Div> {
    Some(match view.have(e) {
        Have::Outdated(_) => status(text.chip_update, t.accent, t.text),
        Have::Installed(_) | Have::External => status(text.chip_installed, t.success, t.text2),
        Have::Not => match (e.kind, e.status) {
            (_, Status::Broken) => status(text.chip_broken, t.critical, t.critical),
            (Kind::Page, _) => status(text.chip_elsewhere, t.text3, t.text3),
            (Kind::Setup, _) => status(text.chip_setup, t.text3, t.text3),
            (_, Status::Experimental) => status(text.chip_experimental, t.caution, t.caution),
            _ if view.custom_ids.contains(&e.id) => status(text.added_by_you, t.text3, t.text3),
            _ => return None,
        },
    })
}

impl MainView {
    /// What a row offers to do without opening the mod's page: install or update it. A mod with
    /// options, one installed elsewhere, or one that cannot go in now opens its page instead.
    fn row_action(&self, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let id = e.id.clone();
        if let Some(work) = self.busy.get(&e.id) {
            let label = match work {
                Work::Install => text.working_install,
                Work::Uninstall => text.working_uninstall,
                Work::Setup => text.working_setup,
            };
            return button(("row-busy", hash(&id)), label, Button::Standard, &t).opacity(0.6).cursor_default().into_any_element();
        }
        let options = e.info.as_ref().is_some_and(|i| i.has_choices());
        let direct = e.installable() && !options && self.setup_problem().is_none();
        let (label, kind) = match self.have(e) {
            Have::Outdated(_) => (text.update, Button::Primary),
            Have::Installed(_) | Have::External => return div().into_any_element(),
            Have::Not if direct => (text.install, Button::Standard),
            Have::Not => return div().into_any_element(),
        };
        let direct = direct || kind == Button::Primary;
        button(("row-action", hash(&id)), label, kind, &t)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if direct {
                    this.install(&id, cx);
                } else {
                    this.go(Screen::Mod(id.clone()), cx);
                }
            }))
            .into_any_element()
    }

    fn list_row(&self, i: usize, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let id = e.id.clone();
        let faded = matches!(e.status, Status::Old | Status::Broken);
        let byline = [if e.author.is_empty() { String::new() } else { fill(text.by, &e.author) }, one_line(&e.description)]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let downloads = if e.downloads() > 0 { short_count(e.downloads()) } else { String::new() };
        let hover = t.layer_hover;
        crate::widgets::row(&t)
            .id(("mod-row", i))
            .gap(px(20.))
            .h(px(ROW_H))
            .py(px(0.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |this, _, _, cx| this.go(Screen::Mod(id.clone()), cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .min_w(px(0.))
                            .child(title(e.name.clone(), if faded { t.text2 } else { t.text }).truncate())
                            .children(state_of(self, e, &t, text)),
                    )
                    .child(caption(byline, t.text2).truncate()),
            )
            .child(caption(category_name(e.category, text), t.text3).w(px(120.)).flex_none().truncate())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_end()
                    .gap(px(6.))
                    .w(px(64.))
                    .when(!downloads.is_empty(), |d| d.child(icon(glyph::DOWNLOAD, 14., t.text3)).child(caption(downloads, t.text2))),
            )
            .child(caption(e.version_text().unwrap_or_default(), t.text3).w(px(72.)).flex_none().text_right().truncate())
            .child(div().flex().flex_none().justify_end().w(px(104.)).child(self.row_action(e, cx)))
            .into_any_element()
    }

    fn tile(&self, i: usize, e: &Entry, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let id = e.id.clone();
        let faded = matches!(e.status, Status::Old | Status::Broken);
        let mut facts = vec![];
        if e.downloads() > 0 {
            facts.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon(glyph::DOWNLOAD, 14., t.text3))
                    .child(caption(short_count(e.downloads()), t.text2)),
            );
        }
        if e.stars > 0 {
            facts.push(
                div().flex().items_center().gap(px(6.)).child(icon(glyph::STAR, 14., t.text3)).child(caption(e.stars.to_string(), t.text2)),
            );
        }
        let hover = (t.layer_hover, t.stroke_strong);
        card(&t)
            .id(("tile", i))
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(4.))
            .w(px(width))
            .h(px(TILE_H))
            .p(px(20.))
            .overflow_hidden()
            .cursor_pointer()
            .hover(move |s| s.bg(hover.0).border_color(hover.1))
            .on_click(cx.listener(move |this, _, _, cx| this.go(Screen::Mod(id.clone()), cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(24.))
                    .mb(px(8.))
                    .child(icon(category_icon(e.category), 20., t.text3))
                    .child(div().flex_1())
                    .children(state_of(self, e, &t, text)),
            )
            .child(title(e.name.clone(), if faded { t.text2 } else { t.text }).truncate())
            .child(caption(fill(text.by, &e.author), t.text3).truncate())
            .child(caption(one_line(&e.description), t.text2).line_clamp(2).pt(px(4.)))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .children(facts)
                    .child(div().flex_1())
                    .child(caption(e.version_text().unwrap_or_default(), t.text3).truncate()),
            )
            .into_any_element()
    }

    /// Which of `count` lines, `step` apart from the list's top, are on screen or near it.
    fn visible(&self, count: usize, step: f32, window: &Window) -> (usize, usize) {
        let view = self.scroll.bounds().size.height;
        let height = if view > px(0.) { f32::from(view) } else { f32::from(window.viewport_size().height) };
        let top = -f32::from(self.scroll.offset().y) - self.list_y.get();
        let first = ((top / step).floor().max(0.) as usize).saturating_sub(SPARE).min(count);
        let last = ((((top + height) / step).ceil().max(0.)) as usize + SPARE).min(count);
        (first, last.max(first))
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

    fn empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        div()
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
            .child(title(self.text.nothing_found, t.text))
            .child(body(self.text.nothing_found_body, t.text2))
            .child(
                icon_label_button("empty-add", glyph::ADD, self.text.add_mod, Button::Standard, &t)
                    .mt(px(12.))
                    .on_click(cx.listener(|this, _, window, cx| this.open_adding(window, cx))),
            )
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
        let categories = filters("category", cats, self.category, &t, cx, |this, c, cx| {
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
        let views = icon_segmented(
            "view",
            vec![(View::List, glyph::LIST, text.view_list), (View::Grid, glyph::GRID, text.view_grid)],
            self.view,
            &t,
            cx,
            |this, v, cx| this.set_view(v, cx),
        );
        let shown = self.shown();
        let count = fill(text.mods_count, shown.len());
        let entries: Vec<Entry> = shown.into_iter().cloned().collect();
        let mods = if entries.is_empty() {
            self.empty(cx)
        } else if self.view == View::List {
            // Only the rows on screen are laid out and drawn; the others are room of their height.
            let (first, last) = self.visible(entries.len(), ROW_STEP, window);
            let mut list = crate::widgets::group(&t).child(div().h(px(first as f32 * ROW_STEP)));
            for (i, e) in entries.iter().enumerate().take(last).skip(first) {
                list = list.child(self.list_row(i, e, cx));
            }
            list.child(div().h(px((entries.len() - last) as f32 * ROW_STEP))).into_any_element()
        } else {
            // As many columns as fit, the tiles stretched to fill the row.
            let width = (crate::frame::inner_width(window) - 2. * layout::GUTTER).min(layout::MAX_W - 2. * layout::GUTTER);
            let cols = ((width + GAP) / (TILE_MIN_W + GAP)).floor().max(1.) as usize;
            let tile_w = ((width - GAP * (cols as f32 - 1.)) / cols as f32).floor();
            let rows = entries.len().div_ceil(cols);
            let (first, last) = self.visible(rows, TILE_H + GAP, window);
            let mut grid =
                div().flex().flex_col().gap(px(GAP)).when(first > 0, |d| d.child(div().h(px(first as f32 * (TILE_H + GAP) - GAP))));
            for r in first..last {
                let mut line = div().flex().gap(px(GAP));
                for (i, e) in entries.iter().enumerate().skip(r * cols).take(cols) {
                    line = line.child(self.tile(i, e, tile_w, cx));
                }
                grid = grid.child(line);
            }
            grid.when(last < rows, |d| d.child(div().h(px((rows - last) as f32 * (TILE_H + GAP) - GAP)))).into_any_element()
        };
        // Where the list starts in the scrolled page, for the next frame to know which rows show.
        let mods = {
            let (scroll, at) = (self.scroll.clone(), self.list_y.clone());
            let marker = gpui::canvas(
                move |bounds, window, _| {
                    let y = f32::from(bounds.origin.y - scroll.bounds().origin.y - scroll.offset().y);
                    if (y - at.get()).abs() > 0.5 {
                        at.set(y);
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_0();
            div().relative().child(marker).child(mods).into_any_element()
        };
        let search = div().flex_1().min_w(px(200.)).max_w(px(420.)).child(self.search.clone());
        let add = icon_label_button("add-mod", glyph::ADD, text.add_mod, Button::Standard, &t)
            .on_click(cx.listener(|this, _, window, cx| this.open_adding(window, cx)));
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(12.))
                    .pb(px(24.))
                    .child(display(text.catalog, t.text))
                    .child(caption(count, t.text3).pb(px(8.)))
                    .child(div().flex_1())
                    .child(add),
            )
            .children(banner)
            .child(div().flex().items_center().gap(px(12.)).pb(px(16.)).child(search).child(div().flex_1()).child(sorts).child(views))
            .child(div().pb(px(24.)).child(categories))
            .child(mods)
            .child(caption(text.drop_hint, t.text3).pt(px(24.)))
            .into_any_element()
    }
}

/// A description on one line: some carry line breaks, which would spill out of a row or tile.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An element id from a catalog id.
fn hash(id: &str) -> usize {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish() as usize
}
