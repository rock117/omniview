//! Fluent / Task Manager–inspired light theme tokens for Omniview.

use gpui::Hsla;

fn rgb(r: u8, g: u8, b: u8) -> Hsla {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let (h, s) = if (max - min).abs() < f32::EPSILON {
        (0.0, 0.0)
    } else {
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if (max - r).abs() < f32::EPSILON {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } / 6.0;
        (h, s)
    };
    Hsla { h, s, l, a: 1.0 }
}

/// Window / page background (#F3F3F3)
pub const BG: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.953,
    a: 1.0,
};
/// Left nav (#EBEBEB-ish)
pub const SIDEBAR_BG: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.922,
    a: 1.0,
};
/// Content surface / white
pub const PANEL_BG: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 1.0,
};
pub const ELEVATED: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 1.0,
    a: 1.0,
};
pub const BORDER: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.85,
    a: 1.0,
};
pub const BORDER_SUBTLE: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.90,
    a: 1.0,
};
pub const HOVER: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.94,
    a: 1.0,
};
/// Selected row / nav (#CFE4FA-ish)
pub const SELECTED: Hsla = Hsla {
    h: 0.58,
    s: 0.55,
    l: 0.90,
    a: 1.0,
};
pub const TEXT: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.12,
    a: 1.0,
};
pub const TEXT_MUTED: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.45,
    a: 1.0,
};
/// Windows accent blue #0078D4
pub const ACCENT: Hsla = Hsla {
    h: 0.57,
    s: 0.95,
    l: 0.42,
    a: 1.0,
};
pub const ACCENT_SOFT: Hsla = Hsla {
    h: 0.57,
    s: 0.55,
    l: 0.92,
    a: 1.0,
};
pub const DANGER: Hsla = Hsla {
    h: 0.98,
    s: 0.75,
    l: 0.42,
    a: 1.0,
};
pub const WARN: Hsla = Hsla {
    h: 0.10,
    s: 0.85,
    l: 0.42,
    a: 1.0,
};
pub const OK: Hsla = Hsla {
    h: 0.35,
    s: 0.55,
    l: 0.38,
    a: 1.0,
};

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const NAV_WIDTH: f32 = 168.0;
pub const DETAIL_HEIGHT: f32 = 220.0;
pub const STATUS_HEIGHT: f32 = 28.0;
pub const ROW_HEIGHT: f32 = 30.0;
pub const FONT_UI: &str = "Segoe UI";

/// Process table column widths (must match header + rows).
pub const COL_TREE: f32 = 22.0;
/// Defaults mirrored by `ProcessColumnWidths::default` (runtime widths are resizable).
#[allow(dead_code)]
pub const COL_NAME: f32 = 220.0;
#[allow(dead_code)]
pub const COL_PID: f32 = 72.0;
#[allow(dead_code)]
pub const COL_CPU: f32 = 80.0;
#[allow(dead_code)]
pub const COL_MEM: f32 = 96.0;
#[allow(dead_code)]
pub const COL_NET: f32 = 200.0;
/// Path uses remaining flex space.

/// Soft heat fill for CPU% (Task Manager style).
pub fn cpu_heat(pct: f32) -> Hsla {
    let t = (pct / 100.0).clamp(0.0, 1.0);
    Hsla {
        h: 0.57,
        s: 0.55 * t,
        l: 0.97 - 0.12 * t,
        a: 1.0,
    }
}

/// Soft heat for memory share of a reference max (e.g. top row).
pub fn mem_heat(bytes: u64, peak: u64) -> Hsla {
    let t = if peak == 0 {
        0.0
    } else {
        (bytes as f32 / peak as f32).clamp(0.0, 1.0)
    };
    Hsla {
        h: 0.45,
        s: 0.40 * t,
        l: 0.97 - 0.10 * t,
        a: 1.0,
    }
}

#[allow(dead_code)]
pub fn _rgb_helper(r: u8, g: u8, b: u8) -> Hsla {
    rgb(r, g, b)
}
