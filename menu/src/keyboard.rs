//! An on-screen keyboard for typing with the remote: the arrows move around it
//! and OK presses the highlighted key. A real keyboard types into it directly.

use eframe::egui::{self, Align2, Color32, Event, FontId, Key, Painter, Rect, pos2, vec2};

#[derive(Clone, Copy, PartialEq)]
enum K {
    Char(char),
    Shift,
    Symbols,
    Space,
    Delete,
    Done,
}

impl K {
    /// Width, in ordinary keys.
    fn width(self) -> f32 {
        match self {
            K::Char(_) => 1.0,
            K::Space => 4.0,
            _ => 2.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Layer {
    Lower,
    Upper,
    Symbols,
}

pub enum Outcome {
    Typing,
    Done(String),
    Cancel,
}

pub struct Keyboard {
    prompt: String,
    text: String,
    layer: Layer,
    row: usize,
    col: usize,
}

const KEY_W: f32 = 80.0;
const KEY_H: f32 = 68.0;
const GAP: f32 = 10.0;

fn white(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
}

impl Keyboard {
    pub fn new(prompt: String) -> Self {
        Self { prompt, text: String::new(), layer: Layer::Lower, row: 1, col: 0 }
    }

    fn rows(&self) -> Vec<Vec<K>> {
        let chars: &[&str] = match self.layer {
            Layer::Lower => &["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"],
            Layer::Upper => &["1234567890", "QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
            Layer::Symbols => &["1234567890", "!@#$%^&*()", "-_=+[]{}\\|", ";:'\",.<>/?", "`~"],
        };
        let mut rows: Vec<Vec<K>> = chars.iter().map(|r| r.chars().map(K::Char).collect()).collect();
        rows.push(vec![K::Shift, K::Symbols, K::Space, K::Delete, K::Done]);
        rows
    }

    /// Each key's centre, in key widths from the middle of its row.
    fn centres(row: &[K]) -> Vec<f32> {
        let total: f32 = row.iter().map(|k| k.width()).sum::<f32>() + (row.len() - 1) as f32 * GAP / KEY_W;
        let mut x = -total / 2.0;
        row.iter()
            .map(|k| {
                let c = x + k.width() / 2.0;
                x += k.width() + GAP / KEY_W;
                c
            })
            .collect()
    }

    pub fn update(&mut self, ctx: &egui::Context) -> Outcome {
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        if [Key::Escape, Key::BrowserBack].into_iter().any(pressed) {
            return Outcome::Cancel;
        }
        // A real keyboard: letters arrive as text, plus Backspace.
        let typed: String = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| if let Event::Text(t) = e { Some(t.clone()) } else { None })
                .collect()
        });
        self.text.extend(typed.chars().filter(|c| !c.is_control()));
        if pressed(Key::Backspace) {
            self.text.pop();
        }

        let rows = self.rows();
        let to = if pressed(Key::ArrowUp) && self.row > 0 {
            Some(self.row - 1)
        } else if pressed(Key::ArrowDown) && self.row + 1 < rows.len() {
            Some(self.row + 1)
        } else {
            None
        };
        if let Some(to) = to {
            // The key in the new row nearest to the old one.
            let centre = Self::centres(&rows[self.row])[self.col];
            let centres = Self::centres(&rows[to]);
            self.col = (0..centres.len())
                .min_by(|&a, &b| (centres[a] - centre).abs().total_cmp(&(centres[b] - centre).abs()))
                .unwrap_or(0);
            self.row = to;
        }
        if pressed(Key::ArrowLeft) && self.col > 0 {
            self.col -= 1;
        }
        if pressed(Key::ArrowRight) && self.col + 1 < rows[self.row].len() {
            self.col += 1;
        }

        if pressed(Key::Enter) {
            match rows[self.row][self.col] {
                K::Char(c) => self.text.push(c),
                K::Space => self.text.push(' '),
                K::Delete => {
                    self.text.pop();
                }
                K::Shift => self.set_layer(if self.layer == Layer::Lower { Layer::Upper } else { Layer::Lower }),
                K::Symbols => self.set_layer(if self.layer == Layer::Symbols { Layer::Lower } else { Layer::Symbols }),
                K::Done => return Outcome::Done(self.text.clone()),
            }
        }
        Outcome::Typing
    }

    /// Switches layer, keeping the highlight on the same key (Shift or #+=).
    fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
        self.row = self.rows().len() - 1;
    }

    pub fn draw(&self, p: &Painter, screen: Rect, s: f32) {
        p.rect_filled(screen, 0.0, Color32::from_black_alpha(250));
        let rows = self.rows();
        let unit = (KEY_W + GAP) * s;
        let top = screen.center().y - (rows.len() as f32 * (KEY_H + GAP) * s) / 2.0 + 60.0 * s;
        let cx = screen.center().x;

        // Prompt, and what's been typed so far, with a cursor.
        p.text(pos2(cx, top - 150.0 * s), Align2::CENTER_CENTER, &self.prompt, FontId::proportional(32.0 * s), white(220));
        let field = Rect::from_center_size(pos2(cx, top - 80.0 * s), vec2(900.0 * s, 64.0 * s));
        p.rect_filled(field, 10.0 * s, white(24));
        p.text(
            pos2(field.left() + 20.0 * s, field.center().y),
            Align2::LEFT_CENTER,
            format!("{}|", self.text),
            FontId::proportional(32.0 * s),
            white(255),
        );

        for (r, row) in rows.iter().enumerate() {
            let y = top + r as f32 * (KEY_H + GAP) * s;
            for (c, (&key, centre)) in row.iter().zip(Self::centres(row)).enumerate() {
                let w = key.width() * KEY_W * s + (key.width() - 1.0) * GAP * s;
                let rect = Rect::from_center_size(pos2(cx + centre * unit, y), vec2(w, KEY_H * s));
                let selected = r == self.row && c == self.col;
                let on = (key == K::Shift && self.layer == Layer::Upper) || (key == K::Symbols && self.layer == Layer::Symbols);
                p.rect_filled(rect, 8.0 * s, white(if selected { 230 } else if on { 70 } else { 30 }));
                let label = match key {
                    K::Char(ch) => ch.to_string(),
                    K::Shift => "Shift".into(),
                    K::Symbols => if self.layer == Layer::Symbols { "abc" } else { "#+=" }.into(),
                    K::Space => "Space".into(),
                    K::Delete => "Delete".into(),
                    K::Done => "Done".into(),
                };
                let colour = if selected { Color32::from_rgb(20, 20, 40) } else { white(230) };
                let size = if matches!(key, K::Char(_)) { 30.0 } else { 24.0 };
                p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(size * s), colour);
            }
        }
    }
}
