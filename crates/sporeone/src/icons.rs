//! The interface's icons, drawn as line art on a 24 by 24 grid, so they look the same on every
//! system (Windows' icon font exists only there). Each is turned into an SVG image in the colour
//! asked for, once, and kept.

use gpui::{Hsla, Image, ImageFormat};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;

fn gear() -> String {
    // Eight teeth around a ring.
    let mut d = String::new();
    let (inner, outer) = (7.2_f32, 9.6_f32);
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        let corners = [(a - 0.24, inner), (a - 0.16, outer), (a + 0.16, outer), (a + 0.24, inner), (a + 0.55, inner)];
        for (j, (b, r)) in corners.iter().enumerate() {
            let (x, y) = (12. + r * b.cos(), 12. + r * b.sin());
            let cmd = if i == 0 && j == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{x:.2} {y:.2}");
        }
    }
    d.push('Z');
    format!(r#"<path d="{d}"/><circle cx="12" cy="12" r="3"/>"#)
}

fn shapes(name: &str) -> String {
    let s = match name {
        "back" => r#"<path d="M19 12H5M11 6l-6 6 6 6"/>"#,
        "chevron-right" => r#"<path d="M9 6l6 6-6 6"/>"#,
        "chevron-down" => r#"<path d="M6 9l6 6 6-6"/>"#,
        "warning" => r#"<path d="M12 3.6 2.8 19.6h18.4Z"/><path d="M12 9.6v4.2M12 16.8v.1"/>"#,
        "delete" => r#"<path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13M10 11v6M14 11v6"/>"#,
        "clock" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3.2 2"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5.6M12 7.6v.1"/>"#,
        "settings" => return gear(),
        "add" => r#"<path d="M12 5v14M5 12h14"/>"#,
        "check" => r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#,
        "close" => r#"<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>"#,
        "minimize" => r#"<path d="M6 12h12"/>"#,
        "maximize" => r#"<rect x="6" y="6" width="12" height="12" rx="1.5"/>"#,
        "restore" => r#"<rect x="5" y="9" width="10" height="10" rx="1.5"/><path d="M9 5h8.5A1.5 1.5 0 0 1 19 6.5V15"/>"#,
        "open" => r#"<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>"#,
        "search" => r#"<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5 20 20"/>"#,
        "download" => r#"<path d="M12 4v11M7 10.5l5 5 5-5M5 20h14"/>"#,
        "play" => r#"<path d="M7.5 4.8v14.4a.8.8 0 0 0 1.2.7l11.3-7.2a.8.8 0 0 0 0-1.4L8.7 4.1a.8.8 0 0 0-1.2.7Z"/>"#,
        "refresh" => r#"<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3M19.5 4.5v4.2h-4.2"/>"#,
        "folder" => {
            r#"<path d="M3.5 7.5A1.5 1.5 0 0 1 5 6h4.2l2 2H19a1.5 1.5 0 0 1 1.5 1.5v8A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5Z"/>"#
        }
        "star" => r#"<path d="M12 3.8l2.5 5.2 5.7.8-4.1 4 1 5.7-5.1-2.7-5.1 2.7 1-5.7-4.1-4 5.7-.8Z"/>"#,
        "block" => r#"<circle cx="12" cy="12" r="8.5"/><path d="M6 18 18 6"/>"#,
        // A cell: a soft outline with a nucleus, as in Spore's first stage.
        "gameplay" => {
            r#"<path d="M12 3.5c4.6 0 8 3.4 8 7.8 0 4.9-3.7 9.2-8.3 9.2S4 16.9 4 12.3C4 7.5 7.3 3.5 12 3.5Z"/><circle cx="13.5" cy="10.5" r="2.5"/><circle cx="8.6" cy="14.6" r=".9"/>"#
        }
        "fixes" => {
            r#"<path d="M14.6 4.4a4.5 4.5 0 0 0-5.5 5.8L4 15.3a2 2 0 0 0 2.8 2.8l5.1-5.1a4.5 4.5 0 0 0 5.8-5.5l-2.6 2.6-2.6-.5-.5-2.6Z"/>"#
        }
        "editors" => r#"<path d="M14.5 4.5 19.5 9.5 9 20H4v-5Z"/><path d="M12.5 6.5l5 5"/>"#,
        "visuals" => {
            r#"<path d="M2.8 12S6.2 5.8 12 5.8 21.2 12 21.2 12 17.8 18.2 12 18.2 2.8 12 2.8 12Z"/><circle cx="12" cy="12" r="3"/>"#
        }
        "multiplayer" => {
            r#"<circle cx="9" cy="8.5" r="3.2"/><path d="M3.5 19.5a5.5 5.5 0 0 1 11 0"/><circle cx="16.5" cy="9.5" r="2.6"/><path d="M15.5 14.6a4.6 4.6 0 0 1 5.5 4.9"/>"#
        }
        "tools" => {
            r#"<rect x="4" y="7" width="16" height="12" rx="1.5"/><path d="M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7M4 12h16"/>"#
        }
        "list" => r#"<path d="M8.5 6.5H20M8.5 12H20M8.5 17.5H20M4 6.5h.5M4 12h.5M4 17.5h.5"/>"#,
        "grid" => {
            r#"<rect x="4" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5"/>"#
        }
        "link" => r#"<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>"#,
        _ => r#"<circle cx="12" cy="12" r="2"/>"#,
    };
    s.to_string()
}

fn svg(name: &str, color: Hsla) -> String {
    let c = color.to_rgb();
    let hex = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="#{:02x}{:02x}{:02x}" stroke-opacity="{:.3}" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
        hex(c.r),
        hex(c.g),
        hex(c.b),
        c.a,
        shapes(name)
    )
}

thread_local! {
    static CACHE: RefCell<HashMap<(&'static str, u32), Arc<Image>>> = RefCell::new(HashMap::new());
}

/// The icon in that colour, as an image to draw at any size.
pub fn image(name: &'static str, color: Hsla) -> Arc<Image> {
    let c = color.to_rgb();
    let key = u32::from_be_bytes([c.r, c.g, c.b, c.a].map(|v| (v.clamp(0., 1.) * 255.).round() as u8));
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry((name, key))
            .or_insert_with(|| Arc::new(Image::from_bytes(ImageFormat::Svg, svg(name, color).into_bytes())))
            .clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_drawn() {
        for name in [
            "back",
            "chevron-right",
            "chevron-down",
            "warning",
            "delete",
            "clock",
            "info",
            "settings",
            "add",
            "check",
            "close",
            "minimize",
            "maximize",
            "restore",
            "open",
            "search",
            "download",
            "play",
            "refresh",
            "folder",
            "star",
            "block",
            "list",
            "grid",
            "link",
            "gameplay",
            "fixes",
            "editors",
            "visuals",
            "multiplayer",
            "tools",
        ] {
            assert!(!shapes(name).contains(r#"r="2"/>"#) || name == "settings", "{name} has no drawing");
        }
        assert!(svg("check", gpui::hsla(0., 0., 1., 1.)).contains("stroke=\"#ffffff\""));
    }
}
