mod backend;
mod behavior;
mod cli;
mod fonts;
mod media;
mod render;
mod session;
mod spec;

use backend::PopupBackend;
use spec::{CloseReason, PopupSpec};

const EXIT_CLOSED: i32 = 0;
const EXIT_TIMEOUT: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_NO_DISPLAY: i32 = 3;
const EXIT_MEDIA: i32 = 4;
const EXIT_VIDEO: i32 = 5;
const EXIT_JUMPED: i32 = 6;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let spec: PopupSpec = match cli::parse_args(std::env::args().collect()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("adpop: {e}");
            return EXIT_USAGE;
        }
    };

    match session::detect() {
        session::Session::Wayland => show_all(&mut backend::wayland::WaylandBackend::new(), &spec),
        session::Session::X11 => show_all(&mut backend::x11::X11Backend::new(), &spec),
        session::Session::None => {
            eprintln!("adpop: 无可用显示后端（既无 WAYLAND_DISPLAY 也无 DISPLAY）");
            EXIT_NO_DISPLAY
        }
    }
}

/// 多弹窗：逐个弹出（间隔 0.8s），任一关闭/跳转即停；全超时返回 TimedOut
fn show_all(b: &mut dyn PopupBackend, spec: &PopupSpec) -> i32 {
    let mut result = CloseReason::TimedOut;
    for i in 0..spec.count {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(800));
        }
        match b.show(spec) {
            Ok(r) => {
                result = r;
                if r != CloseReason::TimedOut {
                    break;
                }
            }
            Err(e) => {
                eprintln!("adpop: {e}");
                return EXIT_MEDIA;
            }
        }
    }
    match result {
        CloseReason::Closed => EXIT_CLOSED,
        CloseReason::TimedOut => EXIT_TIMEOUT,
        CloseReason::Jumped => EXIT_JUMPED,
    }
}
