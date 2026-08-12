mod backend;
mod behavior;
mod cli;
mod fonts;
mod render;
mod session;
mod spec;

use backend::PopupBackend;

fn main() {
    let spec = cli::parse_args(std::env::args().collect()).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(2); });
    let mut backend = backend::x11::X11Backend::new();
    match backend.show(&spec) {
        Ok(r) => { let code = match r { spec::CloseReason::Closed => 0, spec::CloseReason::TimedOut => 1, spec::CloseReason::Jumped => 3 }; std::process::exit(code); }
        Err(e) => { eprintln!("{e}"); std::process::exit(4); }
    }
}
