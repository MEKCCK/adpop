#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session { Wayland, X11, None }

pub fn detect() -> Session {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Session::Wayland
    } else if std::env::var_os("DISPLAY").is_some() {
        Session::X11
    } else {
        Session::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // Env vars are process-global; tests below mutate them concurrently
    // by default, so serialize them to avoid flaky interleavings.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn detects_wayland_first() {
        let _g = lock_env();
        std::env::set_var("WAYLAND_DISPLAY", "wayland-1");
        std::env::set_var("DISPLAY", ":0");
        assert_eq!(detect(), Session::Wayland);
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
    }

    #[test]
    fn detects_x11() {
        let _g = lock_env();
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::set_var("DISPLAY", ":0");
        assert_eq!(detect(), Session::X11);
        std::env::remove_var("DISPLAY");
    }

    #[test]
    fn detects_none() {
        let _g = lock_env();
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        assert_eq!(detect(), Session::None);
    }
}
