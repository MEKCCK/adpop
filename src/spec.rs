#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner { TopLeft, TopRight, BottomLeft, BottomRight }

impl Corner {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "top-left" => Ok(Corner::TopLeft),
            "top-right" => Ok(Corner::TopRight),
            "bottom-left" => Ok(Corner::BottomLeft),
            "bottom-right" => Ok(Corner::BottomRight),
            _ => Err(format!("invalid corner: {s} (top-left|top-right|bottom-left|bottom-right)")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size { pub w: u32, pub h: u32 }

impl Size {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (w, h) = s.split_once('x')
            .ok_or_else(|| format!("invalid size: {s} (expected WxH like 320x220)"))?;
        let w: u32 = w.trim().parse().map_err(|_| format!("invalid size: {s}"))?;
        let h: u32 = h.trim().parse().map_err(|_| format!("invalid size: {s}"))?;
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return Err(format!("invalid size: {s} (must be 1..=4096)"));
        }
        Ok(Size { w, h })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason { Closed, TimedOut, Jumped }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickZone { All, Button, Body, None }

impl ClickZone {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "all" => Ok(ClickZone::All),
            "button" => Ok(ClickZone::Button),
            "body" => Ok(ClickZone::Body),
            "none" => Ok(ClickZone::None),
            _ => Err(format!("invalid click-zone: {s} (all|button|body|none)")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Animate { None, Marquee, Flash }

impl Animate {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "none" => Ok(Animate::None),
            "marquee" => Ok(Animate::Marquee),
            "flash" => Ok(Animate::Flash),
            _ => Err(format!("invalid animate: {s} (none|marquee|flash)")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos { pub x: i32, pub y: i32 }

impl Pos {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (x, y) = s.split_once(',').ok_or_else(|| format!("invalid pos: {s} (expected X,Y)"))?;
        Ok(Pos {
            x: x.trim().parse().map_err(|_| format!("invalid pos: {s}"))?,
            y: y.trim().parse().map_err(|_| format!("invalid pos: {s}"))?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PopupSpec {
    pub title: String,
    pub text: String,
    pub image: Option<String>,   // PNG/JPEG 静态图；.gif 动图
    pub video: Option<String>,   // 视频（ffmpeg 解帧）
    pub audio: Option<String>,   // 独立音频（mpv）；--video 时默认用其音轨
    pub duration: u64,           // 秒；0 = 不自动关闭
    pub corner: Corner,
    pub pos: Option<Pos>,        // 自定义位置（覆盖 corner）
    pub count: u32,              // >= 1
    pub no_close: bool,
    pub size: Size,
    pub url: Option<String>,     // 点击跳转链接
    pub click_zone: ClickZone,   // 可点击区域
    pub animate: Animate,        // 画面动画
}

impl Default for PopupSpec {
    fn default() -> Self {
        PopupSpec {
            title: "广告".to_string(),
            text: String::new(),
            image: None,
            video: None,
            audio: None,
            duration: 15,
            corner: Corner::BottomRight,
            pos: None,
            count: 1,
            no_close: false,
            size: Size { w: 320, h: 220 },
            url: None,
            click_zone: ClickZone::None,
            animate: Animate::None,
        }
    }
}

impl PopupSpec {
    pub fn validate(&self) -> Result<(), String> {
        if self.text.is_empty() && self.image.is_none() && self.video.is_none() && self.audio.is_none() {
            return Err("nothing to show: provide --text and/or --image/--video/--audio".to_string());
        }
        if self.count == 0 {
            return Err("--count must be >= 1".to_string());
        }
        if self.url.is_none() && self.click_zone != ClickZone::None {
            return Err("--click-zone requires --url".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_parse_valid() {
        assert_eq!(Corner::parse("top-left"), Ok(Corner::TopLeft));
        assert_eq!(Corner::parse("bottom-right"), Ok(Corner::BottomRight));
    }

    #[test]
    fn corner_parse_invalid() {
        assert!(Corner::parse("center").is_err());
    }

    #[test]
    fn size_parse_valid() {
        assert_eq!(Size::parse("320x220"), Ok(Size { w: 320, h: 220 }));
        assert_eq!(Size::parse(" 400 x 300 "), Ok(Size { w: 400, h: 300 }));
    }

    #[test]
    fn size_parse_invalid() {
        assert!(Size::parse("abc").is_err());
        assert!(Size::parse("0x100").is_err());
        assert!(Size::parse("99999x10").is_err());
    }

    #[test]
    fn default_spec_valid() {
        let spec = PopupSpec::default();
        assert_eq!(spec.title, "广告");
        assert_eq!(spec.corner, Corner::BottomRight);
        assert_eq!(spec.duration, 15);
        assert_eq!(spec.size, Size { w: 320, h: 220 });
    }

    #[test]
    fn validate_requires_content() {
        let spec = PopupSpec::default(); // text 为空、无 image
        assert!(spec.validate().is_err());
    }
}
