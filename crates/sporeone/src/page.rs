//! A mod's page: what it is, its options, what to know before installing it on this game, and
//! the button that installs, updates or removes it.

use crate::markdown;
use crate::theme::{Theme, icon as glyph, radius};
use crate::ui::{Have, MainView, ReadmeState, Screen, Work, category_name, date, issue_text, short_count, size_text};
use crate::widgets::{Kind as Button, body, button, caption, card, check_box, display, icon, icon_label_button, radio, strong, title};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div, px};
use sporeone_core::catalog::{Entry, Kind};
use sporeone_core::compat::{self, Issue};
use sporeone_core::i18n::{Lang, fill};
use sporeone_core::install::{self, Choices};
use sporeone_core::modinfo::Item;
use sporeone_core::readme;
use std::sync::Arc;

fn section(t: &Theme, heading: &'static str) -> Div {
    div().flex().flex_col().gap(px(12.)).child(title(heading, t.text))
}

fn fact(label: &'static str, value: String, t: &Theme) -> Div {
    div().flex().justify_between().gap(px(12.)).child(caption(label, t.text3)).child(caption(value, t.text2).text_right().truncate())
}

impl MainView {
    /// The options as they stand: picked here, or kept from the last install, or the author's.
    pub fn choices_for(&self, e: &Entry) -> Option<Choices> {
        let info = e.info.as_ref().filter(|i| i.has_choices())?;
        if let Some(c) = self.choices.get(&e.id) {
            return Some(c.clone());
        }
        let saved = self.env.lk.as_ref().and_then(|lk| install::saved_choices(&lk.dir, &info.unique));
        Some(saved.unwrap_or_else(|| install::default_choices(info)))
    }

    /// Starts fetching a GitHub mod's README the first time its page opens. `--demo` stays off
    /// the network and shows the copy kept from an earlier run, if there is one.
    fn want_readme(&mut self, e: &Entry, cx: &mut Context<Self>) {
        if !e.page.contains("github.com") || !e.id.contains('/') || self.readmes.contains_key(&e.id) {
            return;
        }
        self.readmes.insert(e.id.clone(), ReadmeState::Loading);
        let (id, name, demo) = (e.id.clone(), e.name.clone(), self.demo);
        cx.spawn(async move |this, cx| {
            let repo = id.clone();
            let doc = cx
                .background_executor()
                .spawn(async move {
                    let text = if demo { readme::cached(&repo) } else { readme::fetch(&repo).ok() }?;
                    let mut doc = markdown::parse(&text, &repo);
                    markdown::drop_title(&mut doc, &[&name, repo.rsplit('/').next().unwrap_or_default()]);
                    (!doc.is_empty()).then_some(doc)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.readmes.insert(id, doc.map_or(ReadmeState::Failed, |d| ReadmeState::Loaded(Arc::new(d))));
                cx.notify();
            });
        })
        .detach();
    }

    /// The README when it has come, or else the catalog's one line.
    fn about(&self, e: &Entry) -> Option<AnyElement> {
        let t = self.theme;
        let content = match self.readmes.get(&e.id) {
            Some(ReadmeState::Loaded(doc)) => markdown::render(doc, &t).into_any_element(),
            _ if e.description.is_empty() => return None,
            _ => body(e.description.clone(), t.text2).into_any_element(),
        };
        Some(section(&t, self.text.about_mod).child(content).into_any_element())
    }

    fn pick(&mut self, e: &Entry, key: String, value: String, cx: &mut Context<Self>) {
        let mut c = self.choices_for(e).unwrap_or_default();
        c.insert(key, value);
        self.choices.insert(e.id.clone(), c);
        cx.notify();
    }

