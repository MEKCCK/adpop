use image::{DynamicImage, GenericImageView};
use crate::fonts::{rasterize_line, text_width, Fonts};
use crate::media::GifFrame;
use crate::spec::{ClickZone, PopupSpec};

pub struct PopupPixels { pub w: u32, pub h: u32, pub data: Vec<u8> }

/// 弹窗渲染结果：像素 + 可点击区域（供点击跳转命中检测）
pub struct PopupRender {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
    /// 按钮区域 (x, y, w, h)，仅 --url + click_zone!=None 时 Some
    pub btn_rect: Option<(i32, i32, i32, i32)>,
    /// 正文区域 (x, y, w, h)，标题栏下方到按钮上方
    pub body_rect: Option<(i32, i32, i32, i32)>,
}

/// 媒体帧来源：视频帧（bgr0 = XRGB8888 小端字节序，直接拷贝）或 GIF 当前帧
pub enum MediaFrame<'a> {
    Video(&'a [u8]),
    Gif(&'a GifFrame),
}

pub fn scale_image(img: &DynamicImage, target_w: u32) -> (u32, u32) {
    let (iw, ih) = (img.width(), img.height());
    if iw == 0 || ih == 0 { return (0, 0); }
    let scale = target_w as f32 / iw as f32;
    let nw = target_w;
    let nh = ((ih as f32) * scale).round() as u32;
    (nw.max(1), nh.max(1))
}

fn px(data: &mut [u8], w: u32, x: u32, y: u32, color: [u8; 4]) {
    let i = ((y * w + x) * 4) as usize;
    data[i..i + 4].copy_from_slice(&color);
}

fn fill_rect(data: &mut [u8], w: u32, h: u32, x0: u32, y0: u32, rw: u32, rh: u32, color: [u8; 4]) {
    for y in y0..(y0 + rh).min(h) {
        for x in x0..(x0 + rw).min(w) {
            px(data, w, x, y, color);
        }
    }
}

fn blend_text(data: &mut [u8], w: u32, h: u32, x0: u32, y0: u32, glyph: &[u8], gw: u32, gh: u32, color: [u8; 4]) {
    for gy in 0..gh {
        for gx in 0..gw {
            let a = glyph[(gy * gw + gx) as usize];
            if a == 0 { continue; }
            let x = x0 + gx;
            let y = y0 + gy;
            if x >= w || y >= h { continue; }
            let i = ((y * w + x) * 4) as usize;
            data[i] = color[0];
            data[i + 1] = color[1];
            data[i + 2] = color[2];
            data[i + 3] = 0xFF;
        }
    }
}

/// 把 RGBA 图像绘制到 XRGB8888 buffer（缩放到目标尺寸）
fn draw_rgba(data: &mut [u8], w: u32, h: u32, img: &image::RgbaImage, dst_w: u32, dst_h: u32) {
    let (iw, ih) = (img.width(), img.height());
    if iw == 0 || ih == 0 { return; }
    for y in 0..dst_h {
        for x in 0..dst_w {
            let sx = (x * iw) / dst_w;
            let sy = (y * ih) / dst_h;
            let p = img.get_pixel(sx, sy);
            let i = ((y * w + x) * 4) as usize;
            if i + 3 < data.len() {
                data[i] = p[2]; data[i + 1] = p[1]; data[i + 2] = p[0]; data[i + 3] = 0xFF;
            }
        }
    }
}

fn draw_close_x(data: &mut [u8], w: u32, _h: u32, rect: (u32, u32, u32, u32)) {
    let (x0, y0, rw, rh) = rect;
    let white = CLOSE_X_COLOR;
    let cx = x0 + rw / 2;
    let cy = y0 + rh / 2;
    // 中央白色 ×：两条 3px 粗对角线（半臂长 6px），中心在按钮正中
    let half = 6u32;
    for t in 0..3u32 {
        for d in 0..=half * 2 {
            px(data, w, cx - half + d, cy - half + d + t, white); // ↘
            px(data, w, cx + half - d, cy - half + d + t, white); // ↙
        }
    }
}

pub const TITLEBAR_H: u32 = 24;
pub const CLOSE_W: u32 = 32;
pub const MARGIN_X: u32 = 8;

// XRGB8888 小端：32-bit 值 = 0xXXRRGGBB，小端内存字节序为 [B,G,R,X]，
// 故颜色字面量一律按 [B,G,R,0xFF] 书写（与图片路径 data[i]=B 的写入一致）。
const BG_COLOR: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF]; // 白底（对称色）
const TITLEBAR_COLOR: [u8; 4] = [0xF0, 0xF0, 0xF0, 0xFF]; // 灰标题栏（对称色）
const TITLE_TEXT_COLOR: [u8; 4] = [0x33, 0x33, 0x33, 0xFF]; // 深灰标题（对称色）
const BODY_TEXT_COLOR: [u8; 4] = [0x00, 0x00, 0x00, 0xFF]; // 黑正文（对称色）
const CLOSE_X_COLOR: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF]; // 白 ×（对称色）
const CLOSE_BTN_COLOR: [u8; 4] = [0x50, 0x50, 0xE8, 0xFF]; // 红色按钮底：RGB(0xE8,0x50,0x50) 按小端内存 B,G,R,X 排布
const AD_BTN_COLOR: [u8; 4] = [0x20, 0x50, 0xE8, 0xFF];   // 广告按钮红：RGB(0xE8,0x50,0x20)
const AD_BTN_COLOR_FLASH: [u8; 4] = [0x18, 0xC5, 0xF5, 0xFF]; // flash 时黄：RGB(0xF5,0xC5,0x18)
const AD_BTN_TEXT: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const MARQUEE_COLOR: [u8; 4] = [0x30, 0x30, 0xE0, 0xFF];   // 跑马灯红：RGB(0xE0,0x30,0x30)
pub const AD_BTN_H: u32 = 40;      // 广告按钮高
pub const AD_BTN_MARGIN: u32 = 10; // 按钮左右留白

