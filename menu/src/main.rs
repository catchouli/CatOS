//! meownix crossbar menu.
//!
//! A crossbar launcher: categories run left/right, the items in the
//! selected category run up/down. OK (Enter) launches an item fullscreen; holding
//! Back (or pressing Home) on the remote closes it and returns here.
//!
//! It runs as the only client of the cage kiosk compositor. Launched apps open as
//! new windows on top of it, and cage shows the menu again when they close.
//!
//! For development on a normal desktop:
//!     cargo run -- --windowed --dry-run --config menu.toml

mod avatar;
mod config;
mod icons;
#[cfg(target_os = "linux")]
mod remote;
mod runner;

use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Key, Mesh,
    Painter, Pos2, Rect, Shape, Stroke, ViewportCommand, pos2,
};

use avatar::Avatar;
use config::{Config, Item};
use runner::Runner;

const BG_TOP: Color32 = Color32::from_rgb(14, 18, 40);
const BG_BOTTOM: Color32 = Color32::from_rgb(46, 22, 64);

/// The menu fades to black before an app opens, and back in when it closes.
/// (cage can't blend windows, so the app itself appears as a cut from black.)
const FADE_OUT: Duration = Duration::from_millis(350);
const FADE_IN: Duration = Duration::from_millis(500);

/// The cursor shows while the air-mouse is moving, and hides again after this.
const CURSOR_HIDE_AFTER: Duration = Duration::from_secs(3);

/// Inter if it's installed (Debian's fonts-inter), otherwise egui's built-in font.
const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/opentype/inter/InterVariable.ttf",
    "/usr/share/fonts/truetype/inter/InterVariable.ttf",
    "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
    "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
];

fn white(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
}

fn strftime(fmt: &str) -> String {
    let fmt = CString::new(fmt).unwrap();
    let mut buf = [0u8; 64];
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), fmt.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

