use std::fs::File;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::io::FromRawFd;
use std::time::{Duration, Instant};
use memmap2::MmapOptions;
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, ZwlrLayerShellV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1};
use crate::behavior::hit_close;
use crate::render::render_popup;
use crate::spec::{CloseReason, PopupSpec};
use super::PopupBackend;

pub struct WaylandBackend;

#[derive(Default)]
struct State {
    shm: Option<wl_shm::WlShm>,
    comp: Option<wl_compositor::WlCompositor>,
    shell: Option<ZwlrLayerShellV1>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    pointer_pos: (f64, f64),
    clicked: Option<(f64, f64)>,
    configure_serial: Option<u32>,
    shown: bool,
}

impl WaylandBackend {
    pub fn new() -> Self { WaylandBackend }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_shm" => { state.shm = Some(registry.bind::<wl_shm::WlShm, (), State>(name, version.min(1), qh, ())); }
                "wl_compositor" => { state.comp = Some(registry.bind::<wl_compositor::WlCompositor, (), State>(name, version.min(4), qh, ())); }
                "wl_seat" => { state.seat = Some(registry.bind::<wl_seat::WlSeat, (), State>(name, version.min(7), qh, ())); }
                "zwlr_layer_shell_v1" => { state.shell = Some(registry.bind::<ZwlrLayerShellV1, (), State>(name, version.min(5), qh, ())); }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(_: &mut Self, _: &wl_compositor::WlCompositor, _: wl_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(_: &mut Self, _: &wl_surface::WlSurface, _: wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(state: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { .. } = event {
            if state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => { state.pointer_pos = (surface_x, surface_y); }
            wl_pointer::Event::Button { button, state: bs, .. } => {
                if matches!(bs, WEnum::Value(wl_pointer::ButtonState::Pressed)) && button == 0x110 /* BTN_LEFT */ {
                    state.clicked = Some(state.pointer_pos);
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<ZwlrLayerShellV1, ()> for State {
    fn event(_: &mut Self, _: &ZwlrLayerShellV1, _: zwlr_layer_shell_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwlrLayerSurfaceV1, ()> for State {
    fn event(state: &mut Self, _: &ZwlrLayerSurfaceV1, event: zwlr_layer_surface_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
            state.configure_serial = Some(serial);
        }
    }
}

/// 读取 wayland socket 上等待的事件并派发（wayland-client 0.31 的 dispatch_pending 不读 socket，必须手动 read）
fn read_events(conn: &Connection, queue: &mut EventQueue<State>, state: &mut State) -> Result<(), String> {
    let fd = conn.backend().poll_fd().as_raw_fd();
    let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    unsafe { libc::poll(&mut pfd, 1, 50); }
    if pfd.revents & libc::POLLIN != 0 {
        if let Some(guard) = conn.prepare_read() {
            guard.read().map_err(|e| format!("wayland read: {e}"))?;
        }
    }
    conn.flush().ok();
    queue.dispatch_pending(state).map_err(|e| format!("wayland dispatch: {e}")).map(|_| ())
}

impl PopupBackend for WaylandBackend {
    fn show(&mut self, spec: &PopupSpec) -> Result<CloseReason, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("Wayland 连接失败: {e}"))?;
        let mut queue: EventQueue<State> = conn.new_event_queue();
        let qh = queue.handle();
        let mut state = State::default();

        // 绑定全局
        conn.display().get_registry(&qh, ());
        conn.flush().map_err(|e| format!("flush registry: {e}"))?;
        for _ in 0..100 {
            queue.blocking_dispatch(&mut state).map_err(|e| format!("dispatch registry: {e}"))?;
            if state.shell.is_some() && state.shm.is_some() && state.comp.is_some() { break; }
        }
        let shell = state.shell.clone().ok_or("无 zwlr_layer_shell_v1（需要 wlr-layer-shell 支持的合成器）")?;
        let shm = state.shm.clone().ok_or("无 wl_shm")?;
        let comp = state.comp.clone().ok_or("无 wl_compositor")?;

        // 渲染
        let fonts = crate::fonts::load_fonts().map_err(|e| e.to_string())?;
        let popup = render_popup(spec, &fonts, None, 0).map_err(|e| e.to_string())?;
        let (w, h) = (popup.w as i32, popup.h as i32);
        let stride = (popup.w * 4) as i32;

        // memfd 共享内存（注意 memmap2 0.9：map_mut(&file) 直接接受 &File，无 .fd() 方法）
        let buf_size = (popup.data.len()) as i64;
        let name = b"adpop-shm\0";
        let fd = unsafe { libc::memfd_create(name.as_ptr() as *const i8, libc::MFD_CLOEXEC) };
        if fd < 0 { return Err("memfd_create 失败".to_string()); }
        unsafe { libc::ftruncate(fd, buf_size); }
        let file = unsafe { File::from_raw_fd(fd) };
        let mut mmap = unsafe { MmapOptions::new().len(buf_size as usize).map_mut(&file) }
            .map_err(|e| format!("mmap 失败: {e}"))?;
        mmap.copy_from_slice(&popup.data);

        // 创建类请求必须带 qh + udata 参数（wayland-client 0.31 生成代码）
        let pool = shm.create_pool(unsafe { BorrowedFd::borrow_raw(fd) }, buf_size as i32, &qh, ());
        let buffer = pool.create_buffer(0, w, h, stride, wl_shm::Format::Xrgb8888, &qh, ());

        let surface = comp.create_surface(&qh, ());
        let layer_surface = shell.get_layer_surface(&surface, None, zwlr_layer_shell_v1::Layer::Top, "adpop".to_string(), &qh, ());
        layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::Right | zwlr_layer_surface_v1::Anchor::Bottom);
        layer_surface.set_margin(24, 24, 24, 24);
        layer_surface.set_size(w as u32, h as u32);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);

        // 关键流程（Smithay/niri 强制）：先空 commit 触发 initial configure，ack 后才能 attach buffer
        surface.commit();
        conn.flush().map_err(|e| format!("flush: {e}"))?;

        let start = Instant::now();
        let deadline = if spec.duration == 0 { None } else { Some(start + Duration::from_secs(spec.duration)) };

        loop {
            if !state.shown {
                // 等 configure
                if let Some(serial) = state.configure_serial.take() {
                    layer_surface.ack_configure(serial);
                    surface.attach(Some(&buffer), 0, 0);
                    surface.damage(0, 0, w, h);
                    surface.commit();
                    conn.flush().ok();
                    state.shown = true;
                }
            } else {
                // 已显示：处理点击关闭
                if let Some((x, y)) = state.clicked.take() {
                    if hit_close(x as i32, y as i32, spec.size.w) && !spec.no_close {
                        return Ok(CloseReason::Closed);
                    }
                }
                if let Some(d) = deadline {
                    if Instant::now() >= d { return Ok(CloseReason::TimedOut); }
                }
            }
            read_events(&conn, &mut queue, &mut state)?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}