//! The building blocks every screen uses: text styles, panels of rows, buttons, rows of choices,
//! the check box and the round choice.

use crate::theme::{DISPLAY_FONT, FONT, Theme, radius, text};
use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};

pub fn caption(s: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_size(px(text::CAPTION.0)).line_height(px(text::CAPTION.1)).text_color(color).child(s.into())
}

pub fn body(s: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_size(px(text::BODY.0)).line_height(px(text::BODY.1)).text_color(color).child(s.into())
}

pub fn strong(s: impl Into<SharedString>, color: Hsla) -> Div {
    body(s, color).font_weight(FontWeight::MEDIUM)
}

pub fn title(s: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_size(px(text::TITLE.0)).line_height(px(text::TITLE.1)).font_weight(FontWeight::MEDIUM).text_color(color).child(s.into())
}

pub fn heading(s: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .font_family(DISPLAY_FONT)
        .text_size(px(text::HEADING.0))
        .line_height(px(text::HEADING.1))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .child(s.into())
}

pub fn display(s: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .font_family(DISPLAY_FONT)
        .text_size(px(text::DISPLAY.0))
        .line_height(px(text::DISPLAY.1))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .child(s.into())
}

pub fn icon(name: &'static str, size: f32, color: Hsla) -> Div {
    div().flex_none().size(px(size)).child(gpui::img(crate::icons::image(name, color)).size(px(size)))
}

/// Explains more when the pointer rests on `el`.
pub fn with_tip(el: Stateful<Div>, tip: &'static str, t: &Theme) -> Stateful<Div> {
    let t = *t;
    el.tooltip(move |_, cx| cx.new(|_| Tip { text: tip, theme: t }).into())
}

struct Tip {
    text: &'static str,
    theme: Theme,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .rounded(px(radius::SMALL))
            .bg(self.theme.control)
            .border_1()
            .border_color(self.theme.stroke_strong)
            .shadow_md()
            .font_family(FONT)
            .max_w(px(320.))
            .px(px(12.))
            .py(px(8.))
            .child(caption(self.text, self.theme.text))
    }
}

/// A section title above a panel.
pub fn section(s: impl Into<SharedString>, t: &Theme) -> Div {
    title(s, t.text).pt(px(32.)).pb(px(12.))
}

/// A card: a panel's surface with a hairline edge, and no shadow.
pub fn card(t: &Theme) -> Div {
    div().rounded(px(radius::PANEL)).bg(t.layer).border_1().border_color(t.stroke)
}

/// One row of a panel, apart from the one above by a hairline.
pub fn row(t: &Theme) -> Div {
    div().flex().items_center().gap(px(16.)).px(px(20.)).py(px(12.)).min_h(px(60.)).mt(px(-1.)).border_t_1().border_color(t.stroke)
}

/// A panel of rows. Each row draws the line above it, and the first one's falls on the panel's
/// own edge.
pub fn group(t: &Theme) -> Div {
    card(t).flex().flex_col().overflow_hidden()
}

/// A dot before a word of state: installed, an update.
pub fn status(s: impl Into<SharedString>, dot: Hsla, color: Hsla) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .child(div().flex_none().size(px(6.)).rounded_full().bg(dot))
        .child(caption(s, color))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Standard,
    Subtle,
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: Kind, t: &Theme) -> Stateful<Div> {
    button_base(id, kind, t).child(label.into())
}

/// A button with an icon before its label.
pub fn icon_label_button(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    kind: Kind,
    t: &Theme,
) -> Stateful<Div> {
    let fg = if kind == Kind::Primary { t.on_primary } else { t.text };
    button_base(id, kind, t).pl(px(14.)).child(icon(glyph, 16., fg)).child(label.into())
}

fn button_base(id: impl Into<ElementId>, kind: Kind, t: &Theme) -> Stateful<Div> {
    let (bg, hover, fg, border) = match kind {
        Kind::Primary => (t.primary, t.primary_hover, t.on_primary, t.primary),
        Kind::Standard => (t.layer, t.control, t.text, t.stroke_strong),
        Kind::Subtle => (gpui::transparent_black(), t.control, t.text, gpui::transparent_black()),
    };
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .h(px(36.))
        .px(px(16.))
        .rounded(px(radius::CONTROL))
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_size(px(text::BODY.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .font_family(FONT)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
}

/// A button holding only an icon.
pub fn icon_button(id: impl Into<ElementId>, glyph: &'static str, t: &Theme) -> Stateful<Div> {
    let hover = t.control;
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(36.))
        .rounded_full()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(glyph, 18., t.text2))
}