    fn options(&self, e: &Entry, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let info = e.info.as_ref().filter(|i| i.has_choices())?;
        let choices = self.choices_for(e)?;
        let mut list = div().flex().flex_col().gap(px(12.));
        let mut n = 0usize;
        for item in &info.items {
            match item {
                Item::Component(c) => {
                    let on = choices.get(&c.unique).is_some_and(|v| v.eq_ignore_ascii_case("true"));
                    let (entry, key) = (e.clone(), c.unique.clone());
                    n += 1;
                    list = list.child(
                        div()
                            .id(("option", n))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .cursor_pointer()
                            .child(check_box(on, c.display_name.clone(), &t))
                            .children(c.description.clone().map(|d| caption(d, t.text3).pl(px(28.))))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.pick(&entry, key.clone(), if on { "False" } else { "True" }.into(), cx)
                            })),
                    );
                }
                Item::Group { unique, display_name, components } => {
                    let mut group = div().flex().flex_col().gap(px(8.)).child(strong(display_name.clone(), t.text));
                    for c in components {
                        let on = choices.get(unique) == Some(&c.unique);
                        let (entry, key, value) = (e.clone(), unique.clone(), c.unique.clone());
                        n += 1;
                        group = group.child(
                            div()
                                .id(("option", n))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .cursor_pointer()
                                .child(radio(on, c.display_name.clone(), &t))
                                .children(c.description.clone().map(|d| caption(d, t.text3).pl(px(28.))))
                                .on_click(cx.listener(move |this, _, _, cx| this.pick(&entry, key.clone(), value.clone(), cx))),
                        );
                    }
                    list = list.child(group);
                }
                _ => {}
            }
        }
        if n == 0 {
            return None;
        }
        Some(section(&t, self.text.options).child(caption(self.text.options_hint, t.text3)).child(list).into_any_element())
    }

    fn issues(&self, issues: &[Issue]) -> AnyElement {
        let t = self.theme;
        let mut s = section(&t, self.text.before_you_install);
        if issues.is_empty() {
            return s
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(icon(glyph::CHECK, 16., t.success))
                        .child(body(self.text.all_good, t.text)),
                )
                .into_any_element();
        }
        for i in issues {
            let (g, color) = match i {
                i if i.blocks() => (glyph::BLOCK, t.critical),
                Issue::Brings(_) => (glyph::DOWNLOAD, t.accent),
                _ => (glyph::WARNING, t.caution),
            };
            s = s.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .child(icon(g, 16., color).mt(px(2.)))
                    .child(body(issue_text(i, self.text, &self.catalog), t.text).flex_1().min_w(px(0.))),
            );
        }
        s.into_any_element()
    }

    /// The buttons for what can be done with the mod now.
    fn actions(&self, e: &Entry, blocked: bool, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let text = self.text;
        let mut col = div().flex().flex_col().gap(px(8.));
        if let Some(work) = self.busy.get(&e.id) {
            let label = match work {
                Work::Install => text.working_install,
                Work::Uninstall => text.working_uninstall,
                Work::Setup => text.working_setup,
            };
            return col.child(button("working", format!("{label}…"), Button::Standard, &t).w_full().opacity(0.7).cursor_default());
        }
        let wide = |b: gpui::Stateful<Div>| b.w_full();
        let id = e.id.clone();
        match e.kind {
            Kind::Page => {
                let page = e.page.clone();
                col = col.child(
                    wide(icon_label_button("open-page", glyph::OPEN, text.open_page, Button::Primary, &t))
                        .on_click(move |_, _, cx| cx.open_url(&page)),
                );
            }
            Kind::Setup if e.release.is_none() => col = col.child(caption(text.no_download, t.text2)),
            Kind::Setup => {
                let installed = matches!(self.have(e), Have::External);
                let kind = if installed || blocked { Button::Standard } else { Button::Primary };
                col = col.child(
                    wide(icon_label_button("setup", glyph::DOWNLOAD, text.run_setup, kind, &t))
                        .when(blocked, |d| d.opacity(0.5).cursor_default())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !blocked {
                                this.run_setup(&id, cx)
                            }
                        })),
                );
            }
            Kind::Release if e.release.is_none() => col = col.child(caption(text.no_download, t.text2)),
            Kind::Release => match self.have(e) {
                Have::Not | Have::External => {
                    col = col.child(
                        wide(icon_label_button(
                            "install",
                            glyph::DOWNLOAD,
                            text.install,
                            if blocked { Button::Standard } else { Button::Primary },
                            &t,
                        ))
                        .when(blocked, |d| d.opacity(0.5).cursor_default())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !blocked {
                                this.install(&id, cx)
                            }
                        })),
                    );
                }
                Have::Outdated(m) | Have::Installed(m) => {
                    let outdated = matches!(self.have(e), Have::Outdated(_));
                    let name = m.name.clone();
                    if self.confirm.as_deref() == Some(name.as_str()) {
                        return col.child(self.confirm_box(&name, &e.id, cx));
                    }
                    let (label, kind) = if outdated { (text.update, Button::Primary) } else { (text.reinstall, Button::Standard) };
                    let again = id.clone();
                    col = col
                        .when(!blocked || outdated, |d| {
                            d.child(
                                wide(icon_label_button("again", glyph::DOWNLOAD, label, kind, &t))
                                    .on_click(cx.listener(move |this, _, _, cx| this.install(&again, cx))),
                            )
                        })
                        .child(wide(icon_label_button("uninstall", glyph::DELETE, text.uninstall, Button::Standard, &t)).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.confirm = Some(name.clone());
                                cx.notify();
                            }),
                        ));
                }
            },
        }
        col
    }

    /// Asks once more before uninstalling, saying which installed mods need it.
    pub fn confirm_box(&self, name: &str, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let m = self.env.installed.find(name);
        let shown = m.map_or(name.to_string(), |m| m.shown_name().to_string());
        let dependents = m.map(|m| compat::dependents(&m.unique, &self.env.installed)).unwrap_or_default();
        let (n, k) = (name.to_string(), key.to_string());
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .rounded(px(radius::PANEL))
            .bg(t.caution_layer)
            .child(strong(fill(text.confirm_uninstall, shown), t.text))
            .when(!dependents.is_empty(), |d| d.child(caption(fill(text.dependents_warning, dependents.join(", ")), t.text2)))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button("confirm-yes", text.uninstall, Button::Primary, &t)
                            .on_click(cx.listener(move |this, _, _, cx| this.uninstall(&n, &k, cx))),
                    )
                    .child(button("confirm-no", text.cancel, Button::Subtle, &t).on_click(cx.listener(|this, _, _, cx| {
                        this.confirm = None;
                        cx.notify();
                    }))),
            )
            .into_any_element()
    }

    pub fn render_page(&mut self, e: &Entry, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        self.want_readme(e, cx);
        let issues = compat::check(e, &self.catalog, &self.env);
        let blocked = issues.iter().any(Issue::blocks) && e.kind != Kind::Page;
        let back = icon_label_button("back", glyph::BACK, text.catalog, Button::Subtle, &t)
            .on_click(cx.listener(|this, _, _, cx| this.go(Screen::Catalog, cx)));
        let chip = crate::home::state_of(self, e, &t, text);
        let header = div().flex().items_center().gap(px(16.)).pb(px(40.)).child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .flex_1()
                .min_w(px(0.))
                .child(div().flex().items_center().gap(px(16.)).child(display(e.name.clone(), t.text)).children(chip))
                .child(body(format!("{} · {}", fill(text.by, &e.author), category_name(e.category, text)), t.text2)),
        );
        let notes: Vec<String> = e.notes.iter().map(|n| if self.lang == Lang::Pt { n.pt.clone() } else { n.en.clone() }).collect();
        let left = div()
            .flex()
            .flex_col()
            .gap(px(32.))
            .flex_1()
            .min_w(px(0.))
            .children(self.about(e))
            .children(self.options(e, cx))
            .when(!notes.is_empty(), |d| {
                let mut s = section(&t, text.notes);
                for n in notes {
                    s = s.child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(10.))
                            .child(icon(glyph::INFO, 16., t.text2).mt(px(2.)))
                            .child(body(n, t.text).flex_1().min_w(px(0.))),
                    );
                }
                d.child(s)
            })
            .when(e.kind != Kind::Page, |d| d.child(self.issues(&issues)));
        let mut facts = div().flex().flex_col().gap(px(8.));
        if let Some(v) = e.version_text() {
            facts = facts.child(fact(text.version, v, &t));
        }
        if let Some(r) = &e.release {
            if !r.published.is_empty() {
                facts = facts.child(fact(text.released, date(&r.published, self.lang), &t));
            }
            facts = facts.child(fact(text.downloads, short_count(r.downloads), &t));
            if r.size > 0 {
                facts = facts.child(fact(text.size, size_text(r.size), &t));
            }
            facts = facts.child(fact(text.file, r.file.clone(), &t));
        }
        if e.stars > 0 {
            facts = facts.child(fact(text.stars, e.stars.to_string(), &t));
        }
        if !e.needs.stores.is_empty() {
            let stores: Vec<&str> = e.needs.stores.iter().map(|s| crate::ui::store_name(*s, text)).collect();
            facts = facts.child(fact(text.needs_label, stores.join(", "), &t));
        }
        let page = e.page.clone();
        let github = e.page.contains("github.com");
        let mut links = div().flex().flex_wrap().gap(px(8.));
        if !page.is_empty() && (github || e.kind != Kind::Page) {
            links = links.child(
                icon_label_button("link-page", glyph::OPEN, if github { text.open_github } else { text.open_page }, Button::Subtle, &t)
                    .on_click(move |_, _, cx| cx.open_url(&page)),
            );
        }
        if let Some(h) = e.homepage.clone() {
            links = links.child(
                icon_label_button("link-home", glyph::OPEN, text.homepage, Button::Subtle, &t).on_click(move |_, _, cx| cx.open_url(&h)),
            );
        }
        if self.custom_ids.contains(&e.id) {
            let id = e.id.clone();
            links = links.child(
                icon_label_button("remove-custom", glyph::CLOSE, text.remove_custom, Button::Subtle, &t)
                    .on_click(cx.listener(move |this, _, _, cx| this.remove_custom(&id, cx))),
            );
        }
        let right = card(&t)
            .flex()
            .flex_col()
            .gap(px(20.))
            .p(px(24.))
            .rounded(px(radius::SURFACE))
            .w(px(320.))
            .flex_none()
            .child(self.actions(e, blocked, cx))
            .child(facts)
            .child(links);
        div()
            .flex()
            .flex_col()
            .child(div().flex().pb(px(12.)).child(back))
            .child(header)
            .child(div().flex().items_start().gap(px(48.)).child(left).child(right))
            .into_any_element()
    }
}
