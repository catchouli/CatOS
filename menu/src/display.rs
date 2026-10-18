//! The screen mode: setting it when the menu starts, and Settings → Resolution,
//! through `wlr-randr`. A mode picked there has to be confirmed within 10
//! seconds, or the old one comes back (in case the screen can't show it).

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, Painter, Pos2};
use serde_json::Value;

use crate::widgets::{self, Row};

const CONFIRM_FOR: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq)]
struct Mode {
    width: u64,
    height: u64,
    /// Hz.
    refresh: f64,
}

impl Mode {
    /// As wlr-randr's --mode takes it.
    fn arg(&self) -> String {
        format!("{}x{}@{:.3}Hz", self.width, self.height, self.refresh)
    }

    fn label(&self) -> String {
        format!("{} × {} at {} Hz", self.width, self.height, (self.refresh * 100.0).round() / 100.0)
    }
}

/// A screen and the modes it offers.
struct Output {
    name: String,
    modes: Vec<Mode>,
    preferred: Option<Mode>,
    current: Option<Mode>,
}

fn outputs() -> Vec<Output> {
    let Ok(out) = Command::new("wlr-randr").arg("--json").output() else { return Vec::new() };
    let Ok(Value::Array(list)) = serde_json::from_slice::<Value>(&out.stdout) else { return Vec::new() };
    list.iter()
        .filter(|o| o["enabled"] == true)
        .map(|o| {
            let mut modes = Vec::new();
            let (mut preferred, mut current) = (None, None);
            for m in o["modes"].as_array().into_iter().flatten() {
                let mode = Mode {
                    width: m["width"].as_u64().unwrap_or(0),
                    height: m["height"].as_u64().unwrap_or(0),
                    refresh: m["refresh"].as_f64().unwrap_or(0.0),
                };
                if m["preferred"] == true {
                    preferred = Some(mode.clone());
                }
                if m["current"] == true {
                    current = Some(mode.clone());
                }
                modes.push(mode);
            }
            Output { name: o["name"].as_str().unwrap_or("").to_owned(), modes, preferred, current }
        })
        .collect()
}

/// The modes worth offering: one per resolution (at least 720 lines), at the
/// refresh rate nearest 60 Hz, biggest first.
fn choices(output: &Output) -> Vec<Mode> {
    let mut best: Vec<Mode> = Vec::new();
    for m in output.modes.iter().filter(|m| m.height >= 720) {
        match best.iter_mut().find(|b| b.width == m.width && b.height == m.height) {
            Some(b) if (m.refresh - 60.0).abs() < (b.refresh - 60.0).abs() => *b = m.clone(),
            Some(_) => {}
            None => best.push(m.clone()),
        }
    }
    best.sort_by(|a, b| (b.width * b.height).cmp(&(a.width * a.height)).then(b.width.cmp(&a.width)));
    best
}

fn set(output: &str, mode: Option<&Mode>) -> bool {
    let mut cmd = Command::new("wlr-randr");
    cmd.args(["--output", output]);
    match mode {
        Some(m) => cmd.args(["--mode", &m.arg()]),
        None => cmd.arg("--preferred"),
    };
    let ok = cmd.status().is_ok_and(|s| s.success());
    if !ok {
        eprintln!("meownix-menu: couldn't set {output} to {}", mode.map_or("its preferred mode".into(), Mode::arg));
    }
    ok
}

/// Where a mode picked in Settings is kept (in /home, so on the stick's
/// settings partition): "auto", or a wlr-randr mode like "3840x2160@60.000Hz".
fn saved_path() -> PathBuf {
    std::env::home_dir().unwrap_or_default().join(".local/state/meownix/screen-mode")
}

/// Sets every screen to the mode picked in Settings, or else `default` (from
/// menu.toml's [display] mode: e.g. "1920x1080", or "auto" to leave the screen's
/// own mode), where the screen offers it. Called when the menu starts.
pub fn set_startup_mode(default: &str) {
    let saved = std::fs::read_to_string(saved_path()).ok().map(|s| s.trim().to_owned());
    let mode = saved.as_deref().unwrap_or(default);
    if mode == "auto" {
        return;
    }
    for output in outputs() {
        let status = Command::new("wlr-randr").args(["--output", &output.name, "--mode", mode]).status();
        if !status.is_ok_and(|s| s.success()) {
            eprintln!("meownix-menu: couldn't set {} to {mode}", output.name);
        }
    }
}

/// What the Resolution item's description says.
pub fn status() -> String {
    match outputs().first().and_then(|o| o.current.clone()) {
        Some(m) => m.label(),
        None => "No screen found".into(),
    }
}

