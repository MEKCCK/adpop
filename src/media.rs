// src/media.rs —— 核心逻辑（完整测试见下）
use std::process::{Child, ChildStdout, Command, Stdio};

#[derive(Debug, PartialEq)]
pub struct GifFrame { pub img: image::RgbaImage, pub delay_ms: u64 }

pub struct VideoStream { pub child: Child, pub stdout: ChildStdout }

/// GIF：解码所有帧 + 帧延迟（image crate numer_denom_ms 返回毫秒比值，直接相除）
pub fn load_gif(path: &str) -> Result<Vec<GifFrame>, String> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    let data = std::fs::read(path).map_err(|e| format!("读取 GIF 失败: {e}"))?;
    let decoder = GifDecoder::new(std::io::Cursor::new(data)).map_err(|e| format!("GIF 解码失败: {e}"))?;
    let frames = decoder.into_frames().collect_frames().map_err(|e| format!("GIF 帧读取失败: {e}"))?;
    if frames.is_empty() { return Err("GIF 无帧".to_string()); }
    frames.iter().map(|f| {
        let (num, denom) = f.delay().numer_denom_ms();
        let delay_ms = if denom == 0 { 100 } else { (num / denom).max(10) };
        Ok(GifFrame { img: f.clone().into_buffer(), delay_ms: delay_ms as u64 })
    }).collect()
}

/// 按时间选 GIF 帧（循环）
pub fn gif_frame_at(frames: &[GifFrame], t_ms: u64) -> &GifFrame {
    let total: u64 = frames.iter().map(|f| f.delay_ms).sum();
    let mut t = t_ms % total.max(1);
    for f in frames {
        if t < f.delay_ms { return f; }
        t -= f.delay_ms;
    }
    &frames[0]
}

/// 视频：ffmpeg 流式管道（bgr0 = XRGB8888 字节序零转换）
pub fn spawn_video_stream(path: &str, w: u32, h: u32, fps: u32) -> Result<VideoStream, String> {
    let mut child = Command::new("ffmpeg")
        .args(["-i", path, "-vf", &format!("scale={w}:{h},fps={fps}"), "-f", "rawvideo", "-pix_fmt", "bgr0", "pipe:1"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|e| format!("ffmpeg 启动失败: {e}"))?;
    let stdout = child.stdout.take().ok_or("无法取 ffmpeg 输出管道")?;
    Ok(VideoStream { child, stdout })
}

/// 非阻塞读一帧（管道可读才 read_exact，否则 false）
pub fn try_read_video_frame(vs: &mut VideoStream, out: &mut [u8]) -> bool {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    let rfd = vs.stdout.as_raw_fd();
    let mut pfd = libc::pollfd { fd: rfd, events: libc::POLLIN, revents: 0 };
    unsafe { libc::poll(&mut pfd, 1, 0); }
    if pfd.revents & libc::POLLIN != 0 {
        return vs.stdout.read_exact(out).is_ok();
    }
    false
}

/// 音频：mpv 子进程后台播放（不阻塞）
pub fn play_audio(file: &str) -> Option<Child> {
    Command::new("mpv")
        .args(["--no-video", "--volume=80", "--really-quiet", file])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_frame_at_cycles() {
        let frames = vec![
            GifFrame { img: image::RgbaImage::new(1, 1), delay_ms: 100 },
            GifFrame { img: image::RgbaImage::new(1, 1), delay_ms: 100 },
        ];
        assert_eq!(gif_frame_at(&frames, 50), &frames[0]);
        assert_eq!(gif_frame_at(&frames, 150), &frames[1]);
        assert_eq!(gif_frame_at(&frames, 250), &frames[0]); // 循环
    }

    #[test]
    fn gif_frame_at_empty_delay() {
        let frames = vec![GifFrame { img: image::RgbaImage::new(1, 1), delay_ms: 0 }];
        let _ = gif_frame_at(&frames, 0); // 不 panic（total.max(1) 保护）
    }
}