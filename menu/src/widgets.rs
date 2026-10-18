//! The lists that open under a Settings item (Sound, Wi-Fi).

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, pos2, vec2};

pub struct Row {
    pub label: String,
    /// Dimmer text after the label.
    pub detail: Option<String>,
    /// Gets a dot: what's in use now.
    pub marked: bool,
}

const ROW_H: f32 = 52.0;

fn white(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
}

/// Draws `rows` from `at` (the left edge of the first row's text, at its middle)
/// downwards, with `sel` highlighted. `s` is the menu's scale (1 at 1080p).
pub fn list(p: &Painter, at: Pos2, s: f32, rows: &[Row], sel: usize, empty: &str) {
    if rows.is_empty() {
        return note(p, at, s, empty);
    }
    let font = FontId::proportional(28.0 * s);
    let small = FontId::proportional(22.0 * s);
    for (k, row) in rows.iter().enumerate() {
        let y = at.y + k as f32 * ROW_H * s;
        let selected = k == sel;
        let label = p.layout_no_wrap(row.label.clone(), font.clone(), white(if selected { 255 } else { 140 }));
        let mut width = label.size().x;
        let detail = row.detail.as_ref().map(|d| p.layout_no_wrap(d.clone(), small.clone(), white(if selected { 170 } else { 90 })));
        if let Some(d) = &detail {
            width += 18.0 * s + d.size().x;
        }
        if selected {
            let bar = Rect::from_min_size(pos2(at.x - 16.0 * s, y - 22.0 * s), vec2(width + 32.0 * s, 44.0 * s));
            p.rect_filled(bar, 8.0 * s, white(28));
        }
        if row.marked {
            p.circle_filled(pos2(at.x - 34.0 * s, y), 6.0 * s, white(220));
        }
        let label_w = label.size().x;
        p.galley(pos2(at.x, y - label.size().y / 2.0), label, white(255));
        if let Some(d) = detail {
            p.galley(pos2(at.x + label_w + 18.0 * s, y - d.size().y / 2.0), d, white(255));
        }
    }
}

/// A line of text where a list would go ("Looking for networks...").
pub fn note(p: &Painter, at: Pos2, s: f32, text: &str) {
    p.text(at, Align2::LEFT_CENTER, text, FontId::proportional(28.0 * s), white(170));
}
