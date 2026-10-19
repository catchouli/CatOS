//! System → Install to drive: puts CatOS on the computer's own drive, or
//! updates the drive it's on, through `catos-install` (run as root with sudo).
//! Only from a USB stick: CatOS running from the drive can't replace itself.
//!
//! Installing erases the whole drive, so it lists what's on it first and needs
//! OK held down. Updating only replaces the system partition, and finds the
//! drive by itself, so there's nothing to pick.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, Painter, Pos2, vec2};
use serde_json::Value;

use crate::widgets::{self, Row};

/// How long OK has to be held to erase a drive.
const HOLD_TO_ERASE: Duration = Duration::from_secs(3);

#[derive(Clone, Debug)]
struct Partition {
    size: u64,
    fstype: String,
    label: String,
}

#[derive(Clone, Debug)]
struct Drive {
    path: String,
    model: String,
    size: u64,
    /// Has CatOS on it already.
    installed: bool,
    partitions: Vec<Partition>,
}

impl Drive {
    fn name(&self) -> String {
        let model = if self.model.is_empty() { self.path.trim_start_matches("/dev/") } else { &self.model };
        format!("{model} ({})", size(self.size))
    }
}

struct Drives {
    from_usb: bool,
    drives: Vec<Drive>,
}

/// What the installer found, or why it couldn't look.
fn list() -> Result<Drives, String> {
    let out = Command::new("sudo")
        .args(["-n", "/usr/local/sbin/catos-install", "list"])
        .output()
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| error_text(&out.stdout, &out.stderr))?;
    let s = |v: &Value, k: &str| v[k].as_str().unwrap_or_default().to_owned();
    let drives = v["drives"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| Drive {
            path: s(d, "path"),
            model: s(d, "model"),
            size: d["size"].as_u64().unwrap_or(0),
            installed: d["installed"].as_bool().unwrap_or(false),
            partitions: d["partitions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| Partition { size: p["size"].as_u64().unwrap_or(0), fstype: s(p, "fstype"), label: s(p, "label") })
                .collect(),
        })
        .collect();
    Ok(Drives { from_usb: v["from_usb"].as_bool().unwrap_or(false), drives })
}

fn error_text(stdout: &[u8], stderr: &[u8]) -> String {
    let out = String::from_utf8_lossy(stdout);
    let err = String::from_utf8_lossy(stderr);
    let line = out.lines().chain(err.lines()).rev().find(|l| !l.trim().is_empty()).unwrap_or("it didn't say why");
    line.trim_start_matches("error: ").to_owned()
}

/// "64 GB", the way drives are sold.
fn size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 { format!("{:.1} TB", gb / 1000.0) } else if gb >= 10.0 { format!("{gb:.0} GB") } else { format!("{gb:.1} GB") }
}

/// A partition, for the list of what installing erases.
fn describe(p: &Partition) -> String {
    let what = match (p.label.as_str(), p.fstype.as_str()) {
        ("", "") => "Unformatted".to_owned(),
        ("", fs) => fs.to_uppercase(),
        (label, "") => label.to_owned(),
        (label, fs) => format!("{label} · {}", fs.to_uppercase()),
    };
    format!("{what} · {}", size(p.size))
}

/// A line from the installer while it works, or how it ended.
enum Progress {
    Step(String),
    Done(Result<(), String>),
}

/// Runs `catos-install <args>` in the background, passing on each step it prints.
fn run(args: Vec<String>) -> Receiver<Progress> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        eprintln!("catos-menu: catos-install {}", args.join(" "));
        let child = Command::new("sudo")
            .args(["-n", "/usr/local/sbin/catos-install"])
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Progress::Done(Err(e.to_string())));
                return;
            }
        };
        let mut error = None;
        for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
            eprintln!("catos-menu: catos-install: {line}");
            if let Some(e) = line.strip_prefix("error: ") {
                error = Some(e.to_owned());
            } else if line != "done" {
                let _ = tx.send(Progress::Step(line));
            }
        }
        let ok = child.wait().is_ok_and(|s| s.success());
        let result = match (ok, error) {
            (true, _) => Ok(()),
            (false, Some(e)) => Err(e),
            (false, None) => Err("it stopped part way".into()),
        };
        let _ = tx.send(Progress::Done(result));
    });
    rx
}

