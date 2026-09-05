use gpui::prelude::*;
use gpui::*;

use crate::shared::theme;

#[derive(IntoElement)]
pub struct ChipButton {
    id: ElementId,
    label: SharedString,
    active: bool,
    on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
}

impl ChipButton {
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

impl RenderOnce for ChipButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click;
        div()
            .id(self.id)
            .px_3()
            .py_1()
            .rounded(px(theme::RADIUS_SM))
            .bg(if self.active {
                theme::SELECTED
            } else {
                theme::PANEL_BG
            })
            .border_1()
            .border_color(theme::BORDER)
            .text_color(theme::TEXT)
            .text_sm()
            .cursor_pointer()
            .hover(|s| s.bg(theme::HOVER))
            .child(self.label)
            .on_click(move |e, w, cx| on_click(e, w, cx))
    }
}

#[derive(IntoElement)]
pub struct ToolButton {
    id: ElementId,
    label: SharedString,
    danger: bool,
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
            on_click: Box::new(on_click),
        }
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

impl RenderOnce for ToolButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click;
        div()
            .id(self.id)
            .px_2()
            .py_1()
            .rounded(px(theme::RADIUS_SM))
            .bg(theme::ELEVATED)
            .border_1()
            .border_color(theme::BORDER)
            .text_color(if self.danger {
                theme::DANGER
            } else {
                theme::TEXT
            })
            .text_sm()
            .cursor_pointer()
            .hover(|s| s.bg(theme::HOVER))
            .child(self.label)
            .on_click(move |e, w, cx| on_click(e, w, cx))
    }
}
