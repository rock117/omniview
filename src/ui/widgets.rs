use gpui::prelude::*;
use gpui::*;

use crate::shared::theme;

#[derive(IntoElement)]
pub struct NavItem {
    id: ElementId,
    label: SharedString,
    active: bool,
    on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
}

impl NavItem {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            active,
            on_click: Box::new(on_click),
        }
    }
}

impl RenderOnce for NavItem {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click;
        div()
            .id(self.id)
            .w_full()
            .h(px(36.))
            .px_3()
            .mb_1()
            .rounded(px(theme::RADIUS_SM))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .bg(if self.active {
                theme::SELECTED
            } else {
                theme::SIDEBAR_BG
            })
            .hover(|s| {
                if self.active {
                    s
                } else {
                    s.bg(theme::HOVER)
                }
            })
            .child(
                div()
                    .w(px(3.))
                    .h(px(16.))
                    .rounded(px(2.))
                    .bg(if self.active {
                        theme::ACCENT
                    } else {
                        theme::SIDEBAR_BG
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(if self.active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if self.active {
                        theme::ACCENT
                    } else {
                        theme::TEXT
                    })
                    .child(self.label),
            )
            .on_click(move |e, w, cx| on_click(e, w, cx))
    }
}

#[derive(IntoElement)]
pub struct ToolButton {
    id: ElementId,
    label: SharedString,
    danger: bool,
    primary: bool,
    on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
}

impl ToolButton {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            danger: false,
            primary: false,
            on_click: Box::new(on_click),
        }
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
}

impl RenderOnce for ToolButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click;
        let (bg, fg, border) = if self.danger {
            (theme::PANEL_BG, theme::DANGER, theme::BORDER)
        } else if self.primary {
            (theme::ACCENT, theme::PANEL_BG, theme::ACCENT)
        } else {
            (theme::PANEL_BG, theme::TEXT, theme::BORDER)
        };
        div()
            .id(self.id)
            .px_3()
            .py_1()
            .rounded(px(theme::RADIUS_SM))
            .bg(bg)
            .border_1()
            .border_color(border)
            .text_color(fg)
            .text_sm()
            .cursor_pointer()
            .hover(|s| s.bg(theme::HOVER))
            .child(self.label)
            .on_click(move |e, w, cx| on_click(e, w, cx))
    }
}

#[derive(IntoElement)]
pub struct SegmentedButton {
    id: ElementId,
    label: SharedString,
    active: bool,
    on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
}

impl SegmentedButton {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            active,
            on_click: Box::new(on_click),
        }
    }
}

impl RenderOnce for SegmentedButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click;
        div()
            .id(self.id)
            .px_3()
            .py_1()
            .text_sm()
            .cursor_pointer()
            .bg(if self.active {
                theme::ACCENT_SOFT
            } else {
                theme::PANEL_BG
            })
            .text_color(if self.active {
                theme::ACCENT
            } else {
                theme::TEXT_MUTED
            })
            .border_1()
            .border_color(theme::BORDER)
            .child(self.label)
            .on_click(move |e, w, cx| on_click(e, w, cx))
    }
}

/// Keep ChipButton as thin alias for segmented / sort chips during migration.
pub type ChipButton = SegmentedButton;
