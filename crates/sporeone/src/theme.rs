//! Colours, type, corners and icons: a neutral base, dark or light as Windows is, with hairline
//! edges and no shadows. The main action is drawn in the text's own colour; the Windows accent
//! marks only what is picked or new, and the colours of state mean state.

use gpui::{Hsla, Rgba, WindowAppearance, rgb, rgba};
use std::sync::atomic::{AtomicU32, Ordering};

/// Counts the accent colour's changes, so what keeps a theme of its own makes it again.
static ACCENTS: AtomicU32 = AtomicU32::new(0);

pub fn accent_changed() {
    ACCENTS.fetch_add(1, Ordering::Relaxed);
}

pub fn accents_seen() -> u32 {
    ACCENTS.load(Ordering::Relaxed)
}

pub const FONT: &str = "Segoe UI Variable Text";
pub const DISPLAY_FONT: &str = "Segoe UI Variable Display";

/// Type ramp, as (size, line height) in pixels.
pub mod text {
    pub const CAPTION: (f32, f32) = (12., 16.);
    pub const BODY: (f32, f32) = (14., 20.);
    pub const TITLE: (f32, f32) = (16., 24.);
    pub const HEADING: (f32, f32) = (22., 28.);
    pub const DISPLAY: (f32, f32) = (28., 36.);
}

/// How wide the content may grow, and the gutter around it. The bar on top lines up with it.
pub mod layout {
    pub const MAX_W: f32 = 1120.;
    pub const SETTINGS_W: f32 = 760.;
    pub const GUTTER: f32 = 32.;
}

/// Corner radii, concentric: a control is round at its ends, and what holds controls is rounder by
/// its padding.
pub mod radius {
    pub const SURFACE: f32 = 28.;
    pub const PANEL: f32 = 20.;
    pub const CONTROL: f32 = 18.;
    pub const SMALL: f32 = 12.;
}

/// The icons, by name (see `crate::icons`).
pub mod icon {
    pub const BACK: &str = "back";
    pub const CHEVRON_RIGHT: &str = "chevron-right";
    pub const WARNING: &str = "warning";
    pub const DELETE: &str = "delete";
    pub const INFO: &str = "info";
    pub const SETTINGS: &str = "settings";
    pub const CHECK: &str = "check";
    pub const CLOSE: &str = "close";
    pub const MINIMIZE: &str = "minimize";
    pub const MAXIMIZE: &str = "maximize";
    pub const RESTORE: &str = "restore";
    pub const OPEN: &str = "open";
    pub const SEARCH: &str = "search";
    pub const DOWNLOAD: &str = "download";
    pub const PLAY: &str = "play";
    pub const REFRESH: &str = "refresh";
    pub const FOLDER: &str = "folder";
    pub const STAR: &str = "star";
    pub const BLOCK: &str = "block";
    pub const GAMEPLAY: &str = "gameplay";
    pub const FIXES: &str = "fixes";
    pub const EDITORS: &str = "editors";
    pub const VISUALS: &str = "visuals";
    pub const MULTIPLAYER: &str = "multiplayer";
    pub const TOOLS: &str = "tools";
    pub const LIST: &str = "list";
    pub const GRID: &str = "grid";
    pub const ADD: &str = "add";
    pub const LINK: &str = "link";
}

#[derive(Clone, Copy)]
pub struct Theme {
    /// The page.
    pub base: Hsla,
    /// A panel or card.
    pub layer: Hsla,
    pub layer_hover: Hsla,
    /// A control: button, field, the picked choice of a row of them.
    pub control: Hsla,
    pub control_hover: Hsla,
    pub stroke: Hsla,
    pub stroke_strong: Hsla,
    pub text: Hsla,
    pub text2: Hsla,
    pub text3: Hsla,
    /// The main action on a screen.
    pub primary: Hsla,
    pub primary_hover: Hsla,
    pub on_primary: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
    pub critical: Hsla,
    pub success: Hsla,
    pub caution: Hsla,
    pub caution_layer: Hsla,
    /// Behind a dialog.
    pub scrim: Hsla,
}

fn c(v: u32) -> Hsla {
    rgb(v).into()
}

fn ca(v: u32) -> Hsla {
    rgba(v).into()
}

impl Theme {
    pub fn new(appearance: WindowAppearance) -> Theme {
        let dark = matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark);
        let accent = windows_accent(dark).unwrap_or_else(|| if dark { rgb(0x60cdff) } else { rgb(0x005fb8) });
        let accent: Hsla = accent.into();
        if dark {
            Theme {
                base: c(0x0e0e10),
                layer: c(0x151517),
                layer_hover: c(0x1a1a1d),
                control: c(0x1f1f23),
                control_hover: c(0x28282d),
                stroke: ca(0xffffff12),
                stroke_strong: ca(0xffffff21),
                text: c(0xf4f4f5),
                text2: ca(0xffffffa6),
                text3: ca(0xffffff6b),
                primary: c(0xf4f4f5),
                primary_hover: c(0xd9d9dc),
                on_primary: c(0x0e0e10),
                accent,
                accent_soft: accent.opacity(0.16),
                critical: c(0xff99a4),
                success: c(0x6ccb5f),
                caution: c(0xf2c94c),
                caution_layer: c(0x2a2518),
                scrim: ca(0x00000099),
            }
        } else {
            Theme {
                base: c(0xf7f7f8),
                layer: c(0xffffff),
                layer_hover: c(0xfafafa),
                control: c(0xf0f0f2),
                control_hover: c(0xe6e6e9),
                stroke: ca(0x00000014),
                stroke_strong: ca(0x00000024),
                text: c(0x18181b),
                text2: ca(0x0000009c),
                text3: ca(0x0000006b),
                primary: c(0x18181b),
                primary_hover: c(0x3a3a40),
                on_primary: c(0xffffff),
                accent,
                accent_soft: accent.opacity(0.12),
                critical: c(0xc42b1c),
                success: c(0x0f7b0f),
                caution: c(0x9d5d00),
                caution_layer: c(0xfdf6e3),
                scrim: ca(0x00000052),
            }
        }
    }
}

/// The Windows accent colour in the shade Windows itself uses on this background (a lighter one
/// on dark, a darker one on light), or `None` when it is a grey that would not read as an
/// accent: the toggles and highlights would disappear.
#[cfg(not(windows))]
fn windows_accent(_: bool) -> Option<Rgba> {
    None
}

#[cfg(windows)]
fn windows_accent(dark: bool) -> Option<Rgba> {
    let palette = registry_binary(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent", "AccentPalette")?;
    // Eight RGBA entries, lightest to darkest; the accent itself is the fourth.
    let i = if dark { 1 } else { 4 };
    let px = palette.get(i * 4..i * 4 + 3)?;
    let (r, g, b) = (px[0], px[1], px[2]);
    let chroma = r.max(g).max(b) - r.min(g).min(b);
    (chroma >= 48).then(|| rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)))
}

#[cfg(windows)]
fn registry_binary(key: &str, value: &str) -> Option<Vec<u8>> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RegGetValueW};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let mut buf = vec![0u8; 64];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            wide(value).as_ptr(),
            RRF_RT_REG_BINARY,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    (ok == 0).then(|| {
        buf.truncate(len as usize);
        buf
    })
}
