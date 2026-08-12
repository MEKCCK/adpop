# adpop — 通用广告弹窗子进程

给其他软件用的广告弹窗服务：按命令行协议调用，在 Wayland / X11 上弹出仿 Windows 流氓广告弹窗。
完全自绘渲染——任何广告风格都能画（炒股、传奇页游、擦边、抽奖…），支持动图、视频、音频、点击跳转、画面动画、自定义位置/大小。

## 用法

```bash
# 最简：右下角弹窗，15 秒自动关
adpop show --text "恭喜你中奖了！点此领取"

# 完整能力
adpop show \
  --title "股市内幕" --text "7天翻倍 · 稳赚不赔" \
  --image ad.gif \                 # 静态图 PNG/JPEG 或动图 GIF
  --video ad.mp4 \                 # 视频（ffmpeg 流式解码，任意时长；自动播其音轨）
  --audio bgm.mp3 \                # 独立音频（mpv 播放）
  --duration 15 \                  # 秒；0 = 不自动关
  --corner bottom-right \          # top-left|top-right|bottom-left|bottom-right
  --pos 700,300 \                  # 自定义位置（覆盖 --corner）
  --size 480x320 \                 # 自定义大小
  --url "https://example.com" \    # 点击跳转链接
  --click-zone all \               # 可点击区域 all|button|body|none
  --animate marquee \              # 画面动画 none|marquee|flash
  --count 3 \                      # 同时弹 3 个（错开位置）
  --no-close                       # 流氓模式：关闭按钮点了没反应
```

## 参数

| 参数 | 默认 | 说明 |
|---|---|---|
| `--title` | 广告 | 标题栏文字 |
| `--text` | 必填或媒体 | 正文（`\n` 多行） |
| `--image` | 无 | PNG/JPEG 静态图；`.gif` 动图循环播放 |
| `--video` | 无 | 视频（ffmpeg 流式管道 `bgr0`，任意时长、内存恒定） |
| `--audio` | 无 | 独立音频（mpv 后台播放；`--video` 时默认用其音轨） |
| `--duration` | 15 | 超时自动关闭秒数（0 = 不自动关） |
| `--corner` | bottom-right | 四角快捷定位 |
| `--pos` | 无 | 左上角绝对坐标 `X,Y`（覆盖 `--corner`） |
| `--size` | 320x220 | 弹窗尺寸 WxH |
| `--url` | 无 | 点击跳转链接（有它点击才生效） |
| `--click-zone` | none | `all` 主体可点 / `button` 按钮 / `body` 正文 / `none` 不可点 |
| `--animate` | none | `marquee` 跑马灯 / `flash` 闪烁 |
| `--count` | 1 | 弹出数量（错开位置，间隔 0.8s） |
| `--no-close` | 关 | 关闭按钮点了没反应（流氓模式） |

## 退出码

| 码 | 含义 |
|---|---|
| 0 | 用户点击关闭按钮 |
| 1 | 超时自动关闭 |
| 2 | 参数错误 |
| 3 | 无显示后端（既无 Wayland 也无 X11） |
| 4 | 图片/GIF 加载失败（降级纯文字） |
| 5 | 视频解码失败（降级文字；音频失败仅警告） |
| 6 | 点击广告跳转后关闭 |

## 后端

- **Wayland（niri/sway 等支持 wlr-layer-shell 的合成器）**：TOP 层置顶无边框，anchor+margin 定位（`--pos` 四象限推导），wl_shm 多 buffer 轮换动画，wl_pointer 点击
- **X11（含 XWayland）**：override-redirect + above 无边框窗口，PutImage 渲染，ButtonPress 点击
- 自动探测 `$WAYLAND_DISPLAY` / `$DISPLAY`
- 零 GUI 库依赖（无 GTK/Qt）；运行时需要 `ffmpeg`（视频）+ `mpv`（音频）+ `xdg-open`（点击跳转）

## 构建

```bash
cargo build --release
# 产物：target/release/adpop
```

## 技术要点

- 渲染：自绘像素（fontdue 中文 + image 解码），XRGB8888 小端 [B,G,R,X]
- 动图/视频：wl_shm buffer 被合成器缓存，动画必须多 buffer 轮换（3 buffer）
- Wayland 显示流程：空 commit → Configure → ack_configure → attach buffer（Smithay/niri 强制）
- Wayland 事件循环：`poll_fd` + `prepare_read` + `ReadEventsGuard::read` + `dispatch_pending`（dispatch_pending 不读 socket）
