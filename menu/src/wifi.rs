//! Settings → Wi-Fi: picking a network, typing its password with the on-screen
//! keyboard, and connecting, through NetworkManager's `nmcli`. NetworkManager
//! keeps the networks it has joined (and their passwords), so after the first
//! time a network connects by itself.

use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, Painter, Pos2, Rect};

use crate::keyboard::{self, Keyboard};
use crate::widgets::{self, Row};

#[derive(Clone, Debug)]
pub struct Network {
    pub ssid: String,
    /// 0 to 100.
    pub signal: u8,
    pub secure: bool,
    /// NetworkManager has a saved connection for it.
    pub known: bool,
    pub in_use: bool,
    /// In range now. (Saved networks are listed even when they aren't, so they
    /// can be forgotten.)
    pub nearby: bool,
}

/// What can be done with a saved network.
#[derive(Clone, Copy)]
enum Action {
    Connect,
    Disconnect,
    Forget,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Connect => "Connect",
            Action::Disconnect => "Disconnect",
            Action::Forget => "Forget this network",
        }
    }
}

/// What the Wi-Fi item's description says.
pub fn status() -> String {
    let Some(out) = nmcli(&["-t", "-f", "TYPE,STATE,CONNECTION", "device"]) else {
        return "Wi-Fi isn't available".into();
    };
    let devices: Vec<Vec<String>> = out.lines().map(fields).filter(|f| f.first().is_some_and(|t| t == "wifi")).collect();
    if devices.is_empty() {
        return "No Wi-Fi adapter found".into();
    }
    match devices.iter().find(|f| f.get(1).is_some_and(|s| s == "connected")) {
        Some(f) => format!("Connected to {}", f.get(2).map(String::as_str).unwrap_or("Wi-Fi")),
        None => "Not connected".into(),
    }
}

