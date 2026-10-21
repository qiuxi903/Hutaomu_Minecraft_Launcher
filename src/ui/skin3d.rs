//! 轻量 3D 皮肤预览：把 Minecraft 皮肤贴图映射到六个方块上，
//! 使用 egui 的 Mesh 做软件变换与深度排序，不依赖任何 3D 引擎。

use egui::{Color32, CornerRadius, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::core::auth::SkinModel;
use crate::theme::Palette;

#[derive(Clone, Copy, Debug, Default)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}

impl V3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    fn add(self, other: V3) -> V3 {
        V3::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
    fn rot_y(self, angle: f32) -> V3 {
        let (s, c) = angle.sin_cos();
        V3::new(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
    }
    fn rot_x(self, angle: f32) -> V3 {
        let (s, c) = angle.sin_cos();
        V3::new(self.x, self.y * c - self.z * s, self.y * s + self.z * c)
    }
}

/// 贴图上一个面的矩形（单位：皮肤像素）
#[derive(Clone, Copy, Debug)]
struct UvRect {
    u: f32,
    v: f32,
    w: f32,
    h: f32,
}

struct Face {
    corners: [V3; 4],
    uvs: [Pos2; 4],
    normal: V3,
}

fn box_faces(size: V3, uv: UvRect, tex: f32) -> [Face; 6] {
    let (w, h, d) = (size.x, size.y, size.z);
    let (x0, x1) = (-w / 2.0, w / 2.0);
    let (y0, y1) = (-h / 2.0, h / 2.0);
    let (z0, z1) = (-d / 2.0, d / 2.0);
    let (u, v) = (uv.u, uv.v);
    let _ = (uv.w, uv.h);

    // 每个面的 UV 都取自标准皮肤布局
    let rect_uv = |x: f32, y: f32, rw: f32, rh: f32| {
        [
            Pos2::new(x / tex, (y + rh) / tex),
            Pos2::new((x + rw) / tex, (y + rh) / tex),
            Pos2::new((x + rw) / tex, y / tex),
            Pos2::new(x / tex, y / tex),
        ]
    };

    [
        // front (+z)
        Face {
            corners: [
                V3::new(x0, y0, z1),
                V3::new(x1, y0, z1),
                V3::new(x1, y1, z1),
                V3::new(x0, y1, z1),
            ],
            uvs: rect_uv(u + d, v + d, w, h),
            normal: V3::new(0.0, 0.0, 1.0),
        },
        // back (-z)
        Face {
            corners: [
                V3::new(x1, y0, z0),
                V3::new(x0, y0, z0),
                V3::new(x0, y1, z0),
                V3::new(x1, y1, z0),
            ],
            uvs: rect_uv(u + 2.0 * d + w, v + d, w, h),
            normal: V3::new(0.0, 0.0, -1.0),
        },
        // right (+x)
        Face {
            corners: [
                V3::new(x1, y0, z1),
                V3::new(x1, y0, z0),
                V3::new(x1, y1, z0),
                V3::new(x1, y1, z1),
            ],
            uvs: rect_uv(u, v + d, d, h),
            normal: V3::new(1.0, 0.0, 0.0),
        },
        // left (-x)
        Face {
            corners: [
                V3::new(x0, y0, z0),
                V3::new(x0, y0, z1),
                V3::new(x0, y1, z1),
                V3::new(x0, y1, z0),
            ],
            uvs: rect_uv(u + d + w, v + d, d, h),
            normal: V3::new(-1.0, 0.0, 0.0),
        },
        // top (+y)
        Face {
            corners: [
                V3::new(x0, y1, z0),
                V3::new(x1, y1, z0),
                V3::new(x1, y1, z1),
                V3::new(x0, y1, z1),
            ],
            uvs: rect_uv(u + d, v, w, d),
            normal: V3::new(0.0, 1.0, 0.0),
        },
        // bottom (-y)
        Face {
            corners: [
                V3::new(x0, y0, z1),
                V3::new(x1, y0, z1),
                V3::new(x1, y0, z0),
                V3::new(x0, y0, z0),
            ],
            uvs: rect_uv(u + d + w, v, w, d),
            normal: V3::new(0.0, -1.0, 0.0),
        },
    ]
}

struct Part {
    center: V3,
    size: V3,
    uv: UvRect,
    /// 以顶部为轴的摆动角度（弧度）
    swing: f32,
}

fn build_parts(slim: bool, time: f32) -> Vec<Part> {
    let arm_w = if slim { 3.0 } else { 4.0 };
    let arm_gap = 0.0;
    let arm_x = 4.0 + arm_gap + arm_w / 2.0;
    let idle = (time * 1.6).sin() * 0.06;

    vec![
        // 头
        Part {
            center: V3::new(0.0, 28.0, 0.0),
            size: V3::new(8.0, 8.0, 8.0),
            uv: UvRect { u: 0.0, v: 0.0, w: 8.0, h: 8.0 },
            swing: 0.0,
        },
        // 身体
        Part {
            center: V3::new(0.0, 18.0, 0.0),
            size: V3::new(8.0, 12.0, 4.0),
            uv: UvRect { u: 16.0, v: 16.0, w: 8.0, h: 12.0 },
            swing: 0.0,
        },
        // 右臂（角色右侧，屏幕左侧）
        Part {
            center: V3::new(-arm_x, 18.0, 0.0),
            size: V3::new(arm_w, 12.0, 4.0),
            uv: UvRect { u: 40.0, v: 16.0, w: arm_w, h: 12.0 },
            swing: idle,
        },
        // 左臂
        Part {
            center: V3::new(arm_x, 18.0, 0.0),
            size: V3::new(arm_w, 12.0, 4.0),
            uv: UvRect { u: 32.0, v: 48.0, w: arm_w, h: 12.0 },
            swing: -idle,
        },
        // 右腿
        Part {
            center: V3::new(-2.0, 6.0, 0.0),
            size: V3::new(4.0, 12.0, 4.0),
            uv: UvRect { u: 0.0, v: 16.0, w: 4.0, h: 12.0 },
            swing: 0.0,
        },
        // 左腿
        Part {
            center: V3::new(2.0, 6.0, 0.0),
            size: V3::new(4.0, 12.0, 4.0),
            uv: UvRect { u: 16.0, v: 48.0, w: 4.0, h: 12.0 },
            swing: 0.0,
        },
    ]
}

/// 绘制皮肤预览。返回的 response 支持拖拽旋转。
pub fn skin_preview(
    ui: &mut Ui,
    p: &Palette,
    texture: Option<&egui::TextureHandle>,
    model: SkinModel,
    yaw: &mut f32,
    height: f32,
    animate: bool,
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::drag());

    let painter = ui.painter().clone();
    let bg = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), rect.height()));
    painter.rect_filled(bg, p.corner(0), p.surface_alt);

    if response.dragged() {
        *yaw += response.drag_delta().x * 0.012;
    }
    if animate {
        let time = ui.input(|i| i.time) as f32;
        if !response.dragged() {
            *yaw += 0.0035;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        let _ = time;
    }

    // 地面阴影
    let shadow_center = Pos2::new(rect.center().x, rect.top() + height * 0.9);
    let shadow_points: Vec<Pos2> = (0..=36)
        .map(|i| {
            let a = i as f32 / 36.0 * std::f32::consts::TAU;
            Pos2::new(
                shadow_center.x + a.cos() * height * 0.17,
                shadow_center.y + a.sin() * height * 0.035,
            )
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(
        shadow_points,
        Color32::from_black_alpha(if p.dark { 60 } else { 22 }),
        Stroke::NONE,
    ));

    let Some(texture) = texture else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "暂无皮肤",
            super::font(13.0),
            p.text_faint,
        );
        return response;
    };

    let time = ui.input(|i| i.time) as f32;
    let parts = build_parts(model == SkinModel::Slim, if animate { time } else { 0.0 });

    // 模型总高 32 个皮肤像素（脚底 y=0，头顶 y=32），垂直居中放置
    const MODEL_HEIGHT: f32 = 32.0;
    const MODEL_CENTER_Y: f32 = 16.0;
    let scale = height * 0.80 / 34.0;
    let center = Pos2::new(rect.center().x, rect.top() + height * 0.52);
    let pitch = -0.10_f32;
    let yaw_angle = *yaw;

    let mut faces: Vec<(f32, egui::Mesh)> = Vec::new();

    for part in &parts {
        let faces_local = box_faces(part.size, part.uv, 64.0);
        for face in faces_local {
            // 摆动：绕方块顶部（局部 y = +h/2）旋转
            let mut corners = face.corners;
            let mut normal = face.normal;
            if part.swing.abs() > 0.0001 {
                let pivot = V3::new(0.0, part.size.y / 2.0, 0.0);
                for corner in corners.iter_mut() {
                    let local = V3::new(corner.x - pivot.x, corner.y - pivot.y, corner.z - pivot.z)
                        .rot_x(part.swing);
                    *corner = local.add(pivot);
                }
                normal = normal.rot_x(part.swing);
            }

            let mut world = [V3::default(); 4];
            for (i, corner) in corners.iter().enumerate() {
                world[i] = corner.add(part.center).rot_y(yaw_angle).rot_x(pitch);
            }
            let n = normal.rot_y(yaw_angle).rot_x(pitch);
            // 背面剔除
            if n.z <= 0.01 {
                continue;
            }

            let mut mesh = egui::Mesh::with_texture(texture.id());
            let mut depth = 0.0;
            for (i, corner) in world.iter().enumerate() {
                let pos = Pos2::new(
                    center.x + corner.x * scale,
                    center.y - (corner.y - MODEL_CENTER_Y) * scale,
                );
                depth += corner.z;
                mesh.vertices.push(egui::epaint::Vertex {
                    pos,
                    uv: face.uvs[i],
                    color: Color32::WHITE,
                });
            }
            depth /= 4.0;
            mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
            faces.push((depth, mesh));
        }
    }

    // 远的先画
    faces.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, mesh) in faces {
        painter.add(egui::Shape::mesh(mesh));
    }

    // 边框与提示
    painter.rect_stroke(
        bg,
        p.corner(0),
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    if response.hovered() {
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 14.0),
            egui::Align2::CENTER_CENTER,
            "拖动旋转",
            super::font(11.5),
            p.text_faint,
        );
    }

    response
}