pub fn render_popup(
    spec: &PopupSpec,
    fonts: &Fonts,
    media: Option<MediaFrame<'_>>,
    t_ms: u64,
) -> Result<PopupRender, String> {
    let w = spec.size.w;
    let h = spec.size.h;
    let mut data = vec![0xFFu8; (w * h * 4) as usize]; // 默认白
    let f = &fonts.regular;

    // ===== 背景：媒体帧（视频/GIF）或纯白 =====
    match media {
        Some(MediaFrame::Video(vf)) => {
            let len = (w * h * 4) as usize;
            data[..len.min(vf.len())].copy_from_slice(&vf[..len.min(vf.len())]);
        }
        Some(MediaFrame::Gif(gf)) => {
            draw_rgba(&mut data, w, h, &gf.img, w, h);
        }
        None => {}
    }

    // ===== flash 动画：配色交替（600ms） =====
    let flash_on = spec.animate == crate::spec::Animate::Flash && (t_ms / 600) % 2 == 1;
    let ad_btn_color = if flash_on { AD_BTN_COLOR_FLASH } else { AD_BTN_COLOR };

    // ===== 标题栏（半透明黑条压在媒体帧上保证可读） =====
    if media.is_some() {
        // 媒体帧模式：顶部压黑条
        for y in 0..TITLEBAR_H {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if i + 3 < data.len() {
                    data[i] = (data[i] as u32 * 4 / 5) as u8;
                    data[i + 1] = (data[i + 1] as u32 * 4 / 5) as u8;
                    data[i + 2] = (data[i + 2] as u32 * 4 / 5) as u8;
                }
            }
        }
    } else {
        fill_rect(&mut data, w, h, 0, 0, w, TITLEBAR_H, TITLEBAR_COLOR);
    }

    // 标题文字
    let (tw, th, tbitmap) = rasterize_line(f, 14.0, &spec.title);
    blend_text(&mut data, w, h, MARGIN_X, (TITLEBAR_H.saturating_sub(th)) / 2, &tbitmap, tw, th, TITLE_TEXT_COLOR);

    // 关闭按钮（红底 + 白×）
    let close = (w.saturating_sub(CLOSE_W), 0u32, CLOSE_W, TITLEBAR_H);
    fill_rect(&mut data, w, h, close.0, close.1, close.2, close.3, CLOSE_BTN_COLOR);
    draw_close_x(&mut data, w, h, close);

    // ===== 正文区域与按钮布局 =====
    let has_btn = spec.url.is_some() && spec.click_zone != ClickZone::None;
    let btn_y = if has_btn { h.saturating_sub(AD_BTN_H + 8) } else { h };
    let body_top = TITLEBAR_H + 6;
    let body_bottom = btn_y.saturating_sub(4);
    let body_w = w.saturating_sub(MARGIN_X * 2);

    // 广告按钮（--url + click_zone 非 none 时）
    let btn_rect = if has_btn {
        let (bx, by, bw, bh) = (AD_BTN_MARGIN as u32, btn_y, w - AD_BTN_MARGIN * 2, AD_BTN_H);
        fill_rect(&mut data, w, h, bx, by, bw, bh, ad_btn_color);
        let label = "立即领取";
        let (lw, _) = text_size(f, 16.0, label);
        let (_, lh, lbmp) = rasterize_line(f, 16.0, label);
        blend_text(&mut data, w, h, (w - lw) / 2, by + (bh.saturating_sub(lh)) / 2, &lbmp, lw, lh, AD_BTN_TEXT);
        Some((bx as i32, by as i32, bw as i32, bh as i32))
    } else { None };

    // ===== marquee 跑马灯（按钮上方一行，从右向左滚动） =====
    let marquee_text = "🔥 限时特惠 · 点击领取 · 先到先得 · ";
    let marquee_text = marquee_text.trim_start_matches("🔥 ");
    if spec.animate == crate::spec::Animate::Marquee && has_btn && body_bottom > body_top + 20 {
        let (mw, mh, mbmp) = rasterize_line(f, 13.0, marquee_text);
        let span = (w + mw) as u64;
        let sx = ((w as i64) - ((t_ms / 8) % span) as i64).rem_euclid(span as i64) - (mw as i64);
        blend_text(&mut data, w, h, sx.max(0) as u32, body_bottom - mh - 2, &mbmp, mw, mh, MARQUEE_COLOR);
    }

    // 正文：图片 + 文字（媒体帧模式下跳过图片区域，文字压在上部）
    let mut cursor_y = body_top;
    if media.is_none() {
        if let Some(path) = &spec.image {
            let img = image::open(path).map_err(|e| format!("图片加载失败: {path}: {e}"))?;
            let (nw, nh) = scale_image(&img, body_w);
            let img = img.resize_exact(nw, nh, image::imageops::FilterType::Triangle);
            let max_ih = body_bottom.saturating_sub(cursor_y + 4);
            let ih = nh.min(max_ih);
            draw_rgba(&mut data, w, h, &img.to_rgba8(), nw, ih);
            cursor_y += ih + 4;
        }
    }

    let mut remaining = &spec.text[..];
    while !remaining.is_empty() && cursor_y + 14 <= body_bottom {
        let (line, rest) = match remaining.find('\n') {
            Some(i) => remaining.split_at(i + 1),
            None => (remaining, ""),
        };
        let line = line.trim_end_matches('\n');
        if !line.is_empty() {
            let (lw, lh, bitmap) = rasterize_line(f, 14.0, line);
            if text_width(f, 14.0, line) > body_w && lw > body_w {
                let mut cut = String::new();
                for c in line.chars() {
                    if text_width(f, 14.0, &format!("{cut}{c}")) > body_w { break; }
                    cut.push(c);
                }
                let (_, lh2, bitmap2) = rasterize_line(f, 14.0, &cut);
                blend_text(&mut data, w, h, MARGIN_X, cursor_y, &bitmap2, text_width(f, 14.0, &cut), lh2, BODY_TEXT_COLOR);
            } else {
                blend_text(&mut data, w, h, MARGIN_X, cursor_y, &bitmap, lw, lh, BODY_TEXT_COLOR);
            }
        }
        cursor_y += 16;
        remaining = rest;
    }

    // 点击区域
    let body_rect = if has_btn {
        Some((0, TITLEBAR_H as i32, w as i32, (btn_y - TITLEBAR_H) as i32))
    } else if spec.url.is_some() && spec.click_zone != ClickZone::None {
        Some((0, TITLEBAR_H as i32, w as i32, (h - TITLEBAR_H) as i32))
    } else { None };

    Ok(PopupRender { w, h, data, btn_rect, body_rect })
}

