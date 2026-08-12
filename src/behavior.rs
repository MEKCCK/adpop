//! 行为逻辑：关闭按钮命中检测、多弹窗位置错开、屏幕角落定位。

use crate::spec::Corner;

pub const MARGIN: i32 = 24;
pub const TITLEBAR_H: u32 = 24;
pub const CLOSE_W: u32 = 32;
pub const STAGGER: i32 = 24; // 多弹窗错开步长

/// 关闭按钮在窗口内的矩形：(x, y, w, h)。w<CLOSE_W 时回退到 x=0（不 u32 下溢）。
pub fn close_button_rect(w: u32) -> (u32, u32, u32, u32) {
    (w.saturating_sub(CLOSE_W), 0, CLOSE_W, TITLEBAR_H)
}

/// 命中检测：坐标 (x, y)（窗口内，y 向下为正）是否落在关闭按钮上。
pub fn hit_close(x: i32, y: i32, win_w: u32) -> bool {
    let (rx, ry, rw, rh) = close_button_rect(win_w);
    x >= rx as i32 && x < (rx + rw) as i32 && y >= ry as i32 && y < (ry + rh) as i32
}

/// 第 index 个弹窗相对角落的偏移（从角落向屏幕中心错开）。
pub fn popup_offset(corner: &Corner, index: usize) -> (i32, i32) {
    let d = (index as i32) * STAGGER;
    match corner {
        Corner::TopLeft | Corner::BottomLeft => (d, 0),
        _ => (-d, 0),
    }
}

/// 弹窗左上角屏幕坐标。基础版：仅按角落定位（右下角从右下往左上排布），
/// 多弹窗时向屏幕中心错开，负坐标收敛到 0。--pos 任意坐标由 Task 8 后端处理。
pub fn screen_pos(corner: &Corner, win_w: u32, win_h: u32, screen_w: u32, screen_h: u32, index: usize) -> (i32, i32) {
    let (dx, _dy) = popup_offset(corner, index);
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => MARGIN + dx,
        _ => screen_w as i32 - win_w as i32 - MARGIN + dx,
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => MARGIN,
        _ => screen_h as i32 - win_h as i32 - MARGIN,
    };
    (x.max(0), y.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_rect_position() {
        let (x, y, w, h) = close_button_rect(320);
        assert_eq!((x, y), (288, 0));
        assert_eq!((w, h), (32, 24));
    }

    #[test]
    fn hit_close_inside_outside() {
        assert!(hit_close(300, 10, 320));
        assert!(!hit_close(200, 10, 320));
        assert!(!hit_close(300, 30, 320));
    }

    #[test]
    fn offsets_stagger_inward() {
        assert_eq!(popup_offset(&Corner::BottomRight, 0), (0, 0));
        assert_eq!(popup_offset(&Corner::BottomRight, 1), (-24, 0));
        assert_eq!(popup_offset(&Corner::BottomLeft, 1), (24, 0));
    }

    #[test]
    fn screen_pos_corners() {
        // 800x600 屏幕，320x220 窗口
        let br = screen_pos(&Corner::BottomRight, 320, 220, 800, 600, 0);
        assert_eq!(br, (800 - 320 - 24, 600 - 220 - 24));
        let tl = screen_pos(&Corner::TopLeft, 320, 220, 800, 600, 0);
        assert_eq!(tl, (24, 24));
        let br2 = screen_pos(&Corner::BottomRight, 320, 220, 800, 600, 1);
        assert_eq!(br2, (800 - 320 - 24 - 24, 600 - 220 - 24));
    }
}