/// Nearby networks, strongest first, one entry per name, then saved ones that
/// aren't in range.
fn scan() -> Vec<Network> {
    let _ = nmcli(&["radio", "wifi", "on"]);
    let known: Vec<String> = nmcli(&["-t", "-f", "NAME,TYPE", "connection", "show"])
        .unwrap_or_default()
        .lines()
        .map(fields)
        .filter(|f| f.get(1).is_some_and(|t| t == "802-11-wireless"))
        .filter_map(|f| f.into_iter().next())
        .collect();
    let out = nmcli(&["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "yes"])
        .unwrap_or_default();
    let mut networks: Vec<Network> = Vec::new();
    for f in out.lines().map(fields) {
        let [in_use, ssid, signal, security] = &f[..] else { continue };
        if ssid.is_empty() {
            continue; // hidden network
        }
        let network = Network {
            ssid: ssid.clone(),
            signal: signal.parse().unwrap_or(0),
            secure: !security.is_empty() && security != "--",
            known: known.contains(ssid),
            in_use: in_use == "*",
            nearby: true,
        };
        match networks.iter_mut().find(|n| n.ssid == network.ssid) {
            // Several access points with one name: keep the strongest.
            Some(n) => {
                n.in_use |= network.in_use;
                if network.signal > n.signal {
                    n.signal = network.signal;
                }
            }
            None => networks.push(network),
        }
    }
    networks.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    for ssid in known {
        if !networks.iter().any(|n| n.ssid == ssid) {
            networks.push(Network { ssid, signal: 0, secure: true, known: true, in_use: false, nearby: false });
        }
    }
    networks
}

/// Disconnects the Wi-Fi, which then stays off until a network is picked again.
fn disconnect() -> Result<String, String> {
    let devices = nmcli(&["-t", "-f", "DEVICE,TYPE", "device"]).unwrap_or_default();
    for f in devices.lines().map(fields) {
        if f.get(1).is_some_and(|t| t == "wifi") {
            run_nmcli(&["device", "disconnect", &f[0]])?;
        }
    }
    Ok("Disconnected".into())
}

/// Deletes a saved network, and its password.
fn forget(ssid: &str) -> Result<String, String> {
    run_nmcli(&["connection", "delete", "id", ssid]).map(|()| format!("Forgot {ssid}"))
}

/// Why connecting failed.
enum Failure {
    /// The password is missing or wrong.
    Password,
    Other(String),
}

fn connect(network: &Network, password: Option<&str>) -> Result<(), Failure> {
    let ssid = network.ssid.as_str();
    let result = match password {
        None if network.known => run_nmcli(&["connection", "up", "id", ssid]),
        None => run_nmcli(&["device", "wifi", "connect", ssid]),
        Some(password) => {
            if network.known {
                // Start afresh rather than keep the saved, wrong password.
                let _ = run_nmcli(&["connection", "delete", "id", ssid]);
            }
            run_nmcli(&["device", "wifi", "connect", ssid, "password", password])
        }
    };
    result.map_err(|e| {
        if e.contains("Secrets were required") || e.contains("password") || e.contains("802-11-wireless-security") {
            Failure::Password
        } else {
            Failure::Other(e)
        }
    })
}

fn nmcli(args: &[&str]) -> Option<String> {
    let out = Command::new("nmcli").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs nmcli, returning its error message if it fails.
fn run_nmcli(args: &[&str]) -> Result<(), String> {
    let out = Command::new("nmcli").args(["--wait", "30"]).args(args).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    eprintln!("meownix-menu: nmcli {}: {}", args.first().unwrap_or(&""), err.trim());
    Err(err.trim().trim_start_matches("Error: ").to_owned())
}

/// Splits a line of `nmcli -t` output: fields are separated by ':', with ':' and
/// '\' inside them escaped by a backslash.
fn fields(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.last_mut().unwrap().extend(chars.next()),
            ':' => out.push(String::new()),
            c => out.last_mut().unwrap().push(c),
        }
    }
    out
}

/// The Wi-Fi item's list, keyboard and progress, while it's open.
pub struct Menu {
    state: State,
}

enum State {
    Scanning(Receiver<Vec<Network>>),
    List { networks: Vec<Network>, sel: usize },
    /// What to do with a saved network.
    Actions { network: Network, actions: Vec<Action>, sel: usize },
    Password { network: Network, keyboard: Keyboard },
    Connecting { ssid: String, done: Receiver<Result<(), Failure>>, network: Network },
    /// Disconnecting or forgetting: the message to show when it's done.
    Working { text: String, done: Receiver<Result<String, String>> },
    Message { text: String, until: Instant },
}

impl Menu {
    pub fn open() -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(scan());
        });
        Self { state: State::Scanning(rx) }
    }

    /// Handles keys and background work. Returns false once it's closed.
    pub fn update(&mut self, ctx: &egui::Context) -> bool {
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        let back = [Key::Escape, Key::BrowserBack].into_iter().any(pressed);
        match &mut self.state {
            State::Scanning(rx) => {
                if back {
                    return false;
                }
                if let Ok(networks) = rx.try_recv() {
                    let sel = networks.iter().position(|n| n.in_use).unwrap_or(0);
                    self.state = State::List { networks, sel };
                }
            }
            State::List { networks, sel } => {
                if back || pressed(Key::ArrowLeft) || pressed(Key::Backspace) {
                    return false;
                }
                if pressed(Key::ArrowUp) && *sel > 0 {
                    *sel -= 1;
                }
                if pressed(Key::ArrowDown) && *sel + 1 < networks.len() {
                    *sel += 1;
                }
                if pressed(Key::Enter) || pressed(Key::Space) {
                    let Some(network) = networks.get(*sel).cloned() else { return false };
                    let actions = if network.in_use {
                        vec![Action::Disconnect, Action::Forget]
                    } else if network.known && network.nearby {
                        vec![Action::Connect, Action::Forget]
                    } else if network.known {
                        vec![Action::Forget]
                    } else {
                        vec![]
                    };
                    self.state = if !actions.is_empty() {
                        State::Actions { network, actions, sel: 0 }
                    } else if network.secure {
                        let prompt = format!("Password for {}", network.ssid);
                        State::Password { network, keyboard: Keyboard::new(prompt) }
                    } else {
                        start(network, None)
                    };
                }
            }
            State::Actions { network, actions, sel } => {
                if back || pressed(Key::ArrowLeft) || pressed(Key::Backspace) {
                    return false;
                }
                if pressed(Key::ArrowUp) && *sel > 0 {
                    *sel -= 1;
                }
                if pressed(Key::ArrowDown) && *sel + 1 < actions.len() {
                    *sel += 1;
                }
                if pressed(Key::Enter) || pressed(Key::Space) {
                    let network = network.clone();
                    self.state = match actions[*sel] {
                        Action::Connect => start(network, None),
                        Action::Disconnect => work(format!("Disconnecting from {}", network.ssid), disconnect),
                        Action::Forget => work(format!("Forgetting {}", network.ssid), move || forget(&network.ssid)),
                    };
                }
            }
            State::Password { network, keyboard } => match keyboard.update(ctx) {
                keyboard::Outcome::Typing => {}
                keyboard::Outcome::Cancel => return false,
                keyboard::Outcome::Done(password) => self.state = start(network.clone(), Some(password)),
            },
            State::Connecting { ssid, done, network } => {
                if let Ok(result) = done.try_recv() {
                    self.state = match result {
                        Ok(()) => State::Message {
                            text: format!("Connected to {ssid}"),
                            until: Instant::now() + Duration::from_secs(2),
                        },
                        Err(Failure::Password) => {
                            let prompt = format!("That password didn't work. Password for {ssid}");
                            let network = Network { known: false, ..network.clone() };
                            State::Password { network, keyboard: Keyboard::new(prompt) }
                        }
                        Err(Failure::Other(e)) => State::Message {
                            text: format!("Couldn't connect: {e}"),
                            until: Instant::now() + Duration::from_secs(6),
                        },
                    };
                }
            }
            State::Working { done, .. } => {
                if let Ok(result) = done.try_recv() {
                    let (text, secs) = match result {
                        Ok(text) => (text, 2),
                        Err(e) => (format!("That didn't work: {e}"), 6),
                    };
                    self.state = State::Message { text, until: Instant::now() + Duration::from_secs(secs) };
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

    /// Draws the list under the Wi-Fi item (from `at`), or the keyboard over
    /// the whole `screen`.
    pub fn draw(&self, p: &Painter, screen: Rect, at: Pos2, s: f32, t: f64) {
        let dots = ".".repeat((t * 3.0) as usize % 4);
        let note = |text: String| widgets::note(p, at, s, &text);
        match &self.state {
            State::Scanning(_) => note(format!("Looking for networks{dots}")),
            State::List { networks, sel } => {
                let rows: Vec<Row> = networks
                    .iter()
                    .map(|n| Row {
                        label: n.ssid.clone(),
                        detail: Some(if !n.nearby {
                            "Saved · not nearby".to_owned()
                        } else {
                            let what = if n.known { " · saved" } else if n.secure { " · password" } else { "" };
                            format!("{}{what}", strength(n.signal))
                        }),
                        marked: n.in_use,
                    })
                    .collect();
                widgets::list(p, at, s, &rows, *sel, "No networks found");
            }
            State::Actions { network, actions, sel } => {
                note(network.ssid.clone());
                let rows: Vec<Row> =
                    actions.iter().map(|a| Row { label: a.label().into(), detail: None, marked: false }).collect();
                widgets::list(p, at + egui::vec2(0.0, 52.0 * s), s, &rows, *sel, "");
            }
            State::Password { keyboard, .. } => keyboard.draw(p, screen, s),
            State::Connecting { ssid, .. } => note(format!("Connecting to {ssid}{dots}")),
            State::Working { text, .. } => note(format!("{text}{dots}")),
            State::Message { text, .. } => note(text.clone()),
        }
    }
}

fn start(network: Network, password: Option<String>) -> State {
    let (tx, rx) = channel();
    let ssid = network.ssid.clone();
    let for_thread = network.clone();
    std::thread::spawn(move || {
        let _ = tx.send(connect(&for_thread, password.as_deref()));
    });
    State::Connecting { ssid, done: rx, network }
}

/// Runs `job` in the background, showing `text` until it's done.
fn work(text: String, job: impl FnOnce() -> Result<String, String> + Send + 'static) -> State {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
    });
    State::Working { text, done: rx }
}

/// Signal strength in words.
fn strength(signal: u8) -> &'static str {
    match signal {
        70.. => "Strong",
        45..70 => "Good",
        25..45 => "OK",
        _ => "Weak",
    }
}

#[cfg(test)]
mod tests {
    use super::fields;

    #[test]
    fn splits_nmcli_terse_output() {
        assert_eq!(fields("*:Home:80:WPA2"), ["*", "Home", "80", "WPA2"]);
        assert_eq!(fields(r" :Cafe\: Free:40:"), [" ", "Cafe: Free", "40", ""]);
        assert_eq!(fields(r"back\\slash:x"), [r"back\slash", "x"]);
    }
}
