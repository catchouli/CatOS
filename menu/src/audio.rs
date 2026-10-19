//! Picking the sound output, through PipeWire.
//!
//! A sound card offers one output per profile (on a graphics card, typically
//! one per HDMI/DisplayPort socket), but only the active profile's output
//! exists as a sink. So the choices here are the cards' output profiles:
//! picking one switches its card to that profile and makes the card's sink the
//! default. `pw-dump` lists everything as JSON; `wpctl` makes the changes.

use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    /// PipeWire id of the card.
    device: u64,
    /// Index of the card's profile that plays to this output.
    profile: u64,
    pub name: String,
    /// Whether the card's active profile already plays to this output (possibly
    /// alongside a microphone input), so picking it needn't switch profiles.
    active: bool,
    /// Whether this is what's playing now.
    pub current: bool,
}

/// The outputs that are available now (anything plugged into a socket the card
/// can tell is empty is left out).
pub fn outputs() -> Vec<Output> {
    pw_dump().map(|objects| outputs_from(&objects)).unwrap_or_default()
}

fn outputs_from(objects: &[Value]) -> Vec<Output> {
    let default_sink = default_sink_device(objects);
    let mut found = Vec::new();
    for obj in objects {
        let info = &obj["info"];
        if obj["type"] != "PipeWire:Interface:Device" || info["props"]["media.class"] != "Audio/Device" {
            continue;
        }
        let Some(device) = obj["id"].as_u64() else { continue };
        let card = info["props"]["device.description"].as_str().unwrap_or("Sound card");
        // The active profile's output part: "output:analog-stereo" of "output:analog-stereo+input:...".
        let active = info["params"]["Profile"][0]["name"].as_str().unwrap_or("").split('+').next().unwrap_or("");
        for profile in info["params"]["EnumProfile"].as_array().into_iter().flatten() {
            let name = profile["name"].as_str().unwrap_or("");
            // Output-only profiles: "output:hdmi-stereo-extra1", not "...+input:..." or "pro-audio".
            if !name.starts_with("output:") || name.contains('+') || profile["available"] == "no" {
                continue;
            }
            let Some(index) = profile["index"].as_u64() else { continue };
            let what = profile["description"].as_str().unwrap_or(name).trim_end_matches(" Output");
            found.push(Output {
                device,
                profile: index,
                name: format!("{what} · {card}"),
                active: active == name,
                current: default_sink == Some(device) && active == name,
            });
        }
    }
    found
}

/// Name of the output that's playing now, for the menu's description line.
pub fn current_name() -> Option<String> {
    outputs().into_iter().find(|o| o.current).map(|o| o.name)
}

/// Switches to `output`. Runs in the background: the card's new sink takes a
/// moment to appear before it can be made the default.
pub fn select(output: &Output) {
    let (device, profile, active) = (output.device, output.profile, output.active);
    std::thread::spawn(move || {
        if !active {
            run("wpctl", &["set-profile", &device.to_string(), &profile.to_string()]);
        }
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if let Some(sink) = pw_dump().and_then(|objects| sink_of(&objects, device)) {
                run("wpctl", &["set-default", &sink.to_string()]);
                return;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        eprintln!("catos-menu: no sound output appeared for card {device} profile {profile}");
    });
}

fn pw_dump() -> Option<Vec<Value>> {
    let out = Command::new("pw-dump").output().ok()?;
    serde_json::from_slice::<Value>(&out.stdout).ok()?.as_array().cloned()
}

/// The sink belonging to card `device`.
fn sink_of(objects: &[Value], device: u64) -> Option<u64> {
    objects
        .iter()
        .find(|o| {
            let props = &o["info"]["props"];
            props["media.class"] == "Audio/Sink" && props["device.id"].as_u64() == Some(device)
        })
        .and_then(|o| o["id"].as_u64())
}

/// The card the default sink belongs to.
fn default_sink_device(objects: &[Value]) -> Option<u64> {
    let name = objects
        .iter()
        .filter(|o| o["props"]["metadata.name"] == "default")
        .flat_map(|o| o["metadata"].as_array().cloned().unwrap_or_default())
        .find(|entry| entry["key"] == "default.audio.sink")?["value"]["name"]
        .as_str()?
        .to_owned();
    objects
        .iter()
        .find(|o| o["info"]["props"]["node.name"] == name.as_str())
        .and_then(|o| o["info"]["props"]["device.id"].as_u64())
}

fn run(program: &str, args: &[&str]) {
    match Command::new(program).args(args).status() {
        Ok(s) if s.success() => {}
        other => eprintln!("catos-menu: {program} {args:?} failed: {other:?}"),
    }
}


