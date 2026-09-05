//! Single-line text edit: selection, clipboard, caret blink, mouse hit-test, IME.

use std::ops::Range;

use gpui::prelude::*;
use gpui::*;

use crate::shared::theme;

/// Approximate average glyph width for `text-sm` hit-testing (ASCII-biased).
pub const AVG_CHAR_W: f32 = 8.0;

#[derive(Clone, Debug, Default)]
pub struct TextEdit {
    pub text: String,
    /// Cursor in Unicode scalar values (0..=char_len).
    pub cursor: usize,
    /// Selection anchor; range is `min(anchor, cursor)..max(anchor, cursor)`.
    pub anchor: usize,
    pub caret_visible: bool,
    /// IME preedit range in Unicode scalar indices.
    pub marked: Option<(usize, usize)>,
}

impl TextEdit {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.chars().count();
        Self {
            text,
            cursor: len,
            anchor: len,
            caret_visible: true,
            marked: None,
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
        self.caret_visible = true;
        self.marked = None;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = 0;
        self.caret_visible = true;
        self.marked = None;
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
        self.caret_visible = true;
        self.marked = None;
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
        self.caret_visible = true;
        self.marked = None;
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
        self.caret_visible = true;
        self.marked = None;
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
        self.caret_visible = true;
        self.marked = None;
    }