/// A row of choices, one of them picked.
pub fn segmented<T: Copy + PartialEq + 'static, V: 'static>(
    id: &'static str,
    choices: Vec<(T, SharedString)>,
    picked: T,
    t: &Theme,
    cx: &mut Context<V>,
    on_pick: impl Fn(&mut V, T, &mut Context<V>) + 'static,
) -> Div {
    let on_pick = std::rc::Rc::new(on_pick);
    let mut d = div().flex().flex_none().gap(px(2.)).p(px(3.)).rounded_full().bg(t.control);
    for (i, (value, label)) in choices.into_iter().enumerate() {
        let on = value == picked;
        let on_pick = on_pick.clone();
        d = d.child(
            div()
                .id((id, i))
                .flex()
                .items_center()
                .justify_center()
                .min_w(px(56.))
                .h(px(30.))
                .px(px(12.))
                .rounded_full()
                .cursor_pointer()
                .when(on, |d| d.bg(t.layer).border_1().border_color(t.stroke_strong))
                .child(caption(label, if on { t.text } else { t.text2 }).font_weight(FontWeight::MEDIUM))
                .on_click(cx.listener(move |this, _, _, cx| on_pick(this, value, cx))),
        );
    }
    d
}

/// Two to five ways of showing the same thing, as icons in a pill.
pub fn icon_segmented<T: Copy + PartialEq + 'static, V: 'static>(
    id: &'static str,
    choices: Vec<(T, &'static str, &'static str)>,
    picked: T,
    t: &Theme,
    cx: &mut Context<V>,
    on_pick: impl Fn(&mut V, T, &mut Context<V>) + 'static,
) -> Div {
    let on_pick = std::rc::Rc::new(on_pick);
    let mut d = div().flex().flex_none().gap(px(2.)).p(px(3.)).rounded_full().bg(t.control);
    for (i, (value, glyph, tip)) in choices.into_iter().enumerate() {
        let on = value == picked;
        let on_pick = on_pick.clone();
        let el = div()
            .id((id, i))
            .flex()
            .items_center()
            .justify_center()
            .w(px(36.))
            .h(px(30.))
            .rounded_full()
            .cursor_pointer()
            .when(on, |d| d.bg(t.layer).border_1().border_color(t.stroke_strong))
            .child(icon(glyph, 16., if on { t.text } else { t.text3 }))
            .on_click(cx.listener(move |this, _, _, cx| on_pick(this, value, cx)));
        d = d.child(with_tip(el, tip, t));
    }
    d
}

/// Choices that narrow a list, as pills: the picked one filled.
pub fn filters<T: Copy + PartialEq + 'static, V: 'static>(
    id: &'static str,
    choices: Vec<(T, SharedString)>,
    picked: T,
    t: &Theme,
    cx: &mut Context<V>,
    on_pick: impl Fn(&mut V, T, &mut Context<V>) + 'static,
) -> Div {
    let on_pick = std::rc::Rc::new(on_pick);
    let mut d = div().flex().flex_wrap().gap(px(8.));
    for (i, (value, label)) in choices.into_iter().enumerate() {
        let on = value == picked;
        let on_pick = on_pick.clone();
        let hover = t.control;
        d = d.child(
            div()
                .id((id, i))
                .flex()
                .items_center()
                .h(px(32.))
                .px(px(14.))
                .rounded_full()
                .border_1()
                .cursor_pointer()
                .when(on, |d| d.bg(t.primary).border_color(t.primary))
                .when(!on, |d| d.border_color(t.stroke_strong).hover(move |s| s.bg(hover)))
                .child(caption(label, if on { t.on_primary } else { t.text2 }).font_weight(FontWeight::MEDIUM))
                .on_click(cx.listener(move |this, _, _, cx| on_pick(this, value, cx))),
        );
    }
    d
}

/// A check box with its label, for a mod's options.
pub fn check_box(on: bool, label: impl Into<SharedString>, t: &Theme) -> Div {
    let mark = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(18.))
        .rounded(px(6.))
        .when(on, |d| d.bg(t.primary).child(icon(crate::theme::icon::CHECK, 12., t.on_primary)))
        .when(!on, |d| d.border_1().border_color(t.text3));
    div().flex().items_center().gap(px(10.)).child(mark).child(body(label, t.text))
}

/// A round choice with its label, for one option of a group.
pub fn radio(on: bool, label: impl Into<SharedString>, t: &Theme) -> Div {
    let ring = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(18.))
        .rounded_full()
        .border_1()
        .border_color(if on { t.primary } else { t.text3 })
        .when(on, |d| d.child(div().size(px(8.)).rounded_full().bg(t.primary)));
    div().flex().items_center().gap(px(10.)).child(ring).child(body(label, t.text))
}
