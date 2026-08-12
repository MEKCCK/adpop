//! Wayland 后端：wlr-layer-shell TOP 层弹窗。
//!
//! 关键实现要点（niri 26.04 实测验证）：
//! - wayland-client 0.31 的 dispatch_pending 不读 socket，事件循环必须
//!   poll_fd → prepare_read → ReadEventsGuard::read → dispatch_pending
//! - Smithay/niri 强制：layer surface 必须先空 commit 触发 Configure 事件，
//!   收到后 ack_configure(serial) 才能 attach buffer（否则不显示）
//! - wl_shm buffer 被合成器缓存，改共享内存内容不更新显示 → 动画必须多 buffer 轮换
//! - 创建类请求（create_pool/create_buffer/get_layer_surface/get_pointer）
//!   必须带 qh + udata 参数（wayland-client 0.31 生成代码）

use std::fs::File;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::io::FromRawFd;
use std::time::{Duration, Instant};
use memmap2::MmapOptions;
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, ZwlrLayerShellV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1};

use crate::behavior::hit_close;
use crate::media::{load_gif, spawn_video_stream, try_read_video_frame, AudioGuard, GifFrame};
use crate::render::{render_popup, MediaFrame};
use crate::spec::{ClickZone, CloseReason, PopupSpec};
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
    output_w: u32,
    output_h: u32,
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
                "wl_output" => { let _ = registry.bind::<wl_output::WlOutput, (), State>(name, version.min(4), qh, ()); }
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
impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(state: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Mode { width, height, .. } = event {
            state.output_w = width as u32;
            state.output_h = height as u32;
        }
    }
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

/// 读取 wayland socket 上等待的事件并派发（dispatch_pending 不读 socket，必须手动 read）
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

