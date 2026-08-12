//! X11 后端：override-redirect 无边框置顶窗口 + PutImage 渲染 + 事件循环。
//!
//! 像素数据为 XRGB8888 小端（[B,G,R,X]），x86_64 TrueColor24 假设（见 render.rs）。
//! 注意：XWayland 下根窗口是逻辑屏幕（跨多输出），坐标按 X 报告值。

use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use crate::behavior::{hit_close, screen_pos};
use crate::media::{load_gif, spawn_video_stream, try_read_video_frame, AudioGuard, GifFrame};
use crate::render::{render_popup, MediaFrame, TITLEBAR_H};
use crate::spec::{ClickZone, CloseReason, PopupSpec};
use super::PopupBackend;

pub struct X11Backend;

impl X11Backend {
    pub fn new() -> Self { X11Backend }
}

impl PopupBackend for X11Backend {
    fn show(&mut self, spec: &PopupSpec) -> Result<CloseReason, String> {
        let (conn, screen_num) = x11rb::connect(None).map_err(|e| format!("X11 连接失败: {e}"))?;
        let screen = &conn.setup().roots[screen_num];
        let (sw, sh) = (screen.width_in_pixels as i32, screen.height_in_pixels as i32);

        let (w, h) = (spec.size.w as u16, spec.size.h as u16);
        let (sx, sy) = if let Some(pos) = spec.pos {
            (pos.x, pos.y)
        } else {
            screen_pos(&spec.corner, spec.size.w, spec.size.h, sw as u32, sh as u32, 0)
        };

        let win = conn.generate_id().map_err(|e| format!("generate_id: {e}"))?;
        let attrs = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(screen.white_pixel)
            .event_mask(EventMask::EXPOSURE | EventMask::BUTTON_PRESS);
        conn.create_window(24, win, screen.root, sx as i16, sy as i16, w, h, 0,
            WindowClass::INPUT_OUTPUT, 0, &attrs)
            .map_err(|e| format!("create_window: {e}"))?;

        // ===== 媒体准备（弹窗结束自动杀 mpv） =====
        let _audio = AudioGuard::from_spec(spec);
        let fonts = crate::fonts::load_fonts().map_err(|e| e.to_string())?;
        let frame_bytes = (spec.size.w * spec.size.h * 4) as usize;
        let gif_frames = if let Some(p) = &spec.image {
            if p.to_lowercase().ends_with(".gif") { Some(load_gif(p)?) } else { None }
        } else { None };
        let mut video = if let Some(v) = &spec.video {
            Some(spawn_video_stream(v, spec.size.w, spec.size.h, 15).map_err(|e| e)?)
        } else { None };
        let mut video_frame = vec![0u8; frame_bytes];
        if let Some(vs) = &mut video { let _ = try_read_video_frame(vs, &mut video_frame); }

        // ===== 首帧渲染 =====
        let media = if let Some(vf) = &video { if video_frame.iter().any(|&b| b != 0) { Some(MediaFrame::Video(&video_frame)) } else { None } }
            else if let Some(gf) = &gif_frames { Some(MediaFrame::Gif(&gf[0])) } else { None };
        let popup = render_popup(spec, &fonts, media, 0)?;
        let btn_rect = popup.btn_rect;
        let body_rect = popup.body_rect;

        let gc = conn.generate_id().map_err(|e| format!("generate_id gc: {e}"))?;
        conn.create_gc(gc, win, &CreateGCAux::new().foreground(screen.black_pixel))
            .map_err(|e| format!("create_gc: {e}"))?;
        conn.put_image(ImageFormat::Z_PIXMAP, win, gc, w, h, 0, 0, 0, 24, &popup.data)
            .map_err(|e| format!("put_image: {e}"))?;
        conn.map_window(win).map_err(|e| format!("map_window: {e}"))?;
        conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))
            .map_err(|e| format!("configure_window: {e}"))?;
        conn.flush().map_err(|e| format!("flush: {e}"))?;

        // ===== 事件循环：Expose 重绘 / 媒体帧 / ButtonPress 命中 / 超时 =====
        let start = Instant::now();
        let deadline = if spec.duration == 0 { None } else { Some(start + Duration::from_secs(spec.duration)) };
        loop {
            let t = Instant::now() - start;

            // 媒体帧/动画重绘（每 80ms 检查一次新帧）
            let mut repaint = false;
            if let Some(vs) = &mut video {
                let ffmpeg_done = vs.child.try_wait().map(|s| s.is_some()).unwrap_or(false);
                if ffmpeg_done && t.as_secs() >= 2 {
                    return Err("视频解码失败: ffmpeg 无输出（文件损坏或解码失败）".to_string());
                }
                if try_read_video_frame(vs, &mut video_frame) {
                    repaint = true;
                }
            }
            if repaint || gif_frames.is_some() || spec.animate != crate::spec::Animate::None {
                let media = if let Some(vf) = &video { Some(MediaFrame::Video(&video_frame)) }
                    else if let Some(gf) = &gif_frames { Some(MediaFrame::Gif(&gf[0])) } else { None };
                if let Ok(p) = render_popup(spec, &fonts, media, t.as_millis() as u64) {
                    conn.put_image(ImageFormat::Z_PIXMAP, win, gc, w, h, 0, 0, 0, 24, &p.data)
                        .map_err(|e| format!("put_image frame: {e}"))?;
                    conn.flush().ok();
                }
            }

            match conn.poll_for_event() {
                Ok(Some(Event::Expose(_))) => {
                    let media = if let Some(vf) = &video { Some(MediaFrame::Video(&video_frame)) }
                        else if let Some(gf) = &gif_frames { Some(MediaFrame::Gif(&gf[0])) } else { None };
                    if let Ok(p) = render_popup(spec, &fonts, media, t.as_millis() as u64) {
                        conn.put_image(ImageFormat::Z_PIXMAP, win, gc, w, h, 0, 0, 0, 24, &p.data)
                            .map_err(|e| format!("put_image expose: {e}"))?;
                        conn.flush().ok();
                    }
                }
                Ok(Some(Event::ButtonPress(ev))) => {
                    if ev.detail == 1 {
                        let (xi, yi) = (ev.event_x as i32, ev.event_y as i32);
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
                }
                Ok(Some(Event::Error(e))) => return Err(format!("X11 协议错误: {e:?}")),
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => return Err(format!("X11 事件循环: {e}")),
            }

            if let Some(d) = deadline {
                if Instant::now() >= d { return Ok(CloseReason::TimedOut); }
            }
        }
    }
}
