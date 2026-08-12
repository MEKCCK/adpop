use clap::{Arg, ArgAction, Command};
use crate::spec::{Animate, ClickZone, Corner, Pos, PopupSpec, Size};

pub fn parse_args(args: Vec<String>) -> Result<PopupSpec, String> {
    let matches = Command::new("adpop")
        .about("通用广告弹窗：给其他软件当子进程用的弹窗服务")
        .subcommand_required(false)
        .arg(Arg::new("title").long("title").value_name("TEXT").help("标题栏文字（默认：广告）"))
        .arg(Arg::new("text").long("text").value_name("TEXT").help("正文，支持 \\n 多行"))
        .arg(Arg::new("image").long("image").value_name("PATH").help("广告图片（PNG/JPEG）"))
        .arg(Arg::new("duration").long("duration").value_name("SECONDS").default_value("15").help("超时自动关闭秒数（0=不自动关）"))
        .arg(Arg::new("corner").long("corner").value_name("CORNER").default_value("bottom-right").help("角落：top-left|top-right|bottom-left|bottom-right"))
        .arg(Arg::new("count").long("count").value_name("N").default_value("1").help("同时弹出 N 个（错开位置）"))
        .arg(Arg::new("no-close").long("no-close").action(ArgAction::SetTrue).help("流氓模式：关闭按钮点了没反应"))
        .arg(Arg::new("size").long("size").value_name("WxH").default_value("320x220").help("弹窗尺寸"))
        .arg(Arg::new("video").long("video").value_name("PATH").help("视频文件（ffmpeg 解帧 + mpv 音频）"))
        .arg(Arg::new("audio").long("audio").value_name("PATH").help("独立音频文件（mpv 播放）"))
        .arg(Arg::new("pos").long("pos").value_name("X,Y").help("自定义位置：左上角绝对坐标（覆盖 --corner）"))
        .arg(Arg::new("url").long("url").value_name("URL").help("点击跳转链接"))
        .arg(Arg::new("click-zone").long("click-zone").value_name("ZONE").default_value("none").help("可点击区域：all|button|body|none"))
        .arg(Arg::new("animate").long("animate").value_name("MODE").default_value("none").help("画面动画：none|marquee|flash"))
        .get_matches_from(args);

    let mut spec = PopupSpec::default();
    if let Some(t) = matches.get_one::<String>("title") { spec.title = t.clone(); }
    if let Some(t) = matches.get_one::<String>("text") { spec.text = t.clone(); }
    if let Some(p) = matches.get_one::<String>("image") { spec.image = Some(p.clone()); }
    spec.duration = matches.get_one::<String>("duration").unwrap()
        .parse().map_err(|_| "--duration 必须是整数秒".to_string())?;
    spec.corner = Corner::parse(matches.get_one::<String>("corner").unwrap())?;
    spec.count = matches.get_one::<String>("count").unwrap()
        .parse().map_err(|_| "--count 必须是整数".to_string())?;
    spec.no_close = matches.get_flag("no-close");
    spec.size = Size::parse(matches.get_one::<String>("size").unwrap())?;
    if let Some(v) = matches.get_one::<String>("video") { spec.video = Some(v.clone()); }
    if let Some(a) = matches.get_one::<String>("audio") { spec.audio = Some(a.clone()); }
    if let Some(p) = matches.get_one::<String>("pos") { spec.pos = Some(Pos::parse(p)?); }
    if let Some(u) = matches.get_one::<String>("url") { spec.url = Some(u.clone()); }
    spec.click_zone = ClickZone::parse(matches.get_one::<String>("click-zone").unwrap())?;
    spec.animate = Animate::parse(matches.get_one::<String>("animate").unwrap())?;
    spec.validate()?;
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal() {
        let spec = parse_args(vec!["adpop".into(), "--text".into(), "hello".into()]).unwrap();
        assert_eq!(spec.text, "hello");
        assert_eq!(spec.title, "广告");
        assert_eq!(spec.corner, Corner::BottomRight);
        assert_eq!(spec.duration, 15);
    }

    #[test]
    fn parse_full() {
        let spec = parse_args(vec![
            "adpop".into(),
            "--title".into(), "恭喜".into(),
            "--text".into(), "点此领取".into(),
            "--image".into(), "/tmp/a.png".into(),
            "--duration".into(), "0".into(),
            "--corner".into(), "top-left".into(),
            "--count".into(), "3".into(),
            "--no-close".into(),
            "--size".into(), "400x300".into(),
        ]).unwrap();
        assert_eq!(spec.title, "恭喜");
        assert_eq!(spec.image.as_deref(), Some("/tmp/a.png"));
        assert_eq!(spec.duration, 0);
        assert_eq!(spec.corner, Corner::TopLeft);
        assert_eq!(spec.count, 3);
        assert!(spec.no_close);
        assert_eq!(spec.size, Size { w: 400, h: 300 });
    }

    #[test]
    fn parse_bad_duration() {
        assert!(parse_args(vec!["adpop".into(), "--text".into(), "x".into(), "--duration".into(), "abc".into()]).is_err());
    }

    #[test]
    fn parse_bad_corner() {
        assert!(parse_args(vec!["adpop".into(), "--text".into(), "x".into(), "--corner".into(), "center".into()]).is_err());
    }

    #[test]
    fn parse_no_content() {
        assert!(parse_args(vec!["adpop".into()]).is_err());
    }
}