/// --pos 绝对坐标 → (anchor_bits, (top,right,bottom,left)) 四象限推导
fn pos_to_anchor_margin(px: i32, py: i32, ww: i32, wh: i32, out_w: i32, out_h: i32) -> (u32, (i32, i32, i32, i32)) {
    let mut a = 0u32;
    let (mut top, mut right, mut bottom, mut left) = (0i32, 0i32, 0i32, 0i32);
    if px <= out_w / 2 { a |= 4; left = px; } else { a |= 8; right = out_w - px - ww; }
    if py <= out_h / 2 { a |= 1; top = py; } else { a |= 2; bottom = out_h - py - wh; }
    (a, (top, right, bottom, left))
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

        // 等 output 尺寸（--pos 定位需要）
        let out_start = Instant::now();
        while (state.output_w == 0 || state.output_h == 0) && out_start.elapsed() < Duration::from_secs(3) {
            let fd = conn.backend().poll_fd().as_raw_fd();
            let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            unsafe { libc::poll(&mut pfd, 1, 50); }
            if pfd.revents & libc::POLLIN != 0 {
                if let Some(guard) = conn.prepare_read() { let _ = guard.read(); }
            }
            conn.flush().ok();
            let _ = queue.dispatch_pending(&mut state);
        }
        if state.output_w == 0 { state.output_w = 1920; }
        if state.output_h == 0 { state.output_h = 1080; }

        // ===== 媒体准备 =====
        let fonts = crate::fonts::load_fonts().map_err(|e| e.to_string())?;
        let (w, h) = (spec.size.w as i32, spec.size.h as i32);
        let stride = (spec.size.w * 4) as i32;
        let frame_bytes = (spec.size.w * spec.size.h * 4) as usize;

        // 音频（mpv 子进程，弹窗结束自动 kill）
        let _audio = AudioGuard::from_spec(spec);

        // GIF 帧
        let gif_frames = if let Some(p) = &spec.image {
            if p.to_lowercase().ends_with(".gif") { Some(load_gif(p)?) } else { None }
        } else { None };

        // 视频流式管道
        let mut video = if let Some(v) = &spec.video {
            Some(spawn_video_stream(v, spec.size.w, spec.size.h, 15).map_err(|e| e)?)
        } else { None };
        let mut video_frame = vec![0u8; frame_bytes];
        // 首次尝试读一帧（视频可能未就绪，失败则用默认）
        if let Some(vs) = &mut video {
            let _ = try_read_video_frame(vs, &mut video_frame);
        }

        // ===== 渲染首帧 + 3 buffer =====
        let media = if let Some(vf) = &video { if video_frame.iter().any(|&b| b != 0) { Some(MediaFrame::Video(&video_frame)) } else { None } }
            else if let Some(gf) = &gif_frames { Some(MediaFrame::Gif(&gf[0])) } else { None };
        let popup = render_popup(spec, &fonts, media, 0)?;
        let btn_rect = popup.btn_rect;
        let body_rect = popup.body_rect;

        let buf_size = popup.data.len() as i64;
        const N_BUFS: i64 = 3;
        let total_size = buf_size * N_BUFS;
        let name = b"adpop-shm\0";
        let fd = unsafe { libc::memfd_create(name.as_ptr() as *const i8, libc::MFD_CLOEXEC) };
        if fd < 0 { return Err("memfd_create 失败".to_string()); }
        unsafe { libc::ftruncate(fd, total_size); }
        let file = unsafe { File::from_raw_fd(fd) };
        let mut mmap = unsafe { MmapOptions::new().len(total_size as usize).map_mut(&file) }
            .map_err(|e| format!("mmap 失败: {e}"))?;
        mmap[..buf_size as usize].copy_from_slice(&popup.data);

        let pool = shm.create_pool(unsafe { BorrowedFd::borrow_raw(fd) }, total_size as i32, &qh, ());
        let buffers: Vec<wl_buffer::WlBuffer> = (0..N_BUFS)
            .map(|i| pool.create_buffer((i * buf_size) as i32, w, h, stride, wl_shm::Format::Xrgb8888, &qh, ()))
            .collect();

        // ===== layer surface =====
        let surface = comp.create_surface(&qh, ());
        let layer_surface = shell.get_layer_surface(&surface, None, zwlr_layer_shell_v1::Layer::Top, "adpop".to_string(), &qh, ());
        if let Some(pos) = spec.pos {
            let (ab, (t, r, b, l)) = pos_to_anchor_margin(pos.x, pos.y, w, h, state.output_w as i32, state.output_h as i32);
            layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::from_bits(ab).unwrap_or(zwlr_layer_surface_v1::Anchor::Right | zwlr_layer_surface_v1::Anchor::Bottom));
            layer_surface.set_margin(t, r, b, l);
        } else {
            layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::Right | zwlr_layer_surface_v1::Anchor::Bottom);
            layer_surface.set_margin(24, 24, 24, 24);
        }
        layer_surface.set_size(w as u32, h as u32);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);

        // 空 commit 触发 initial configure（Smithay/niri 强制）
        surface.commit();
        conn.flush().map_err(|e| format!("flush: {e}"))?;

        let start = Instant::now();
        let deadline = if spec.duration == 0 { None } else { Some(start + Duration::from_secs(spec.duration)) };

        loop {
            if !state.shown {
                if let Some(serial) = state.configure_serial.take() {
                    layer_surface.ack_configure(serial);
                    surface.attach(Some(&buffers[0]), 0, 0);
                    surface.damage(0, 0, w, h);
                    surface.commit();
                    conn.flush().ok();
                    state.shown = true;
                }
            } else {
                let t = Instant::now() - start;
                // 读视频帧（检测 ffmpeg 无输出：子进程已退出且从未读到帧 → 视频失败）
                if let Some(vs) = &mut video {
                    let ffmpeg_done = vs.child.try_wait().map(|s| s.is_some()).unwrap_or(false);
                    if ffmpeg_done && t.as_secs() >= 2 {
                        return Err("视频解码失败: ffmpeg 无输出（文件损坏或解码失败）".to_string());
                    }
                    if try_read_video_frame(vs, &mut video_frame) {
                        let media = MediaFrame::Video(&video_frame);
                        if let Ok(p) = render_popup(spec, &fonts, Some(media), t.as_millis() as u64) {
                            let bi = ((t.as_millis() as u64) / 100) as usize % buffers.len();
                            let off = (bi as i64 * buf_size) as usize;
                            mmap[off..off + buf_size as usize].copy_from_slice(&p.data);
                            surface.attach(Some(&buffers[bi]), 0, 0);
                            surface.damage(0, 0, w, h);
                            surface.commit();
                            conn.flush().ok();
                        }
                    }
                } else if let Some(gfs) = &gif_frames {
                    // GIF 帧循环（每 100ms 检查帧切换）
                    let bi = ((t.as_millis() as u64) / 100) as usize % buffers.len();
                    let media = MediaFrame::Gif(&gfs[0]);
                    if let Ok(p) = render_popup(spec, &fonts, Some(media), t.as_millis() as u64) {
                        let off = (bi as i64 * buf_size) as usize;
                        mmap[off..off + buf_size as usize].copy_from_slice(&p.data);
                        surface.attach(Some(&buffers[bi]), 0, 0);
                        surface.damage(0, 0, w, h);
                        surface.commit();
                        conn.flush().ok();
                    }
                }

                // 点击处理：关闭按钮 → Closed；可点击区域（--url）→ 跳转
                if let Some((x, y)) = state.clicked.take() {
                    let (xi, yi) = (x as i32, y as i32);
                    if hit_close(xi, yi, spec.size.w) {
                        if !spec.no_close { return Ok(CloseReason::Closed); }
                    } else if let Some(url) = &spec.url {
                        let in_zone = match spec.click_zone {
                            ClickZone::All => xi >= 0 && yi >= TITLEBAR_H as i32 && xi < spec.size.w as i32 && yi < spec.size.h as i32,
                            ClickZone::Button => btn_rect.map(|(bx, by, bw, bh)| xi >= bx && xi < bx + bw && yi >= by && yi < by + bh).unwrap_or(false),
                            ClickZone::Body => body_rect.map(|(bx, by, bw, bh)| xi >= bx && xi < bx + bw && yi >= by && yi < by + bh).unwrap_or(false),
                            ClickZone::None => false,
                        };
                        if in_zone {
                            eprintln!("adpop: 点击广告，打开 {url}");
                            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                            return Ok(CloseReason::Jumped);
                        }
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

use crate::render::TITLEBAR_H;
