//! X11 后端：override-redirect 无边框置顶窗口 + PutImage 渲染 + 事件循环。
//!
//! 像素数据为 XRGB8888 小端（[B,G,R,X]），x86_64 TrueColor24 假设，见 render.rs。

use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;

use crate::behavior::{hit_close, screen_pos};
use crate::render::render_popup;
use crate::spec::{CloseReason, PopupSpec};

use super::PopupBackend;

pub struct X11Backend;

impl X11Backend {
    pub fn new() -> Self {
        X11Backend
    }
}

impl PopupBackend for X11Backend {
    fn show(&mut self, spec: &PopupSpec) -> Result<CloseReason, String> {
        let (conn, screen_num) = x11rb::connect(None).map_err(|e| format!("X11 连接失败: {e}"))?;
        let screen = &conn.setup().roots[screen_num];

        let (w, h) = (spec.size.w as u16, spec.size.h as u16);
        let (sx, sy) = screen_pos(
            &spec.corner,
            spec.size.w,
            spec.size.h,
            screen.width_in_pixels as u32,
            screen.height_in_pixels as u32,
            0,
        );

        let win = conn.generate_id().map_err(|e| format!("generate_id: {e}"))?;
        let attrs = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(screen.white_pixel)
            .event_mask(EventMask::EXPOSURE | EventMask::BUTTON_PRESS);
        conn.create_window(
            24,
            win,
            screen.root,
            sx as i16,
            sy as i16,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &attrs,
        )
        .map_err(|e| format!("create_window: {e}"))?;

        // 渲染像素
        let fonts = crate::fonts::load_fonts().map_err(|e| e.to_string())?;
        let popup = render_popup(spec, &fonts).map_err(|e| e.to_string())?;

        // GC + PutImage（ZPixmap depth 24，小端 XRGB8888）
        let gc = conn.generate_id().map_err(|e| format!("generate_id gc: {e}"))?;
        conn.create_gc(gc, win, &CreateGCAux::new().foreground(screen.black_pixel))
            .map_err(|e| format!("create_gc: {e}"))?;
        conn.put_image(ImageFormat::Z_PIXMAP, win, gc, w, h, 0, 0, 0, 24, &popup.data)
            .map_err(|e| format!("put_image: {e}"))?;

        conn.map_window(win).map_err(|e| format!("map_window: {e}"))?;
        conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))
            .map_err(|e| format!("configure_window: {e}"))?;
        conn.flush().map_err(|e| format!("flush: {e}"))?;

        // 事件循环：Expose 重绘 / ButtonPress 命中 / 超时
        let start = Instant::now();
        let deadline = if spec.duration == 0 {
            None
        } else {
            Some(start + Duration::from_secs(spec.duration))
        };
        loop {
            if let Some(d) = deadline {
                if Instant::now() >= d {
                    return Ok(CloseReason::TimedOut);
                }
            }
            match conn.poll_for_event() {
                Ok(Some(Event::Expose(_))) => {
                    conn.put_image(ImageFormat::Z_PIXMAP, win, gc, w, h, 0, 0, 0, 24, &popup.data)
                        .map_err(|e| format!("put_image expose: {e}"))?;
                    conn.flush().ok();
                }
                Ok(Some(Event::ButtonPress(ev))) => {
                    if ev.detail == 1
                        && hit_close(ev.event_x as i32, ev.event_y as i32, spec.size.w)
                    {
                        if !spec.no_close {
                            return Ok(CloseReason::Closed);
                        }
                    }
                }
                Ok(Some(Event::Error(e))) => {
                    return Err(format!("X11 协议错误: {e:?}"));
                }
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => return Err(format!("X11 事件循环: {e}")),
            }
        }
    }
}
