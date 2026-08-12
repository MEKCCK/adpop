use image::{DynamicImage, GenericImageView};
use crate::fonts::{rasterize_line, text_width, Fonts};
use crate::spec::PopupSpec;

pub struct PopupPixels { pub w: u32, pub h: u32, pub data: Vec<u8> }

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

fn draw_close_x(data: &mut [u8], w: u32, _h: u32, rect: (u32, u32, u32, u32)) {
    let (x0, y0, rw, rh) = rect;
    let white = [0xFF, 0xFF, 0xFF, 0xFF];
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

pub fn render_popup(spec: &PopupSpec, fonts: &Fonts) -> Result<PopupPixels, String> {
    let w = spec.size.w;
    let h = spec.size.h;
    let mut data = vec![0xFFu8; (w * h * 4) as usize]; // 默认白

    // 标题栏
    fill_rect(&mut data, w, h, 0, 0, w, TITLEBAR_H, [0xF0, 0xF0, 0xF0, 0xFF]);

    // 标题文字（深灰）
    let (tw, th, tbitmap) = rasterize_line(&fonts.regular, 14.0, &spec.title);
    blend_text(&mut data, w, h, MARGIN_X, (TITLEBAR_H.saturating_sub(th)) / 2, &tbitmap, tw, th, [0x33, 0x33, 0x33, 0xFF]);

    // 关闭按钮（红底 + 白×）
    let close = (w - CLOSE_W, 0u32, CLOSE_W, TITLEBAR_H);
    fill_rect(&mut data, w, h, close.0, close.1, close.2, close.3, [0xE8, 0x50, 0x50, 0xFF]);
    draw_close_x(&mut data, w, h, close);

    // 正文区域
    let body_top = TITLEBAR_H + 6;
    let body_bottom = h.saturating_sub(8);
    let body_w = w - MARGIN_X * 2;

    let mut cursor_y = body_top;

    // 图片
    if let Some(path) = &spec.image {
        let img = image::open(path).map_err(|e| format!("图片加载失败: {path}: {e}"))?;
        let (nw, nh) = scale_image(&img, body_w);
        let img = img.resize_exact(nw, nh, image::imageops::FilterType::Triangle);
        let max_ih = body_bottom.saturating_sub(cursor_y + 4);
        let ih = nh.min(max_ih);
        for y in 0..ih {
            for x in 0..nw {
                let p = img.get_pixel(x, y);
                let i = (((cursor_y + y) * w + (MARGIN_X + x)) * 4) as usize;
                data[i] = p[2];     // B
                data[i + 1] = p[1]; // G
                data[i + 2] = p[0]; // R
                data[i + 3] = 0xFF;
            }
        }
        cursor_y += ih + 4;
    }

    // 文字逐行
    let mut remaining = &spec.text[..];
    while !remaining.is_empty() && cursor_y + 14 <= body_bottom {
        let (line, rest) = match remaining.find('\n') {
            Some(i) => remaining.split_at(i + 1),
            None => (remaining, ""),
        };
        let line = line.trim_end_matches('\n');
        if !line.is_empty() {
            let (lw, lh, bitmap) = rasterize_line(&fonts.regular, 14.0, line);
            if text_width(&fonts.regular, 14.0, line) > body_w && lw > body_w {
                // 超宽截断（简单粗暴：按字符截断到 body_w）
                let mut cut = String::new();
                for c in line.chars() {
                    if text_width(&fonts.regular, 14.0, &format!("{cut}{c}")) > body_w { break; }
                    cut.push(c);
                }
                let (_, lh2, bitmap2) = rasterize_line(&fonts.regular, 14.0, &cut);
                blend_text(&mut data, w, h, MARGIN_X, cursor_y, &bitmap2, text_width(&fonts.regular, 14.0, &cut), lh2, [0x00, 0x00, 0x00, 0xFF]);
            } else {
                blend_text(&mut data, w, h, MARGIN_X, cursor_y, &bitmap, lw, lh, [0x00, 0x00, 0x00, 0xFF]);
            }
        }
        cursor_y += 16;
        remaining = rest;
    }

    Ok(PopupPixels { w, h, data })
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
        let p = render_popup(&sample_spec(), &fonts).unwrap();
        assert_eq!(p.w, 320);
        assert_eq!(p.h, 220);
        assert_eq!(p.data.len(), (320 * 220 * 4) as usize);
    }

    #[test]
    fn titlebar_is_gray() {
        let fonts = load_fonts().unwrap();
        let p = render_popup(&sample_spec(), &fonts).unwrap();
        let i = 0; // (0,0) 标题栏
        assert_eq!(&p.data[i..i + 4], &[0xF0, 0xF0, 0xF0, 0xFF]);
    }

    #[test]
    fn close_button_red() {
        let fonts = load_fonts().unwrap();
        let p = render_popup(&sample_spec(), &fonts).unwrap();
        let i = ((0 * p.w + (p.w - 16)) * 4) as usize; // 右上角中部
        assert_eq!(&p.data[i..i + 4], &[0xE8, 0x50, 0x50, 0xFF]);
    }

    #[test]
    fn close_x_is_white_diagonal() {
        // 偏离 brief：原实现画的是顶部实心横条而非 ×；此处断言×的四个方向中心像素为白
        let fonts = load_fonts().unwrap();
        let p = render_popup(&sample_spec(), &fonts).unwrap();
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
        assert_eq!(at(w - 32 + 1, 1), [0xE8, 0x50, 0x50, 0xFF]);
        assert_eq!(at(w - 1, 23), [0xE8, 0x50, 0x50, 0xFF]);
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
        assert!(render_popup(&spec, &fonts).is_err());
    }
}