/// The list of modes, and the "keep it?" question, while they're open.
pub struct Menu {
    state: State,
}

enum State {
    List { output: Output, choices: Vec<Option<Mode>>, sel: usize },
    /// Trying `trying`; goes back to `previous` unless kept.
    Confirm { output: String, previous: Option<Mode>, trying: Option<Mode>, until: Instant, keep: bool },
    Message { text: String, until: Instant },
}

impl Menu {
    pub fn open() -> Self {
        let state = match outputs().into_iter().next() {
            Some(output) => {
                // "Auto" (the screen's own preferred mode) first.
                let choices: Vec<Option<Mode>> = std::iter::once(None).chain(choices(&output).into_iter().map(Some)).collect();
                let sel = choices.iter().position(|c| c.is_some() && *c == output.current).unwrap_or(0);
                State::List { output, choices, sel }
            }
            None => State::Message { text: "No screen found".into(), until: Instant::now() + Duration::from_secs(3) },
        };
        Self { state }
    }

    /// Handles keys and the countdown. Returns false once it's closed.
    pub fn update(&mut self, ctx: &egui::Context) -> bool {
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        let back = [Key::Escape, Key::BrowserBack].into_iter().any(pressed);
        match &mut self.state {
            State::List { output, choices, sel } => {
                if back || pressed(Key::ArrowLeft) || pressed(Key::Backspace) {
                    return false;
                }
                if pressed(Key::ArrowUp) && *sel > 0 {
                    *sel -= 1;
                }
                if pressed(Key::ArrowDown) && *sel + 1 < choices.len() {
                    *sel += 1;
                }
                if pressed(Key::Enter) || pressed(Key::Space) {
                    let trying = choices[*sel].clone();
                    let previous = output.current.clone();
                    if trying.is_some() && trying == previous {
                        return false; // already using it
                    }
                    set(&output.name, trying.as_ref());
                    self.state = State::Confirm {
                        output: output.name.clone(),
                        previous,
                        trying,
                        until: Instant::now() + CONFIRM_FOR,
                        keep: false,
                    };
                }
            }
            State::Confirm { output, previous, trying, until, keep } => {
                if pressed(Key::ArrowUp) {
                    *keep = true;
                }
                if pressed(Key::ArrowDown) {
                    *keep = false;
                }
                let timed_out = Instant::now() > *until;
                if pressed(Key::Enter) || pressed(Key::Space) || back || timed_out {
                    let text = if *keep && !back && !timed_out {
                        let saved = trying.as_ref().map_or("auto".into(), Mode::arg);
                        let path = saved_path();
                        let _ = std::fs::create_dir_all(path.parent().unwrap());
                        if let Err(e) = std::fs::write(&path, format!("{saved}\n")) {
                            eprintln!("meownix-menu: couldn't save the screen mode: {e}");
                        }
                        "Kept".to_owned()
                    } else {
                        set(output, previous.as_ref());
                        "Went back".to_owned()
                    };
                    self.state = State::Message { text, until: Instant::now() + Duration::from_secs(2) };
                }
            }
            State::Message { until, .. } => {
                if Instant::now() > *until || back || pressed(Key::Enter) {
                    return false;
                }
            }
        }
        true
    }

    pub fn draw(&self, p: &Painter, at: Pos2, s: f32) {
        match &self.state {
            State::List { output, choices, sel } => {
                let rows: Vec<Row> = choices
                    .iter()
                    .map(|c| match c {
                        None => Row {
                            label: "Auto".into(),
                            detail: Some(output.preferred.as_ref().map_or("the screen's own".into(), Mode::label)),
                            marked: false,
                        },
                        Some(m) => Row { label: m.label(), detail: None, marked: Some(m) == output.current.as_ref() },
                    })
                    .collect();
                widgets::list(p, at, s, &rows, *sel, "No modes found");
            }
            State::Confirm { until, keep, .. } => {
                let left = until.saturating_duration_since(Instant::now()).as_secs() + 1;
                widgets::note(p, at, s, &format!("Keep this resolution? Going back in {left} s"));
                let rows = [
                    Row { label: "Keep it".into(), detail: None, marked: false },
                    Row { label: "Go back".into(), detail: None, marked: false },
                ];
                widgets::list(p, at + egui::vec2(0.0, 52.0 * s), s, &rows, if *keep { 0 } else { 1 }, "");
            }
            State::Message { text, .. } => widgets::note(p, at, s, text),
        }
    }
}
