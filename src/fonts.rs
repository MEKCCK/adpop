use fontdue::{Font, FontSettings};

pub struct Fonts {
    pub regular: Font,
}

const FONT_CANDIDATES: &[(&str, usize)] = &[
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 2), // index 2 = SC（简体）
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0), // fallback 任意 face
    ("/usr/share/fonts/TTF/DejaVuSans.ttf", 0),               // 英文 fallback
    ("/usr/share/fonts/liberation/LiberationSans-Regular.ttf", 0),
];

pub fn load_fonts() -> Result<Fonts, String> {
    for (path, index) in FONT_CANDIDATES {
        if let Ok(data) = std::fs::read(path) {
            let settings = FontSettings {
                collection_index: *index as u32,
                ..Default::default()
            };
            if let Ok(font) = Font::from_bytes(data, settings) {
                return Ok(Fonts { regular: font });
            }
        }
    }
    Err("无法加载任何系统字体（需要 noto-cjk 或 dejavu）".to_string())
}

/// 测量文本行宽（像素）
pub fn text_width(font: &Font, px: f32, s: &str) -> u32 {
    s.chars().map(|c| font.rasterize(c, px).0.advance_width.ceil() as u32).sum()
}

/// 光栅化一行文字为灰度像素（返回 w, h, data）
pub fn rasterize_line(font: &Font, px: f32, s: &str) -> (u32, u32, Vec<u8>) {
    let mut width = 0u32;
    for c in s.chars() {
        let (m, _) = font.rasterize(c, px);
        width += m.advance_width.ceil() as u32;
    }
    let height = (px.ceil() as u32).max(1);
    let mut data = vec![0u8; (width * height) as usize];
    let mut x = 0usize;
    for c in s.chars() {
        let (m, bitmap) = font.rasterize(c, px);
        let cw = m.width as usize;
        for (i, &v) in bitmap.iter().enumerate() {
            let bx = i % cw;
            let by = i / cw;
            let dx = x + bx + m.xmin.max(0) as usize;
            let dy = m.ymin.max(0) as usize + by;
            if dx < width as usize && dy < height as usize {
                data[dy * width as usize + dx] = v;
            }
        }
        x += m.advance_width.ceil() as usize;
    }
    (width, height, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_cjk_font() {
        let fonts = load_fonts().expect("系统应有 noto-cjk 或 fallback 字体");
        let (w, h, data) = rasterize_line(&fonts.regular, 16.0, "广告测试Ad");
        assert!(w > 0 && h > 0);
        assert_eq!(data.len() as u32, w * h);
        assert!(data.iter().any(|&v| v > 0), "应有非零像素");
    }

    #[test]
    fn text_width_matches_rasterize() {
        let fonts = load_fonts().unwrap();
        let w1 = text_width(&fonts.regular, 16.0, "测试");
        let (w2, _, _) = rasterize_line(&fonts.regular, 16.0, "测试");
        assert_eq!(w1, w2);
    }
}