fn hostname() -> String {
    let mut buf = [0 as libc::c_char; 256];
    unsafe {
        libc::gethostname(buf.as_mut_ptr(), buf.len());
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

/// The address other machines on the LAN can reach us at.
fn local_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?; // UDP connect sends nothing
    Some(s.local_addr().ok()?.ip().to_string())
}

/// An app waiting for the fade-out to finish before it starts.
struct Launch {
    name: String,
    command: Vec<String>,
    scale: f32,
    since: Instant,
}

struct Menu {
    cfg: Config,
    runner: Runner,
    dry_run: bool,
    windowed: bool,
    cat: usize,
    items: Vec<usize>,
    // Animated positions that chase `cat` and `items`.
    cat_pos: f32,
    item_pos: Vec<f32>,
    toast: Option<(String, Instant)>,
    confirm: Option<((usize, usize), Instant)>,
    ip: Option<String>,
    ip_checked: Option<Instant>,
    launching: Option<Launch>,
    /// The app that's opening or open, while the menu sits behind it in black.
    opening: Option<String>,
    fade_in_from: Option<Instant>,
    avatar: Option<Avatar>,
    mouse_moved: Option<Instant>,
}

impl Menu {
    fn new(cfg: Config, runner: Runner, dry_run: bool, windowed: bool) -> Self {
        let n = cfg.category.len();
        let avatar = cfg.avatar.model.as_deref().and_then(Avatar::load);
        Self {
            cfg,
            runner,
            dry_run,
            windowed,
            cat: 0,
            items: vec![0; n],
            cat_pos: 0.0,
            item_pos: vec![0.0; n],
            toast: None,
            confirm: None,
            ip: None,
            ip_checked: None,
            launching: None,
            opening: None,
            fade_in_from: None,
            avatar,
            mouse_moved: None,
        }
    }

    fn selected(&self) -> Option<&Item> {
        self.cfg.category.get(self.cat)?.item.get(self.items[self.cat])
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        let n_items = self.cfg.category[self.cat].item.len();
        let before = (self.cat, self.items[self.cat]);
        if pressed(Key::ArrowLeft) && self.cat > 0 {
            self.cat -= 1;
        }
        if pressed(Key::ArrowRight) && self.cat + 1 < self.cfg.category.len() {
            self.cat += 1;
        }
        if pressed(Key::ArrowUp) && self.items[self.cat] > 0 {
            self.items[self.cat] -= 1;
        }
        if pressed(Key::ArrowDown) && self.items[self.cat] + 1 < n_items {
            self.items[self.cat] += 1;
        }
        if before != (self.cat, self.items[self.cat]) {
            self.confirm = None;
        }
        if pressed(Key::Enter) || pressed(Key::Space) {
            self.activate(ctx);
        }
        if self.windowed && pressed(Key::Escape) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn activate(&mut self, ctx: &egui::Context) {
        let Some(item) = self.selected() else { return };
        let (name, command, action) = (item.name.clone(), item.command.clone(), item.action.clone());
        let scale = self.app_scale(ctx, item.scale);
        let here = (self.cat, self.items[self.cat]);
        let now = Instant::now();
        match action.as_deref() {
            Some(a @ ("reboot" | "poweroff")) => {
                // Power actions need OK twice, so a stray press can't turn the TV box off.
                if !matches!(self.confirm, Some((at, until)) if at == here && now < until) {
                    self.confirm = Some((here, now + Duration::from_secs(4)));
                    return;
                }
                self.confirm = None;
                self.toast = Some((format!("{name}…"), now + Duration::from_secs(30)));
                eprintln!("meownix-menu: systemctl {a}");
                if !self.dry_run {
                    let _ = std::process::Command::new("systemctl").arg(a).spawn();
                }
            }
            Some("quit") => ctx.send_viewport_cmd(ViewportCommand::Close),
            Some(_) => {}
            None if !command.is_empty() => {
                // Fade out first; ui() starts the app when the fade is done.
                self.launching = Some(Launch { name, command, scale, since: now });
            }
            None => {}
        }
    }

    /// How black the screen is, 0 to 1: fading out to open an app, black while
    /// it's open, fading back in after it closes.
    fn veil(&self) -> f32 {
        let progress = |since: Instant, over: Duration| (since.elapsed().as_secs_f32() / over.as_secs_f32()).min(1.0);
        let v = if let Some(l) = &self.launching {
            progress(l.since, FADE_OUT)
        } else if self.opening.is_some() {
            1.0
        } else if let Some(t) = self.fade_in_from {
            1.0 - progress(t, FADE_IN)
        } else {
            0.0
        };
        v * v * (3.0 - 2.0 * v) // smoothstep
    }

    /// The zoom factor for an app: the configured 1080p scale, adjusted for the
    /// screen's real height in pixels.
    fn app_scale(&self, ctx: &egui::Context, item_scale: Option<f32>) -> f32 {
        let height_px = ctx.content_rect().height() * ctx.pixels_per_point();
        let scale = item_scale.unwrap_or(self.cfg.display.scale) * height_px / 1080.0;
        (scale * 4.0).round().max(4.0) / 4.0 // quarter steps, never below 1x
    }

    fn description(&mut self, item_idx: (usize, usize)) -> String {
        let item = &self.cfg.category[item_idx.0].item[item_idx.1];
        if matches!(self.confirm, Some((at, _)) if at == item_idx) {
            return format!("Press OK again to {}", item.name.to_lowercase());
        }
        if item.action.as_deref() == Some("network") {
            if self.ip_checked.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
                self.ip = local_ip();
                self.ip_checked = Some(Instant::now());
            }
            return format!("{} · {}", hostname(), self.ip.as_deref().unwrap_or("not connected"));
        }
        item.description.clone()
    }

    /// `mouse` is where the cursor is, while the air-mouse is in use.
    fn draw(&mut self, p: &Painter, screen: Rect, dt: f32, t: f64, veil: f32, mouse: Option<Pos2>) {
        let (w, h) = (screen.width(), screen.height());
        let s = h / 1080.0;
        let px = |size: f32| FontId::proportional(size * s);

        let ease = 1.0 - (-dt * 14.0).exp();
        self.cat_pos += (self.cat as f32 - self.cat_pos) * ease;
        for (pos, &target) in self.item_pos.iter_mut().zip(&self.items) {
            *pos += (target as f32 - *pos) * ease;
        }
        if self.confirm.is_some_and(|(_, until)| Instant::now() > until) {
            self.confirm = None;
        }

        // Background gradient
        let mut mesh = Mesh::default();
        for (pos, c) in [
            (screen.left_top(), BG_TOP),
            (screen.right_top(), BG_TOP),
            (screen.right_bottom(), BG_BOTTOM),
            (screen.left_bottom(), BG_BOTTOM),
        ] {
            mesh.colored_vertex(pos, c);
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(Shape::mesh(mesh));

        // Slow ribbon of waves across the middle.
        for k in 0..3 {
            let kf = k as f32;
            let pts: Vec<Pos2> = (0..=64)
                .map(|i| {
                    let u = i as f32 / 64.0;
                    let phase = u * std::f32::consts::TAU * (1.0 + 0.3 * kf) + (t as f32) * (0.25 + 0.1 * kf) + kf;
                    pos2(screen.left() + w * u, screen.top() + h * 0.62 + phase.sin() * h * (0.05 + 0.02 * kf))
                })
                .collect();
            let c = Color32::from_rgba_unmultiplied(120 + 30 * k, 140 + 30 * k, 255, 70);
            p.add(Shape::line(pts, Stroke::new(1.5 * s, c)));
        }

        let focus_x = screen.left() + w * 0.26;
        let row_y = screen.top() + h * 0.30;
        let spacing = 220.0 * s;

        // The avatar watches the cursor, or else the selected item. Behind the
        // menu's text, and not drawn at all while an app covers the menu.
        if let Some(avatar) = &self.avatar {
            avatar.update(dt, screen, mouse.unwrap_or(pos2(focus_x + 200.0 * s, row_y + 150.0 * s)));
            if veil < 1.0 {
                avatar.paint(p, screen);
            }
        }

        // Category row
        for (i, cat) in self.cfg.category.iter().enumerate() {
            let x = focus_x + (i as f32 - self.cat_pos) * spacing;
            let selected = i == self.cat;
            let (size, alpha) = if selected { (104.0, 255) } else { (80.0, 120) };
            icons::draw(p, &cat.icon, pos2(x, row_y), size * s, white(alpha), BG_TOP);
            if selected {
                p.text(pos2(x, row_y + 66.0 * s), Align2::CENTER_TOP, &cat.name, px(30.0), white(255));
            }
        }

        // Items of the selected category; the list scrolls so the selection stays put.
        let top = row_y + 150.0 * s;
        let row_h = 112.0 * s;
        let scroll = self.item_pos[self.cat];
        for j in 0..self.cfg.category[self.cat].item.len() {
            let y = top + (j as f32 - scroll) * row_h;
            if (j as f32) < scroll - 0.5 {
                continue; // items above the selection tuck away behind the row
            }
            if y > screen.bottom() {
                break;
            }
            let selected = j == self.items[self.cat];
            let desc = if selected { self.description((self.cat, j)) } else { String::new() };
            let item = &self.cfg.category[self.cat].item[j];
            let (size, alpha) = if selected { (76.0, 255) } else { (60.0, 110) };
            icons::draw(p, &item.icon, pos2(focus_x, y), size * s, white(alpha), BG_TOP);
            let lx = focus_x + 70.0 * s;
            if selected {
                p.text(pos2(lx, y - 12.0 * s), Align2::LEFT_CENTER, &item.name, px(42.0), white(255));
                if !desc.is_empty() {
                    p.text(pos2(lx, y + 16.0 * s), Align2::LEFT_TOP, desc, px(24.0), white(170));
                }
            } else {
                p.text(pos2(lx, y), Align2::LEFT_CENTER, &item.name, px(32.0), white(130));
            }
        }

        // Clock
        let right = screen.right() - 60.0 * s;
        p.text(pos2(right, screen.top() + 44.0 * s), Align2::RIGHT_TOP, strftime("%H:%M"), px(46.0), white(255));
        p.text(pos2(right, screen.top() + 102.0 * s), Align2::RIGHT_TOP, strftime("%a %-d %b"), px(22.0), white(160));

        // "Restart..." after a power action
        if let Some((msg, until)) = &self.toast {
            if Instant::now() > *until {
                self.toast = None;
            } else {
                let dots = ".".repeat((t * 3.0) as usize % 4);
                p.text(
                    pos2(focus_x - 52.0 * s, screen.bottom() - 70.0 * s),
                    Align2::LEFT_BOTTOM,
                    format!("{}{dots}", msg.trim_end_matches('…')),
                    px(30.0),
                    white(255),
                );
            }
        }

        // Bottom left, under the menu: the avatar stands at the right.
        p.text(
            pos2(focus_x - 52.0 * s, screen.bottom() - 40.0 * s),
            Align2::LEFT_BOTTOM,
            "OK to open  ·  hold Back or press Home to return here",
            px(20.0),
            white(90),
        );

        // Fade to black, with "Opening Flex..." in the middle while the app starts.
        if veil > 0.0 {
            let a = (veil * 255.0) as u8;
            p.rect_filled(screen, 0.0, Color32::from_black_alpha(a));
            if let Some(name) = self.launching.as_ref().map(|l| &l.name).or(self.opening.as_ref()) {
                let text = format!("Opening {name}");
                // Position from the width with all three dots, so the text doesn't shift.
                let full = p.layout_no_wrap(format!("{text}..."), px(34.0), white(a)).size().x;
                let dots = ".".repeat((t * 3.0) as usize % 4);
                p.text(
                    pos2(screen.center().x - full / 2.0, screen.center().y),
                    Align2::LEFT_CENTER,
                    format!("{text}{dots}"),
                    px(34.0),
                    white(a),
                );
            }
        }
    }
}

impl eframe::App for Menu {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Start the app once the menu has faded out.
        if self.launching.as_ref().is_some_and(|l| l.since.elapsed() >= FADE_OUT) {
            let l = self.launching.take().unwrap();
            if self.runner.launch(&l.name, &l.command, l.scale) {
                self.opening = Some(l.name);
            } else {
                self.fade_in_from = Some(Instant::now());
            }
        }
        let running = self.runner.running();
        if running.is_none() && self.opening.take().is_some() {
            self.fade_in_from = Some(Instant::now()); // the app closed: fade back in
        }
        if running.is_none() && self.launching.is_none() {
            self.handle_keys(&ctx);
        }
        if ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            self.mouse_moved = Some(Instant::now());
        }
        let mouse_active = self.mouse_moved.is_some_and(|t| t.elapsed() < CURSOR_HIDE_AFTER);
        if !self.windowed && !mouse_active {
            ctx.set_cursor_icon(CursorIcon::None);
        }
        let mouse = if mouse_active { ctx.input(|i| i.pointer.latest_pos()) } else { None };

        let (dt, t) = ctx.input(|i| (i.stable_dt.min(0.1), i.time));
        let veil = self.veil();
        self.draw(ui.painter(), ctx.content_rect(), dt, t, veil, mouse);

        if running.is_some() {
            // An app is starting or covering us: redraw just enough to animate the
            // "Opening…" dots, and leave the CPU to the app.
            ctx.request_repaint_after(Duration::from_millis(250));
        } else {
            ctx.request_repaint();
        }
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let Some(bytes) = FONT_CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("inter".into(), FontData::from_owned(bytes).into());
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result {
    let (mut windowed, mut dry_run, mut config_path) = (false, false, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--windowed" => windowed = true,
            "--dry-run" => dry_run = true,
            "--config" => config_path = args.next().map(PathBuf::from),
            _ => {
                eprintln!("usage: meownix-menu [--windowed] [--dry-run] [--config FILE]");
                std::process::exit(2);
            }
        }
    }
    let cfg = config::load(config_path).unwrap_or_else(|e| {
        eprintln!("meownix-menu: {e}");
        std::process::exit(1);
    });

    let viewport = egui::ViewportBuilder::default().with_title("meownix").with_app_id("meownix");
    let viewport = if windowed {
        viewport.with_inner_size([1280.0, 720.0]).with_position([80.0, 80.0])
    } else {
        viewport.with_fullscreen(true)
    };
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    let runner = Runner::new(dry_run);
    eframe::run_native(
        "meownix",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            #[cfg(target_os = "linux")]
            {
                let (runner, ctx) = (runner.clone(), cc.egui_ctx.clone());
                remote::spawn(&cfg.controls, move || {
                    runner.close();
                    ctx.request_repaint();
                });
            }
            Ok(Box::new(Menu::new(cfg, runner, dry_run, windowed)))
        }),
    )
}
