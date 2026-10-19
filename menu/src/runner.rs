//! Launches apps one at a time and closes them again.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Running {
    pub name: String,
    pub started: Instant,
    child: Child,
}

#[derive(Clone)]
pub struct Runner {
    current: Arc<Mutex<Option<Running>>>,
    dry_run: bool,
}

impl Runner {
    pub fn new(dry_run: bool) -> Self {
        Self { current: Arc::default(), dry_run }
    }

    /// Name and start time of the running app, if any. Reaps it if it exited.
    pub fn running(&self) -> Option<(String, Instant)> {
        let mut cur = self.current.lock().unwrap();
        if let Some(r) = cur.as_mut() {
            if !matches!(r.child.try_wait(), Ok(None)) {
                eprintln!("catos-menu: {} exited", r.name);
                *cur = None;
            }
        }
        cur.as_ref().map(|r| (r.name.clone(), r.started))
    }

    /// Runs `command`. `{scale}` in its arguments, and $CATOS_SCALE in its
    /// environment, give the zoom factor the app should use.
    pub fn launch(&self, name: &str, command: &[String], scale: f32) -> bool {
        if self.running().is_some() || command.is_empty() {
            return false;
        }
        eprintln!("catos-menu: launching {name}: {command:?}");
        let mut cmd = if self.dry_run {
            let mut c = Command::new("sleep");
            c.arg("3");
            c
        } else {
            let scale = format!("{scale:.2}");
            let mut c = Command::new(&command[0]);
            c.args(command[1..].iter().map(|a| a.replace("{scale}", &scale)));
            c.env("CATOS_SCALE", &scale);
            c
        };
        // Own process group, so closing the app also closes its helpers
        // (browser content processes and so on).
        cmd.process_group(0).stdin(Stdio::null());
        match cmd.spawn() {
            Ok(child) => {
                *self.current.lock().unwrap() = Some(Running {
                    name: name.to_owned(),
                    started: Instant::now(),
                    child,
                });
                true
            }
            Err(e) => {
                eprintln!("catos-menu: failed to launch {name}: {e}");
                false
            }
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    /// Ask the running app to quit, and kill it if it hasn't after 4 seconds.
    pub fn close(&self) {
        let pid = match self.current.lock().unwrap().as_ref() {
            Some(r) => {
                eprintln!("catos-menu: closing {}", r.name);
                r.child.id() as libc::pid_t
            }
            None => return,
        };
        unsafe { libc::killpg(pid, libc::SIGTERM) };
        let me = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(4));
            let still_there = matches!(
                me.current.lock().unwrap().as_mut().map(|r| (r.child.id(), r.child.try_wait())),
                Some((id, Ok(None))) if id as libc::pid_t == pid
            );
            if still_there {
                eprintln!("catos-menu: killing process group {pid}");
                unsafe { libc::killpg(pid, libc::SIGKILL) };
            }
        });
    }
}