/// 纯展示用的小尺寸 3D 头像（只画头部）
pub fn head_preview(
    ui: &mut Ui,
    texture: Option<&egui::TextureHandle>,
    yaw: f32,
    size: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let Some(texture) = texture else { return };
    let painter = ui.painter().clone();
    let parts = [Part {
        center: V3::new(0.0, 0.0, 0.0),
        size: V3::new(8.0, 8.0, 8.0),
        uv: UvRect { u: 0.0, v: 0.0, w: 8.0, h: 8.0 },
        swing: 0.0,
    }];
    let scale = size / 14.0;
    let center = rect.center();
    let pitch = -0.12;
    let mut faces: Vec<(f32, egui::Mesh)> = Vec::new();
    for part in &parts {
        for face in box_faces(part.size, part.uv, 64.0) {
            let world: Vec<V3> = face
                .corners
                .iter()
                .map(|c| c.add(part.center).rot_y(yaw).rot_x(pitch))
                .collect();
            let n = face.normal.rot_y(yaw).rot_x(pitch);
            if n.z <= 0.01 {
                continue;
            }
            let mut mesh = egui::Mesh::with_texture(texture.id());
            let mut depth = 0.0;
            for (i, corner) in world.iter().enumerate() {
                depth += corner.z;
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: Pos2::new(center.x + corner.x * scale, center.y - corner.y * scale),
                    uv: face.uvs[i],
                    color: Color32::WHITE,
                });
            }
            mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
            faces.push((depth / 4.0, mesh));
        }
    }
    faces.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, mesh) in faces {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// 简单的 2D 资源包图标底色
pub fn placeholder_tile(ui: &mut Ui, p: &Palette, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), p.surface_alt);
}
