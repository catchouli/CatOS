//! A Live2D avatar who stands at the side of the menu and looks at the cursor
//! while the air-mouse is moving, or at the selected item otherwise.
//!
//! Live2D's Cubism framework does the animating and drawing, through a small C++
//! bridge (avatar/avatar.cpp). That's only built when the Cubism SDK is
//! available (see build.rs); without it `load` always returns None.

#![cfg_attr(not(avatar), allow(unused_imports, dead_code))]

use std::path::Path;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Painter, Pos2, Rect};

/// Where the shaders for Live2D's OpenGL renderer are installed.
pub const SHADER_DIR: &str = "/usr/share/catos/live2d-shaders";

/// Her height, as a fraction of the screen's, and where her middle sits.
const HEIGHT: f32 = 0.95;
const CENTRE_X: f32 = 0.80;
const CENTRE_Y: f32 = 0.58;
/// Roughly where her face is, for working out which way to look.
const FACE_Y: f32 = 0.30;

#[cfg(avatar)]
mod ffi {
    use std::ffi::{c_char, c_float, c_int, c_uint, c_void};

    unsafe extern "C" {
        pub fn catos_avatar_load(model_json: *const c_char, shader_dir: *const c_char) -> *mut c_void;
        pub fn catos_avatar_texture_count(a: *mut c_void) -> c_int;
        pub fn catos_avatar_texture_path(a: *mut c_void, i: c_int, buf: *mut c_char, len: c_int);
        pub fn catos_avatar_init_gl(a: *mut c_void, textures: *const c_uint, count: c_int, w: c_int, h: c_int);
        pub fn catos_avatar_update(a: *mut c_void, dt: c_float, look_x: c_float, look_y: c_float);
        pub fn catos_avatar_draw(a: *mut c_void, x: c_float, y: c_float, height: c_float, w: c_int, h: c_int);
        pub fn catos_avatar_free(a: *mut c_void);
    }
}

pub struct Avatar {
    #[cfg(avatar)]
    inner: Arc<Mutex<Inner>>,
}

#[cfg(avatar)]
struct Inner {
    model: *mut std::ffi::c_void,
    /// Texture files, uploaded on the first draw (when there's a GL context).
    texture_paths: Vec<std::path::PathBuf>,
    gl_ready: bool,
}

// The model is only touched with the mutex held, from the UI thread.
#[cfg(avatar)]
unsafe impl Send for Inner {}

#[cfg(avatar)]
impl Drop for Inner {
    fn drop(&mut self) {
        unsafe { ffi::catos_avatar_free(self.model) };
    }
}

/// The model's .model3.json: `path` itself, or the first one in it if it's a
/// folder. None, quietly, if there's nothing there: the avatar is optional.
fn find_model(path: &Path) -> Option<std::path::PathBuf> {
    if !path.is_dir() {
        return path.exists().then(|| path.to_path_buf());
    }
    let mut found: Vec<_> = std::fs::read_dir(path)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".model3.json"))
        .collect();
    found.sort();
    found.into_iter().next()
}

impl Avatar {
    /// Loads a model from its .model3.json (or a folder with one in it), or
    /// None if there isn't one, it can't be loaded, or this build has no
    /// Live2D support.
    #[cfg(avatar)]
    pub fn load(path: &Path) -> Option<Self> {
        use std::ffi::{CStr, CString};
        let model_json = find_model(path)?;
        let model_json = model_json.as_path();
        let path = CString::new(model_json.as_os_str().as_encoded_bytes()).ok()?;
        let shaders = CString::new(SHADER_DIR).unwrap();
        let model = unsafe { ffi::catos_avatar_load(path.as_ptr(), shaders.as_ptr()) };
        if model.is_null() {
            eprintln!("catos-menu: couldn't load the avatar from {}", model_json.display());
            return None;
        }
        let n = unsafe { ffi::catos_avatar_texture_count(model) };
        let texture_paths = (0..n)
            .map(|i| {
                let mut buf = [0 as std::ffi::c_char; 4096];
                unsafe {
                    ffi::catos_avatar_texture_path(model, i, buf.as_mut_ptr(), buf.len() as _);
                    CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned().into()
                }
            })
            .collect();
        let inner = Inner { model, texture_paths, gl_ready: false };
        Some(Self { inner: Arc::new(Mutex::new(inner)) })
    }

