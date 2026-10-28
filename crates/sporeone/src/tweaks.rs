//! The game's tweaks: settings of Spore itself that players otherwise change by hand in its
//! `Config` files, each turned on and off with a switch.

use crate::ui::MainView;
use crate::widgets::{body, caption, display, group, row, strong, switch};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div, px};
use sporeone_core::tweaks;

impl MainView {
    pub fn render_tweaks(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let text = self.text;
        let shown = [(tweaks::SKIP_INTRO.id, text.tweak_skip_intro)];
        let game = self.env.game().cloned();
        let mut list = group(&t);
        match &game {
            None => list = list.child(row(&t).child(body(text.game_none, t.text2).flex_1().min_w(px(0.)))),
            Some(g) if tweaks::config_dirs(g).is_empty() => {
                list = list.child(row(&t).child(body(text.tweaks_no_config, t.text2).flex_1().min_w(px(0.))))
            }
            Some(g) => {
                for (i, (id, name)) in shown.into_iter().enumerate() {
                    let on = tweaks::by_id(id).is_some_and(|tw| tweaks::is_on(tw, g));
                    let busy = self.busy.contains_key(&format!("tweak:{id}"));
                    list = list.child(
                        row(&t)
                            .child(strong(name, t.text).flex_1().min_w(px(0.)))
                            .when(busy, |d| d.child(caption(text.tweak_working, t.text3)))
                            .child(switch(("tweak", i), on, &t).when(busy, |d| d.opacity(0.5)).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if !busy {
                                        this.set_tweak(id, name, !on, cx);
                                    }
                                },
                            ))),
                    );
                }
            }
        }
        div()
            .flex()
            .flex_col()
            .child(display(text.tweaks, t.text))
            .child(body(text.tweaks_body, t.text2).pt(px(4.)).pb(px(24.)))
            .child(list)
            .into_any_element()
    }
}
