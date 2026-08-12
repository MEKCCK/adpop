# adpop 设计文档 — 通用广告弹窗子进程（Rust）

日期：2026-08-12
状态：已确认

## 背景与目标

给其他软件提供一个"广告弹窗"子进程服务：调用方（任意语言）通过约定的命令行协议 spawn 本进程并传参，进程在屏幕上弹出仿 Windows 流氓广告样式的弹窗，弹完即退。

必须同时支持两种桌面协议：

- **Wayland（niri 26.04，当前桌面）**：用 `wlr-layer-shell` 协议创建 TOP 层角落悬浮窗
- **X11（含 XWayland DISPLAY=:0）**：用 `x11rb` 创建 override-redirect + above 无边框窗口

技术路线：Rust 单二进制，双后端，像素自绘（不依赖 GTK/Qt/softbuffer-winit）。

## 架构

```
adpop (Rust, 单二进制)
├── main.rs            clap CLI 解析 → 参数校验
├── session.rs         会话探测：$WAYLAND_DISPLAY → Wayland；否则 $DISPLAY → X11
├── backend/mod.rs     trait PopupBackend { show(&PopupSpec) -> Result<CloseReason> }
│   ├── wayland.rs     wlr-layer-shell：TOP 层 + anchor 角落 + margin 偏移
│   └── x11.rs         x11rb：override-redirect + above + 无边框 + 屏幕坐标
├── render.rs          像素自绘：解码图片 + 文字排版 → RGB buffer
│   └── fonts.rs       fontdue + 系统 Noto Sans CJK 渲染中文
└── behavior.rs        关闭按钮命中检测、超时、多弹窗错开
```

依赖 crates：

- `clap`（CLI 解析）
- `wayland-client` + `wlr-layer-shell` 协议（Wayland 后端）
- `x11rb`（X11 后端）
- `image`（PNG/JPEG 解码）
- `fontdue`（文本光栅化，直接读系统字体文件）
- `memmap2` / `libc`（共享内存 buffer）

零系统 GUI 依赖，只需系统自带的 libwayland-client / libxcb。

## CLI 协议（给其他软件的 API）

```bash
# 最小调用：默认右下角、标题"广告"、15 秒后自动关
adpop show --text "恭喜你中奖了！点此领取"

# 完整参数
adpop show \
  --title "恭喜你中奖了" \        # 标题栏文字（默认"广告"）
  --text "点此领取 iPhone 15" \   # 正文，支持多行（\n）
  --image /path/ad.png \          # 广告图（可选；有图则图上文下，无图纯文字）
  --duration 15 \                 # 秒，超时自动关闭（默认 15；0 = 不自动关）
  --corner bottom-right \         # top-left|top-right|bottom-left|bottom-right
  --count 3 \                     # 同时弹 3 个（位置微错开、逐个延迟 0.8s 出现）
  --no-close \                    # 流氓模式：关闭按钮点了没反应
  --size 320x220                  # 弹窗尺寸（默认 320x220）
```

### 退出码约定

| 退出码 | 含义 |
|---|---|
| 0 | 用户点击了关闭按钮（真关闭） |
| 1 | 超时自动关闭 |
| 2 | 参数错误 |
| 3 | 无可用显示后端（既无 Wayland 也无 X11） |
| 4 | 图片加载失败（降级为纯文字继续弹，同时返回 4） |

## 渲染外观（仿 Windows 流氓广告）

- 白色主体 + 灰色标题栏（高约 24px，左侧标题文字，右侧"×"按钮）
- 有图：图片按宽度等比缩放显示在上部，文字在下部；无图：文字居中
- 无边框、无阴影（layer-shell / X11 override 天然无边框）
- 中文用系统 `/usr/share/fonts/noto-cjk` 渲染（fontdue 直接读字体文件，无需 fontconfig）

## 弹窗行为（流氓特性清单）

- 置顶（Wayland TOP 层 / X11 above）
- 屏幕角落弹出（默认右下角，margin 24px）
- 无边框
- 关闭按钮点了没反应（`--no-close` 时点 × 假装没发生）
- 超时自动消失
- 多弹窗：count > 1 时在**单个进程内**创建多个弹窗（不 spawn 多个进程），位置按角落方向微错开（每层偏移 24px），逐个延迟 0.8s 出现

## 错误处理

- 后端探测：优先 Wayland，连接失败自动降级 X11；都失败退出码 3
- 图片路径不存在/解码失败：警告并降级为纯文字弹窗，退出码 4
- 参数错误（非法 corner、非法 size、未知子命令）：退出码 2 + stderr 报错

## 测试

- 单元测试：
  - CLI 解析（各参数合法/非法组合）
  - 会话探测（Wayland/X11/都无）
  - 关闭按钮命中坐标（含 --no-close 忽略）
  - 图片缩放计算（等比缩放、超界裁剪）
- 手动验证：
  - niri 上跑 Wayland 后端实测弹窗效果（截图确认）
  - X11 后端用现有 XWayland（DISPLAY=:0）实测
  - 用现有 GLM-OCR 技能对截图做 OCR 验证弹窗文字渲染正确

## 交付物

- `~/Documents/adpop/` Rust 项目，`cargo build --release` 产出单二进制 `adpop`
- 设计文档（本文档）
- 实施计划（writing-plans 产出）