    #[cfg(not(avatar))]
    pub fn load(path: &Path) -> Option<Self> {
        if let Some(model_json) = find_model(path) {
            eprintln!(
                "catos-menu: this build has no Live2D support, so the avatar ({}) is off",
                model_json.display()
            );
        }
        None
    }

    /// Animates her by `dt` seconds, looking towards `target` on `screen`.
    #[cfg_attr(not(avatar), allow(unused_variables))]
    pub fn update(&self, dt: f32, screen: Rect, target: Pos2) {
        #[cfg(avatar)]
        {
            let face = screen.lerp_inside(egui::vec2(CENTRE_X, FACE_Y));
            let look_x = ((target.x - face.x) / (screen.width() * 0.4)).clamp(-1.0, 1.0);
            let look_y = (-(target.y - face.y) / (screen.height() * 0.4)).clamp(-1.0, 1.0);
            let inner = self.inner.lock().unwrap();
            unsafe { ffi::catos_avatar_update(inner.model, dt, look_x, look_y) };
        }
    }

    /// Adds her to the painter, on top of whatever's been drawn so far.
    #[cfg_attr(not(avatar), allow(unused_variables))]
    pub fn paint(&self, p: &Painter, screen: Rect) {
        #[cfg(avatar)]
        {
            let inner = self.inner.clone();
            let callback = eframe::egui_glow::CallbackFn::new(move |info, painter| {
                let [w, h] = info.screen_size_px;
                let mut inner = inner.lock().unwrap();
                unsafe { inner.draw(painter.gl(), w as i32, h as i32) };
            });
            p.add(egui::PaintCallback { rect: screen, callback: Arc::new(callback) });
        }
    }
}

#[cfg(avatar)]
impl Inner {
    unsafe fn draw(&mut self, gl: &eframe::glow::Context, w: i32, h: i32) {
        use eframe::glow::{self, HasContext};
        unsafe {
            if !self.gl_ready {
                self.gl_ready = true;
                let textures: Vec<u32> = self.texture_paths.iter().map(|p| upload_texture(gl, p)).collect();
                ffi::catos_avatar_init_gl(self.model, textures.as_ptr(), textures.len() as _, w, h);
            }
            // egui leaves its scissor on, which would also clip the masks
            // Live2D draws offscreen.
            gl.viewport(0, 0, w, h);
            gl.disable(glow::SCISSOR_TEST);
            // Clip space is -1..1 with +y up.
            let (x, y) = (CENTRE_X * 2.0 - 1.0, 1.0 - CENTRE_Y * 2.0);
            ffi::catos_avatar_draw(self.model, x, y, HEIGHT * 2.0, w, h);
        }
    }
}

/// Loads a PNG into a GL texture with premultiplied alpha, as the bridge
/// tells Live2D to expect. Returns 0 (no texture) if it can't be read.
#[cfg(avatar)]
unsafe fn upload_texture(gl: &eframe::glow::Context, path: &Path) -> u32 {
    use eframe::glow::{self, HasContext, PixelUnpackData};
    let rgba = match decode_png(path) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("catos-menu: avatar texture {}: {e}", path.display());
            return 0;
        }
    };
    unsafe {
        let tex = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            rgba.0 as i32,
            rgba.1 as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            PixelUnpackData::Slice(Some(&rgba.2)),
        );
        gl.generate_mipmap(glow::TEXTURE_2D);
        gl.bind_texture(glow::TEXTURE_2D, None);
        tex.0.get()
    }
}

/// (width, height, premultiplied RGBA pixels)
#[cfg(avatar)]
fn decode_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let mut rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        other => return Err(format!("unexpected colour type {other:?}")),
    };
    for px in rgba.chunks_mut(4) {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = ((*c as u16 * a + 127) / 255) as u8;
        }
    }
    Ok((info.width, info.height, rgba))
}
