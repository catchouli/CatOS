//! Global "go back to the menu" keys.
//!
//! While an app is open the menu doesn't have keyboard focus, so we read the
//! input devices directly (the tv user is in the `input` group). New devices
//! are picked up every few seconds, so the remote's dongle can be plugged in late.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use evdev::{EventSummary, KeyCode};

use crate::config::Controls;

pub fn spawn(controls: &Controls, on_home: impl Fn() + Send + Sync + 'static) {
    let parse = |names: &[String]| -> HashSet<KeyCode> {
        names
            .iter()
            .filter_map(|n| match KeyCode::from_str(n) {
                Ok(k) => Some(k),
                Err(_) => {
                    eprintln!("meownix-menu: unknown key name {n:?} in config");
                    None
                }
            })
            .collect()
    };
    let home = Arc::new(parse(&controls.home_keys));
    let hold = Arc::new(parse(&controls.hold_keys));
    let hold_for = Duration::from_secs_f32(controls.hold_seconds);
    let on_home = Arc::new(on_home);
    let held: Arc<Mutex<HashMap<KeyCode, Instant>>> = Arc::default();
    let watching: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();

    std::thread::spawn(move || {
        let mut ticks = 0u32;
        loop {
            if ticks % 30 == 0 {
                for (path, mut dev) in evdev::enumerate() {
                    if watching.lock().unwrap().contains(&path) {
                        continue;
                    }
                    let relevant = dev
                        .supported_keys()
                        .is_some_and(|keys| home.iter().chain(hold.iter()).any(|k| keys.contains(*k)));
                    if !relevant {
                        continue;
                    }
                    eprintln!("meownix-menu: watching {} ({})", dev.name().unwrap_or("?"), path.display());
                    watching.lock().unwrap().insert(path.clone());
                    let (home, hold, held, watching, on_home) =
                        (home.clone(), hold.clone(), held.clone(), watching.clone(), on_home.clone());
                    std::thread::spawn(move || {
                        loop {
                            let events = match dev.fetch_events() {
                                Ok(ev) => ev.collect::<Vec<_>>(),
                                Err(_) => break, // unplugged
                            };
                            for ev in events {
                                if let EventSummary::Key(_, code, value) = ev.destructure() {
                                    if value == 1 && home.contains(&code) {
                                        on_home();
                                    }
                                    if hold.contains(&code) {
                                        let mut held = held.lock().unwrap();
                                        match value {
                                            1 => {
                                                held.insert(code, Instant::now());
                                            }
                                            0 => {
                                                held.remove(&code);
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        watching.lock().unwrap().remove(&path);
                    });
                }
            }
            let fired = {
                let mut held = held.lock().unwrap();
                let before = held.len();
                held.retain(|_, since| since.elapsed() < hold_for);
                held.len() != before
            };
            if fired {
                on_home();
            }
            ticks = ticks.wrapping_add(1);
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}
