//! Visual tokens for Omniview (original palette — not copied from Zed/Loom).

use gpui::Hsla;

pub const BG: Hsla = Hsla {
    h: 0.55,
    s: 0.06,
    l: 0.12,
    a: 1.0,
};
pub const SIDEBAR_BG: Hsla = Hsla {
    h: 0.55,
    s: 0.07,
    l: 0.10,
    a: 1.0,
};
pub const PANEL_BG: Hsla = Hsla {
    h: 0.55,
    s: 0.05,
    l: 0.15,
    a: 1.0,
};
pub const ELEVATED: Hsla = Hsla {
    h: 0.55,
    s: 0.05,
    l: 0.18,
    a: 1.0,
};
pub const BORDER: Hsla = Hsla {
    h: 0.55,
    s: 0.04,
    l: 0.22,
    a: 1.0,
};
pub const HOVER: Hsla = Hsla {
    h: 0.55,
    s: 0.06,
    l: 0.20,
    a: 1.0,
};
pub const SELECTED: Hsla = Hsla {
    h: 0.52,
    s: 0.28,
    l: 0.22,
    a: 1.0,
};

pub const TEXT: Hsla = Hsla {
    h: 0.50,
    s: 0.04,
    l: 0.90,
    a: 1.0,
};
pub const TEXT_MUTED: Hsla = Hsla {
    h: 0.50,
    s: 0.04,
    l: 0.58,
    a: 1.0,
};

pub const ACCENT: Hsla = Hsla {
    h: 0.48,
    s: 0.42,
    l: 0.52,
    a: 1.0,
};
pub const DANGER: Hsla = Hsla {
    h: 0.02,
    s: 0.55,
    l: 0.52,
    a: 1.0,
};

pub const RADIUS_SM: f32 = 4.0;
pub const FONT_UI: &str = "Segoe UI";
