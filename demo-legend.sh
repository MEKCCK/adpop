#!/usr/bin/env bash
# 传奇页游广告演示脚本
# 用法: ./demo-legend.sh [视频路径] [时长秒]
#   - 默认视频: 之前的 share_d3a8848521...mp4（不存在则用纯文字/GIF 效果）
#   - 默认时长: 30 秒（0 = 不自动关）
set -euo pipefail

cd "$(dirname "$0")"

# 默认素材
DEFAULT_VIDEO="/home/elite/Downloads/share_d3a8848521c6db5f7a57686fe076d76f1786536153.mp4"
VIDEO="${1:-$DEFAULT_VIDEO}"
DURATION="${2:-30}"

# 构建（如未编译）
if [ ! -x target/release/adpop ]; then
    echo "==> 构建 adpop..."
    cargo build --release
fi

ARGS=(--text "屠龙宝刀点击就送 · 一刀999级 · 是兄弟就来砍我"
       --url "https://www.example.com"
       --click-zone all
       --animate marquee
       --duration "$DURATION")

if [ -f "$VIDEO" ]; then
    ARGS+=(--video "$VIDEO")
    echo "==> 传奇广告素材: $VIDEO"
else
    echo "==> 视频不存在（$VIDEO），使用内置传奇风格文字广告"
fi

echo "==> 弹出传奇页游广告（${DURATION} 秒，点击任意位置打开浏览器跳转）"
echo "==> 按 Ctrl+C 提前关闭"
echo

./target/release/adpop "${ARGS[@]}"
CODE=$?
echo
case $CODE in
    0) echo "已点击关闭按钮关闭" ;;
    1) echo "超时自动关闭" ;;
    6) echo "点击广告跳转后关闭（浏览器已打开）" ;;
    *) echo "退出码: $CODE" ;;
esac
