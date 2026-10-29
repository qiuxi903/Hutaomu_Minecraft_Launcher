//! 动效基础设施
//!
//! 界面上所有会动的地方都从这里取值：淡入、位移、缩放、脉动。
//! 全部受「设置 - 界面动画」开关控制，关掉后所有进度直接到 1，
//! 界面回到静态的样子。
//!
//! 动画状态存在 egui 的内存里（按 Id 索引），不用自己维护生命周期。
//!
//! **时长只用下面这几个档位**，不要再随手写数字。同一个交互在不同控件里
//! 用不同时间，手感会打架：按下去要跟手，展开要稳，入场要看得清。

use egui::{Color32, Context, Id, Rect, Stroke};

use super::animations_enabled;

/// 按下反馈：最快，要有一点「咔」的手感
pub const DUR_PRESS: f32 = 0.08;
/// 悬停
pub const DUR_HOVER: f32 = 0.12;
/// 选中、展开这类状态变化
pub const DUR_BASE: f32 = 0.16;
/// 入场
pub const DUR_ENTER: f32 = 0.22;
/// 淡出
pub const DUR_FADE: f32 = 0.30;

/// 缓出：起步快，收尾稳
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// 带回弹的缓出：收尾时轻轻弹一下
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// 布尔状态过渡
///
/// 关掉动画时直接给目标值，不做过渡。
pub fn to_bool(ctx: &Context, id: Id, target: bool, duration: f32) -> f32 {
    if !animations_enabled() {
        return if target { 1.0 } else { 0.0 };
    }
    ctx.animate_bool_with_time(id, target, duration)
}

/// 数值过渡
///
/// 关掉动画时直接给目标值。
pub fn to_value(ctx: &Context, id: Id, target: f32, duration: f32) -> f32 {
    if !animations_enabled() {
        return target;
    }
    ctx.animate_value_with_time(id, target, duration)
}

/// 一条动画的进度
///
/// `delay` 用来做交错入场：同一组里的第 N 项延后 N × delay 才开始动。
pub fn progress(ctx: &Context, id: Id, delay: f32, duration: f32) -> f32 {
    if !animations_enabled() {
        return 1.0;
    }
    let now = ctx.input(|i| i.time);
    let start = ctx
        .memory(|m| m.data.get_temp::<f64>(id))
        .unwrap_or_else(|| {
            ctx.memory_mut(|m| m.data.insert_temp(id, now));
            now
        });
    let elapsed = (now - start - delay as f64) as f32;
    if elapsed <= 0.0 {
        return 0.0;
    }
    ease_out(elapsed / duration.max(0.01))
}

/// 以首次渲染时间为基准计算 0→1 的入场进度；active 为 false 时清除状态
///
/// 关掉动画时直接到 1，控件不会再有淡入与位移。
pub fn enter_progress(ctx: &Context, id: Id, active: bool, duration: f32) -> f32 {
    if !active {
        ctx.memory_mut(|m| m.data.remove::<f64>(id));
        return 0.0;
    }
    if !animations_enabled() {
        return 1.0;
    }
    progress(ctx, id, 0.0, duration)
}

/// 淡入：0 → 1
pub fn fade_in(ctx: &Context, id: Id, delay: f32) -> f32 {
    progress(ctx, id, delay, DUR_ENTER)
}

/// 一组条目的交错延迟
///
/// 第 index 项延后 index × step 秒入场，
/// 一长串卡片就不会一起糊在屏幕上，而是像依次落位。
pub fn stagger(index: usize, step: f32) -> f32 {
    (index as f32 * step).min(0.6)
}

/// 内容区入场：淡入 + 上移 + 轻微放大
///
/// 页面切换时整块内容一起推进来，比单纯改透明度更有方向感。
pub struct Enter {
    pub opacity: f32,
    pub offset: f32,
    pub scale: f32,
}

/// 给页面内容算一个入场状态
///
/// 透明度不从 0 开始：整页一起淡入时，卡片的阴影也会跟着变浓变淡，
/// 起点太低时看着像阴影在闪。这里留大半底，只做轻微的渐入。
pub fn enter(ctx: &Context, id: Id, delay: f32) -> Enter {
    let t = progress(ctx, id, delay, DUR_ENTER);
    Enter {
        opacity: 0.82 + 0.18 * t,
        offset: (1.0 - t) * 6.0,
        scale: 0.995 + 0.005 * t,
    }
}

/// 画一个柔和的阴影
///
/// egui 没有模糊阴影，这里用几圈由内到外变淡的描边叠出层次感。
///
/// 两个要点：
/// - 阴影画在卡片外面，不会盖住里面的文字
/// - 不复制 painter：egui 的 Painter 带着这一帧已绘制的全部图形，
///   复制一次就是全量拷贝，页面上卡片一多就会把渲染拖死
pub fn shadow(painter: &egui::Painter, rect: Rect, radius: egui::CornerRadius, alpha: u8) {
    if !animations_enabled() || alpha == 0 {
        return;
    }
    // 阴影不跟着外层的淡入一起变淡，页面切换时看着才稳
    let opacity = painter.opacity().max(0.05);
    let boost = (alpha as f32 / opacity).min(255.0) as u8;
    const LAYERS: usize = 4;
    for layer in (1..=LAYERS).rev() {
        let t = layer as f32 / LAYERS as f32;
        let spread = 1.0 + t * 3.0;
        let a = (boost as f32 * 0.16 * (1.0 - t * 0.55)) as u8;
        if a == 0 {
            continue;
        }
        painter.rect_stroke(
            rect.expand(spread),
            radius,
            Stroke::new(spread * 1.5, Color32::from_black_alpha(a)),
            egui::StrokeKind::Outside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_stay_in_range() {
        for step in 0..=10 {
            let t = step as f32 / 10.0;
            assert!((0.0..=1.0).contains(&ease_out(t)));
            // 回弹缓动会冲过 1 再落回来，这是它本来的样子
            assert!((-0.1..=1.2).contains(&ease_out_back(t)));
        }
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn stagger_is_capped() {
        assert_eq!(stagger(0, 0.03), 0.0);
        assert!((stagger(10, 0.03) - 0.3).abs() < 1e-5);
        assert!(stagger(1000, 0.03) <= 0.6);
    }

    #[test]
    fn 时长档位由快到慢() {
        assert!(DUR_PRESS < DUR_HOVER);
        assert!(DUR_HOVER < DUR_BASE);
        assert!(DUR_BASE < DUR_ENTER);
        assert!(DUR_ENTER < DUR_FADE);
    }
}
