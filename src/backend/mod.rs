pub mod wayland;
pub mod x11;

use crate::spec::{CloseReason, PopupSpec};

/// 弹窗后端抽象：向桌面显示一个弹窗并运行事件循环，
/// 返回关闭原因（用户点击关闭 / 超时自动关闭 / 跳转）。
pub trait PopupBackend {
    fn show(&mut self, spec: &PopupSpec) -> Result<CloseReason, String>;
}