/// A row in the list of what can be done.
#[derive(Clone)]
enum Choice {
    Update(Drive),
    Install(Drive),
}

/// The list, confirmation and progress under the item, while it's open.
pub struct Menu {
    state: State,
}

enum State {
    Looking(Receiver<Result<Drives, String>>),
    Choose { choices: Vec<Choice>, sel: usize },
    /// Installing erases `drive`: OK has to be held down.
    ConfirmInstall { drive: Drive, held_since: Option<Instant> },
    ConfirmUpdate { drive: Drive, sel: usize },
    Working { what: String, step: String, progress: Receiver<Progress> },
    /// Stays until Back or OK.
    Message(String),
}

impl Menu {
    pub fn open() -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(list());
        });
        Self { state: State::Looking(rx) }
    }

    /// Handles keys and background work. Returns false once it's closed.
    pub fn update(&mut self, ctx: &egui::Context) -> bool {
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        let back = [Key::Escape, Key::BrowserBack].into_iter().any(pressed);
        let ok = pressed(Key::Enter) || pressed(Key::Space);
        let leave = back || pressed(Key::ArrowLeft) || pressed(Key::Backspace);
        match &mut self.state {
            State::Looking(rx) => {
                if back {
                    return false;
                }
                if let Ok(result) = rx.try_recv() {
                    self.state = match result {
                        Err(e) => State::Message(format!("Couldn't look for drives: {e}")),
                        Ok(d) if !d.from_usb => State::Message(
                            "To install or update, start CatOS from a USB stick"
                                .into(),
                        ),
                        Ok(d) if d.drives.is_empty() => State::Message("There's no drive to install CatOS on".into()),
                        Ok(d) => {
                            // Updating first: it's what's wanted once CatOS is installed.
                            let mut choices: Vec<Choice> =
                                d.drives.iter().filter(|d| d.installed).cloned().map(Choice::Update).collect();
                            choices.extend(d.drives.into_iter().map(Choice::Install));
                            State::Choose { choices, sel: 0 }
                        }
                    };
                }
            }
            State::Choose { choices, sel } => {
                if leave {
                    return false;
                }
                if pressed(Key::ArrowUp) && *sel > 0 {
                    *sel -= 1;
                }
                if pressed(Key::ArrowDown) && *sel + 1 < choices.len() {
                    *sel += 1;
                }
                if ok {
                    self.state = match choices[*sel].clone() {
                        Choice::Update(drive) => State::ConfirmUpdate { drive, sel: 0 },
                        Choice::Install(drive) => State::ConfirmInstall { drive, held_since: None },
                    };
                }
            }
            State::ConfirmInstall { drive, held_since } => {
                if leave {
                    return false;
                }
                // The press that opened this doesn't count, even held: OK has to
                // be pressed again (not a key repeat), then held.
                let fresh = ctx.input(|i| {
                    i.events.iter().any(|e| {
                        matches!(e, egui::Event::Key { key: Key::Enter | Key::Space, pressed: true, repeat: false, .. })
                    })
                });
                let down = ctx.input(|i| i.key_down(Key::Enter) || i.key_down(Key::Space));
                if !down {
                    *held_since = None;
                } else if fresh && held_since.is_none() {
                    *held_since = Some(Instant::now());
                }
                if held_since.is_some_and(|t| t.elapsed() >= HOLD_TO_ERASE) {
                    let what = format!("Installing CatOS on {}", drive.name());
                    let progress = run(vec!["install".into(), drive.path.clone()]);
                    self.state = State::Working { what, step: String::new(), progress };
                }
            }
            State::ConfirmUpdate { drive, sel } => {
                if leave {
                    return false;
                }
                if pressed(Key::ArrowUp) && *sel > 0 {
                    *sel -= 1;
                }
                if pressed(Key::ArrowDown) && *sel < 1 {
                    *sel += 1;
                }
                if ok {
                    if *sel == 1 {
                        return false;
                    }
                    let what = format!("Updating CatOS on {}", drive.name());
                    self.state = State::Working { what, step: String::new(), progress: run(vec!["update".into()]) };
                }
            }
            // Not stoppable: half an install is worse than a whole one.
            State::Working { what, step, progress } => {
                while let Ok(p) = progress.try_recv() {
                    match p {
                        Progress::Step(s) => *step = s,
                        Progress::Done(result) => {
                            let installing = what.starts_with("Installing");
                            self.state = State::Message(match result {
                                Ok(()) if installing => {
                                    "Installed. Take the stick out and restart to use it"
                                        .into()
                                }
                                Ok(()) => "Updated. Take the stick out and restart to use it".into(),
                                Err(e) => format!("That didn't work: {e}"),
                            });
                            return true;
                        }
                    }
                }
            }
            State::Message(_) => {
                if back || ok || pressed(Key::ArrowLeft) {
                    return false;
                }
            }
        }
        true
    }

    /// Draws under the item, from `at`.
    pub fn draw(&self, p: &Painter, at: Pos2, s: f32, t: f64) {
        let dots = ".".repeat((t * 3.0) as usize % 4);
        let note = |at: Pos2, text: &str| widgets::note(p, at, s, text);
        let line = |n: f32| at + vec2(0.0, n * 52.0 * s);
        match &self.state {
            State::Looking(_) => note(at, &format!("Looking for drives{dots}")),
            State::Choose { choices, sel } => {
                let rows: Vec<Row> = choices
                    .iter()
                    .map(|c| match c {
                        Choice::Update(d) => Row {
                            label: format!("Update CatOS on {}", d.name()),
                            detail: Some("keeps your settings".into()),
                            marked: false,
                        },
                        Choice::Install(d) => Row {
                            label: format!("Install on {}", d.name()),
                            detail: Some("erases it".into()),
                            marked: false,
                        },
                    })
                    .collect();
                widgets::list(p, at, s, &rows, *sel, "");
            }
            State::ConfirmInstall { drive, held_since } => {
                note(at, &format!("Everything on {} will be erased:", drive.name()));
                let mut n = 1.0;
                if drive.partitions.is_empty() {
                    note(line(n), "    (nothing: it's empty)");
                    n += 1.0;
                }
                for part in &drive.partitions {
                    note(line(n), &format!("    {}", describe(part)));
                    n += 1.0;
                }
                let prompt = match held_since {
                    Some(t) => {
                        let left = HOLD_TO_ERASE.saturating_sub(t.elapsed()).as_secs_f32().ceil();
                        format!("Keep holding OK… {left:.0}")
                    }
                    None => "Hold OK to erase it and install, or Back to cancel".into(),
                };
                note(line(n + 0.5), &prompt);
            }
            State::ConfirmUpdate { drive, sel } => {
                note(at, &format!("Update CatOS on {}? Your settings and apps stay.", drive.name()));
                let rows = ["Update", "Cancel"].map(|l| Row { label: l.into(), detail: None, marked: false });
                widgets::list(p, line(1.0), s, &rows, *sel, "");
            }
            State::Working { what, step, .. } => {
                note(at, &format!("{what}{dots}"));
                if !step.is_empty() {
                    note(line(1.0), &capitalise(step));
                }
                note(line(2.0), "Leave it on until it's done");
            }
            State::Message(text) => note(at, text),
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_drive_labels() {
        assert_eq!(size(64_000_000_000), "64 GB");
        assert_eq!(size(256_060_514_304), "256 GB");
        assert_eq!(size(2_000_398_934_016), "2.0 TB");
        assert_eq!(size(536_870_912), "0.5 GB");
    }

    #[test]
    fn partitions_say_what_they_are() {
        let p = |fstype: &str, label: &str| Partition { size: 63_900_000_000, fstype: fstype.into(), label: label.into() };
        assert_eq!(describe(&p("ntfs", "Windows")), "Windows · NTFS · 64 GB");
        assert_eq!(describe(&p("vfat", "")), "VFAT · 64 GB");
        assert_eq!(describe(&p("", "")), "Unformatted · 64 GB");
    }
}
