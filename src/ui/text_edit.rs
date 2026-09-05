//! Single-line text edit with selection (copy / cut / paste / select-all).

use gpui::prelude::*;
use gpui::*;

use crate::shared::theme;

#[derive(Clone, Debug, Default)]
pub struct TextEdit {
    pub text: String,
    /// Cursor in Unicode scalar values (0..=char_len).
    pub cursor: usize,
    /// Selection anchor; range is `min(anchor, cursor)..max(anchor, cursor)`.
    pub anchor: usize,
}

impl TextEdit {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.chars().count();
        Self {
            text,
            cursor: len,
            anchor: len,
        }
    }

    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn sel_range(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.cursor
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.char_len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = 0;
    }

    pub fn selected_text(&self) -> String {
        let (lo, hi) = self.sel_range();
        self.text.chars().skip(lo).take(hi - lo).collect()
    }

    pub fn delete_selection(&mut self) -> bool {
        let (lo, hi) = self.sel_range();
        if lo == hi {
            return false;
        }
        let before: String = self.text.chars().take(lo).collect();
        let after: String = self.text.chars().skip(hi).collect();
        self.text = before + &after;
        self.cursor = lo;
        self.anchor = lo;
        true
    }

    pub fn insert(&mut self, typed: &str) {
        self.delete_selection();
        let before: String = self.text.chars().take(self.cursor).collect();
        let after: String = self.text.chars().skip(self.cursor).collect();
        let insert_len = typed.chars().count();
        self.text = before + typed + &after;
        self.cursor += insert_len;
        self.anchor = self.cursor;
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let before: String = self.text.chars().take(self.cursor - 1).collect();
        let after: String = self.text.chars().skip(self.cursor).collect();
        self.text = before + &after;
        self.cursor -= 1;
        self.anchor = self.cursor;
    }

    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor >= self.char_len() {
            return;
        }
        let before: String = self.text.chars().take(self.cursor).collect();
        let after: String = self.text.chars().skip(self.cursor + 1).collect();
        self.text = before + &after;
        self.anchor = self.cursor;
    }

    pub fn move_left(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            let (lo, _) = self.sel_range();
            self.cursor = lo;
            self.anchor = lo;
            return;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
        }
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            let (_, hi) = self.sel_range();
            self.cursor = hi;
            self.anchor = hi;
            return;
        }
        if self.cursor < self.char_len() {
            self.cursor += 1;
        }
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub fn move_home(&mut self, extend: bool) {
        self.cursor = 0;
        if !extend {
            self.anchor = 0;
        }
    }

    pub fn move_end(&mut self, extend: bool) {
        self.cursor = self.char_len();
        if !extend {
            self.anchor = self.cursor;
        }
    }

    /// Replace text when it changes; keep caret if unchanged.
    pub fn set_text_if_changed(&mut self, text: impl Into<String>) {
        let text = text.into();
        if self.text == text {
            return;
        }
        self.text = text;
        let len = self.char_len();
        self.cursor = len;
        self.anchor = len;
    }

    /// Select / copy / navigate only (no mutate). Returns true if consumed.
    pub fn handle_key_readonly(&mut self, event: &KeyDownEvent, cx: &mut App) -> bool {
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;
        let chord = mods.control || mods.platform;
        let shift = mods.shift;

        if chord && key.eq_ignore_ascii_case("a") {
            self.select_all();
            return true;
        }
        if chord && (key.eq_ignore_ascii_case("c") || key.eq_ignore_ascii_case("x")) {
            let text = if self.has_selection() {
                self.selected_text()
            } else {
                self.text.clone()
            };
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            return true;
        }
        if key == "left" {
            self.move_left(shift);
            return true;
        }
        if key == "right" {
            self.move_right(shift);
            return true;
        }
        if key == "home" {
            self.move_home(shift);
            return true;
        }
        if key == "end" {
            self.move_end(shift);
            return true;
        }
        // Block edit keys on read-only fields.
        if chord && key.eq_ignore_ascii_case("v") {
            return true;
        }
        if key == "backspace" || key == "delete" {
            return true;
        }
        false
    }

    /// Handle clipboard / navigation keys. Returns true if consumed.
    pub fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut App) -> bool {
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;
        let chord = mods.control || mods.platform;
        let shift = mods.shift;

        if chord && key.eq_ignore_ascii_case("a") {
            self.select_all();
            return true;
        }
        if chord && key.eq_ignore_ascii_case("c") {
            let text = if self.has_selection() {
                self.selected_text()
            } else {
                self.text.clone()
            };
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            return true;
        }
        if chord && key.eq_ignore_ascii_case("x") {
            let text = if self.has_selection() {
                self.selected_text()
            } else {
                self.text.clone()
            };
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            if self.has_selection() {
                self.delete_selection();
            } else {
                self.clear();
            }
            return true;
        }
        if (chord && key.eq_ignore_ascii_case("v"))
            || (mods.shift && key.eq_ignore_ascii_case("insert"))
        {
            if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                let cleaned = text.replace('\r', "").replace('\n', "");
                if !cleaned.is_empty() {
                    self.insert(&cleaned);
                }
            }
            return true;
        }
        if key == "backspace" {
            self.backspace();
            return true;
        }
        if key == "delete" {
            self.delete_forward();
            return true;
        }
        if key == "left" {
            self.move_left(shift);
            return true;
        }
        if key == "right" {
            self.move_right(shift);
            return true;
        }
        if key == "home" {
            self.move_home(shift);
            return true;
        }
        if key == "end" {
            self.move_end(shift);
            return true;
        }
        false
    }

    /// Printable character from keystroke, if any.
    pub fn typed_from_keystroke(keystroke: &Keystroke) -> Option<String> {
        if keystroke.key.as_str() == "space" {
            return Some(" ".to_string());
        }
        keystroke
            .key_char
            .as_ref()
            .map(|s| s.replace('\r', "").replace('\n', ""))
            .filter(|s| !s.is_empty())
    }

    /// Render text with selection highlight and caret (when focused).
    pub fn render_content(&self, focused: bool, placeholder: &str) -> AnyElement {
        if self.text.is_empty() && !focused {
            return div()
                .text_color(theme::TEXT_MUTED)
                .child(placeholder.to_string())
                .into_any_element();
        }
        if self.text.is_empty() && focused {
            return div()
                .flex()
                .items_center()
                .child(caret_el())
                .child(
                    div()
                        .text_color(theme::TEXT_MUTED)
                        .child(placeholder.to_string()),
                )
                .into_any_element();
        }

        let (lo, hi) = self.sel_range();
        let cursor = self.cursor.min(self.char_len());
        let before_sel: String = self.text.chars().take(lo).collect();
        let selected: String = self.text.chars().skip(lo).take(hi - lo).collect();
        let after_sel: String = self.text.chars().skip(hi).collect();

        let sel_el = |s: String| {
            div()
                .bg(theme::ACCENT_SOFT)
                .rounded(px(2.0))
                .child(s)
        };

        let mut row = div()
            .flex()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(theme::TEXT);

        if !focused || lo == hi {
            let before: String = self.text.chars().take(cursor).collect();
            let after: String = self.text.chars().skip(cursor).collect();
            row = row.child(div().child(before));
            if focused {
                row = row.child(caret_el());
            }
            row = row.child(div().overflow_hidden().child(after));
        } else if cursor <= lo {
            let before_c: String = before_sel.chars().take(cursor).collect();
            let mid: String = before_sel.chars().skip(cursor).collect();
            row = row
                .child(div().child(before_c))
                .child(caret_el())
                .child(div().child(mid))
                .child(sel_el(selected))
                .child(div().overflow_hidden().child(after_sel));
        } else if cursor >= hi {
            let after_before: String = after_sel.chars().take(cursor - hi).collect();
            let after_after: String = after_sel.chars().skip(cursor - hi).collect();
            row = row
                .child(div().child(before_sel))
                .child(sel_el(selected))
                .child(div().child(after_before))
                .child(caret_el())
                .child(div().overflow_hidden().child(after_after));
        } else {
            let sel_before: String = selected.chars().take(cursor - lo).collect();
            let sel_after: String = selected.chars().skip(cursor - lo).collect();
            row = row
                .child(div().child(before_sel))
                .child(sel_el(sel_before))
                .child(caret_el())
                .child(sel_el(sel_after))
                .child(div().overflow_hidden().child(after_sel));
        }
        row.into_any_element()
    }
}

fn caret_el() -> impl IntoElement {
    div()
        .w(px(1.0))
        .h(px(14.0))
        .flex_shrink_0()
        .bg(theme::TEXT)
}