    pub fn move_left(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            let (lo, _) = self.sel_range();
            self.cursor = lo;
            self.anchor = lo;
            self.caret_visible = true;
            return;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
        }
        if !extend {
            self.anchor = self.cursor;
        }
        self.caret_visible = true;
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            let (_, hi) = self.sel_range();
            self.cursor = hi;
            self.anchor = hi;
            self.caret_visible = true;
            return;
        }
        if self.cursor < self.char_len() {
            self.cursor += 1;
        }
        if !extend {
            self.anchor = self.cursor;
        }
        self.caret_visible = true;
    }

    pub fn move_home(&mut self, extend: bool) {
        self.cursor = 0;
        if !extend {
            self.anchor = 0;
        }
        self.caret_visible = true;
    }

    pub fn move_end(&mut self, extend: bool) {
        self.cursor = self.char_len();
        if !extend {
            self.anchor = self.cursor;
        }
        self.caret_visible = true;
    }

    pub fn set_caret(&mut self, index: usize, extend: bool) {
        let i = index.min(self.char_len());
        self.cursor = i;
        if !extend {
            self.anchor = i;
        }
        self.caret_visible = true;
        self.marked = None;
    }

    pub fn char_index_at_x(&self, local_x: f32) -> usize {
        if local_x <= 0.0 {
            return 0;
        }
        let idx = (local_x / AVG_CHAR_W).round() as usize;
        idx.min(self.char_len())
    }

    pub fn set_text_if_changed(&mut self, text: impl Into<String>) {
        let text = text.into();
        if self.text == text {
            return;
        }
        self.text = text;
        let len = self.char_len();
        self.cursor = len;
        self.anchor = len;
        self.marked = None;
    }

    pub fn char_to_utf16(text: &str, char_idx: usize) -> usize {
        text.chars().take(char_idx).map(|c| c.len_utf16()).sum()
    }

    pub fn utf16_to_char(text: &str, utf16_idx: usize) -> usize {
        let mut u16s = 0usize;
        for (i, ch) in text.chars().enumerate() {
            if u16s >= utf16_idx {
                return i;
            }
            u16s += ch.len_utf16();
        }
        text.chars().count()
    }

    #[allow(dead_code)]
    pub fn utf16_len(text: &str) -> usize {
        text.chars().map(|c| c.len_utf16()).sum()
    }

    pub fn selection_utf16(&self) -> Range<usize> {
        let (lo, hi) = self.sel_range();
        Self::char_to_utf16(&self.text, lo)..Self::char_to_utf16(&self.text, hi)
    }

    /// Replace a UTF-16 range (or selection / marked) with `new_text` (IME commit / WM_CHAR).
    pub fn replace_utf16_range(&mut self, range_utf16: Option<Range<usize>>, new_text: &str) {
        let range = range_utf16
            .map(|r| {
                Self::utf16_to_char(&self.text, r.start)..Self::utf16_to_char(&self.text, r.end)
            })
            .or_else(|| self.marked.map(|(a, b)| a..b))
            .unwrap_or_else(|| {
                let (lo, hi) = self.sel_range();
                lo..hi
            });
        let before: String = self.text.chars().take(range.start).collect();
        let after: String = self.text.chars().skip(range.end).collect();
        let cleaned = new_text.replace('\r', "").replace('\n', "");
        let insert_len = cleaned.chars().count();
        self.text = before + &cleaned + &after;
        self.cursor = range.start + insert_len;
        self.anchor = self.cursor;
        self.marked = None;
        self.caret_visible = true;
    }

    /// IME preedit: replace range and mark the inserted text.
    pub fn replace_and_mark_utf16(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_utf16: Option<Range<usize>>,
    ) {
        let range = range_utf16
            .map(|r| {
                Self::utf16_to_char(&self.text, r.start)..Self::utf16_to_char(&self.text, r.end)
            })
            .or_else(|| self.marked.map(|(a, b)| a..b))
            .unwrap_or_else(|| {
                let (lo, hi) = self.sel_range();
                lo..hi
            });
        let before: String = self.text.chars().take(range.start).collect();
        let after: String = self.text.chars().skip(range.end).collect();
        let cleaned = new_text.replace('\r', "").replace('\n', "");
        let insert_len = cleaned.chars().count();
        self.text = before + &cleaned + &after;
        if cleaned.is_empty() {
            self.marked = None;
        } else {
            self.marked = Some((range.start, range.start + insert_len));
        }
        if let Some(sel) = new_selected_utf16 {
            // Selection is relative to the new marked text in UTF-16 (GPUI convention varies);
            // map within inserted segment when possible.
            let rel_start = Self::utf16_to_char(&cleaned, sel.start);
            let rel_end = Self::utf16_to_char(&cleaned, sel.end);
            self.cursor = range.start + rel_end.min(insert_len);
            self.anchor = range.start + rel_start.min(insert_len);
        } else {
            self.cursor = range.start + insert_len;
            self.anchor = self.cursor;
        }
        self.caret_visible = true;
    }

    pub fn unmark(&mut self) {
        self.marked = None;
    }

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
        if chord && key.eq_ignore_ascii_case("v") {
            return true;
        }
        if key == "backspace" || key == "delete" {
            return true;
        }
        false
    }

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

    /// Render text with selection, IME mark, and blinking caret.
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
                .child(caret_el(self.caret_visible))
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
        let marked_el = |s: String| {
            div()
                .border_b_1()
                .border_color(theme::ACCENT)
                .child(s)
        };

        let mut row = div()
            .flex()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(theme::TEXT);

        // Prefer showing IME marked range when present.
        if focused {
            if let Some((mlo, mhi)) = self.marked {
                let mlo = mlo.min(self.char_len());
                let mhi = mhi.min(self.char_len()).max(mlo);
                let before: String = self.text.chars().take(mlo).collect();
                let marked: String = self.text.chars().skip(mlo).take(mhi - mlo).collect();
                let after: String = self.text.chars().skip(mhi).collect();
                return row
                    .child(div().child(before))
                    .child(marked_el(marked))
                    .child(caret_el(self.caret_visible))
                    .child(div().overflow_hidden().child(after))
                    .into_any_element();
            }
        }

        if !focused || lo == hi {
            let before: String = self.text.chars().take(cursor).collect();
            let after: String = self.text.chars().skip(cursor).collect();
            row = row.child(div().child(before));
            if focused {
                row = row.child(caret_el(self.caret_visible));
            }
            row = row.child(div().overflow_hidden().child(after));
        } else if cursor <= lo {
            let before_c: String = before_sel.chars().take(cursor).collect();
            let mid: String = before_sel.chars().skip(cursor).collect();
            row = row
                .child(div().child(before_c))
                .child(caret_el(self.caret_visible))
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
                .child(caret_el(self.caret_visible))
                .child(div().overflow_hidden().child(after_after));
        } else {
            let sel_before: String = selected.chars().take(cursor - lo).collect();
            let sel_after: String = selected.chars().skip(cursor - lo).collect();
            row = row
                .child(div().child(before_sel))
                .child(sel_el(sel_before))
                .child(caret_el(self.caret_visible))
                .child(sel_el(sel_after))
                .child(div().overflow_hidden().child(after_sel));
        }
        row.into_any_element()
    }
}

fn caret_el(visible: bool) -> impl IntoElement {
    div()
        .w(px(1.0))
        .h(px(14.0))
        .flex_shrink_0()
        .bg(if visible {
            theme::TEXT
        } else {
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.0,
            }
        })
}