fn text_size(font: &fontdue::Font, px: f32, s: &str) -> (u32, u32) {
    let w: u32 = s.chars().map(|c| font.rasterize(c, px).0.advance_width.ceil() as u32).sum();
    (w, px.ceil() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::load_fonts;
    use crate::spec::Size;

    fn sample_spec() -> PopupSpec {
        let mut s = PopupSpec::default();
        s.text = "恭喜你中奖了！".to_string();
        s.size = Size { w: 320, h: 220 };
        s
    }

    #[test]
    fn buffer_size_matches() {
        let fonts = load_fonts().unwrap();
        let p = render_popup(&sample_spec(), &fonts, None, 0).unwrap();
        assert_eq!(p.w, 320);
        assert_eq!(p.h, 220);
        assert_eq!(p.data.len(), (320 * 220 * 4) as usize);
    }

    #[test]
    fn titlebar_is_gray() {
        let fonts = load_fonts().unwrap();
        let p = render_popup(&sample_spec(), &fonts, None, 0).unwrap();
        let i = 0; // (0,0) 标题栏
        assert_eq!(&p.data[i..i + 4], &[0xF0, 0xF0, 0xF0, 0xFF]);
    }

    #[test]
    fn close_button_red() {
        // XRGB8888 小端：内存字节序 B,G,R,X → RGB(0xE8,0x50,0x50) 红在内存为 [0x50,0x50,0xE8,0xFF]
        let fonts = load_fonts().unwrap();
        let p = render_popup(&sample_spec(), &fonts, None, 0).unwrap();
        let i = ((0 * p.w + (p.w - 16)) * 4) as usize; // 右上角中部
        assert_eq!(&p.data[i..i + 4], &CLOSE_BTN_COLOR);
    }

    #[test]
    fn close_x_is_white_diagonal() {
        // 偏离 brief：原实现画的是顶部实心横条而非 ×；此处断言×的四个方向中心像素为白
        let fonts = load_fonts().unwrap();
        let p = render_popup(&sample_spec(), &fonts, None, 0).unwrap();
        let w = p.w;
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * w + x) * 4) as usize;
            p.data[i..i + 4].try_into().unwrap()
        };
        assert_eq!(at(w - 16, 12), [0xFF, 0xFF, 0xFF, 0xFF], "× 中心应为白");
        assert_eq!(at(w - 16 + 4, 8), [0xFF, 0xFF, 0xFF, 0xFF], "× 左上臂应为白");
        assert_eq!(at(w - 16 - 4, 8), [0xFF, 0xFF, 0xFF, 0xFF], "× 右上臂应为白");
        assert_eq!(at(w - 16 - 4, 16), [0xFF, 0xFF, 0xFF, 0xFF], "× 左下臂应为白");
        assert_eq!(at(w - 16 + 4, 16), [0xFF, 0xFF, 0xFF, 0xFF], "× 右下臂应为白");
        // 按钮四角仍为红底
        assert_eq!(at(w - 32 + 1, 1), CLOSE_BTN_COLOR);
        assert_eq!(at(w - 1, 23), CLOSE_BTN_COLOR);
    }

    #[test]
    fn narrow_popup_no_underflow() {
        // w < CLOSE_W(32) 时不得 u32 下溢（回归：w - CLOSE_W 曾 panic）
        let fonts = load_fonts().unwrap();
        let mut spec = sample_spec();
        spec.size = Size { w: 16, h: 48 };
        let p = render_popup(&spec, &fonts, None, 0).unwrap();
        assert_eq!(p.w, 16);
        assert_eq!(p.h, 48);
        assert_eq!(p.data.len(), (16 * 48 * 4) as usize);
    }

    #[test]
    fn scale_image_proportional() {
        let img = image::RgbaImage::from_pixel(200, 100, image::Rgba([0, 0, 0, 255]));
        let (nw, nh) = scale_image(&image::DynamicImage::ImageRgba8(img), 100);
        assert_eq!(nw, 100);
        assert_eq!(nh, 50);
    }

    #[test]
    fn missing_image_errors() {
        let fonts = load_fonts().unwrap();
        let mut spec = sample_spec();
        spec.image = Some("/nonexistent/xyz.png".to_string());
        assert!(render_popup(&spec, &fonts, None, 0).is_err());
    }
}

