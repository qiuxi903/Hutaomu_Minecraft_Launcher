//! 轻量 3D 皮肤预览：把皮肤贴图映射到方块上，支持第二层（帽子 / 外套），
//! 使用 egui 的 Mesh 做软件变换与深度排序，不依赖任何 3D 引擎。

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

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

/// size 是几何尺寸（可能是放大后的第二层），uv_size 是贴图布局使用的原始尺寸
fn box_faces(size: V3, uv_size: V3, uv: UvRect, tex: f32) -> [Face; 6] {
    let (w, h, d) = (uv_size.x, uv_size.y, uv_size.z);
    let (x0, x1) = (-size.x / 2.0, size.x / 2.0);
    let (y0, y1) = (-size.y / 2.0, size.y / 2.0);
    let (z0, z1) = (-size.z / 2.0, size.z / 2.0);
    let (u, v) = (uv.u, uv.v);

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

#[derive(Clone, Copy)]
struct Part {
    center: V3,
    /// 几何尺寸
    size: V3,
    /// 贴图布局尺寸（第二层沿用原始尺寸）
    uv_size: V3,
    uv: UvRect,
    /// 以顶部为轴的摆动角度（弧度）
    swing: f32,
    /// 是否为第二层
    overlay: bool,
    /// 是否不画底面
    ///
    /// 帽子层在皮肤贴图里只有五个面，底面是空的，
    /// 照着第一层画底面的话，脑袋下面会多出一块不该有的颜色。
    no_bottom: bool,
}

/// 帽子层放大比例
///
/// Minecraft 只放大帽子（12.5%），外套、袖子、裤子的第二层与第一层同尺寸，
/// 靠 alpha 区分。全部放大的话袖子会粗一圈，看着整个都错位。
const HAT_SCALE: f32 = 1.125;

fn build_parts(slim: bool, time: f32, has_overlay: bool) -> Vec<Part> {
    let arm_w = if slim { 3.0 } else { 4.0 };
    let arm_x = 4.0 + arm_w / 2.0;
    let idle = (time * 1.6).sin() * 0.06;

    let mut parts = vec![
        // 头
        Part {
            center: V3::new(0.0, 28.0, 0.0),
            size: V3::new(8.0, 8.0, 8.0),
            uv_size: V3::new(8.0, 8.0, 8.0),
            uv: UvRect { u: 0.0, v: 0.0, w: 8.0, h: 8.0 },
            swing: 0.0,
            overlay: false,
            no_bottom: false,
        },
        // 身体
        Part {
            center: V3::new(0.0, 18.0, 0.0),
            size: V3::new(8.0, 12.0, 4.0),
            uv_size: V3::new(8.0, 12.0, 4.0),
            uv: UvRect { u: 16.0, v: 16.0, w: 8.0, h: 12.0 },
            swing: 0.0,
            overlay: false,
            no_bottom: false,
        },
        // 右臂（角色右侧，屏幕左侧）
        Part {
            center: V3::new(-arm_x, 18.0, 0.0),
            size: V3::new(arm_w, 12.0, 4.0),
            uv_size: V3::new(arm_w, 12.0, 4.0),
            uv: UvRect { u: 40.0, v: 16.0, w: arm_w, h: 12.0 },
            swing: idle,
            overlay: false,
            no_bottom: false,
        },
        // 左臂
        Part {
            center: V3::new(arm_x, 18.0, 0.0),
            size: V3::new(arm_w, 12.0, 4.0),
            uv_size: V3::new(arm_w, 12.0, 4.0),
            uv: UvRect { u: 32.0, v: 48.0, w: arm_w, h: 12.0 },
            swing: -idle,
            overlay: false,
            no_bottom: false,
        },
        // 右腿
        Part {
            center: V3::new(-2.0, 6.0, 0.0),
            size: V3::new(4.0, 12.0, 4.0),
            uv_size: V3::new(4.0, 12.0, 4.0),
            uv: UvRect { u: 0.0, v: 16.0, w: 4.0, h: 12.0 },
            swing: 0.0,
            overlay: false,
            no_bottom: false,
        },
        // 左腿
        Part {
            center: V3::new(2.0, 6.0, 0.0),
            size: V3::new(4.0, 12.0, 4.0),
            uv_size: V3::new(4.0, 12.0, 4.0),
            uv: UvRect { u: 16.0, v: 48.0, w: 4.0, h: 12.0 },
            swing: 0.0,
            overlay: false,
            no_bottom: false,
        },
    ];

    if has_overlay {
        // 帽子层：Minecraft 里放大 12.5%，底面是空的
        let hat = 8.0 * HAT_SCALE;
        let overlays = [
            // 帽子层 (32,0)
            Part {
                center: V3::new(0.0, 28.375, 0.0),
                size: V3::new(hat, hat, hat),
                uv_size: V3::new(8.0, 8.0, 8.0),
                uv: UvRect { u: 32.0, v: 0.0, w: 8.0, h: 8.0 },
                swing: 0.0,
                overlay: true,
                no_bottom: true,
            },
            // 外套层 (16,32)：与第一层同尺寸
            Part {
                center: V3::new(0.0, 18.0, 0.0),
                size: V3::new(8.0, 12.0, 4.0),
                uv_size: V3::new(8.0, 12.0, 4.0),
                uv: UvRect { u: 16.0, v: 32.0, w: 8.0, h: 12.0 },
                swing: 0.0,
                overlay: true,
                no_bottom: false,
            },
            // 右臂外层 (40,32)
            Part {
                center: V3::new(-arm_x, 18.0, 0.0),
                size: V3::new(arm_w, 12.0, 4.0),
                uv_size: V3::new(arm_w, 12.0, 4.0),
                uv: UvRect { u: 40.0, v: 32.0, w: arm_w, h: 12.0 },
                swing: idle,
                overlay: true,
                no_bottom: false,
            },
            // 左臂外层 (48,48)
            Part {
                center: V3::new(arm_x, 18.0, 0.0),
                size: V3::new(arm_w, 12.0, 4.0),
                uv_size: V3::new(arm_w, 12.0, 4.0),
                uv: UvRect { u: 48.0, v: 48.0, w: arm_w, h: 12.0 },
                swing: -idle,
                overlay: true,
                no_bottom: false,
            },
            // 右腿外层 (0,32)
            Part {
                center: V3::new(-2.0, 6.0, 0.0),
                size: V3::new(4.0, 12.0, 4.0),
                uv_size: V3::new(4.0, 12.0, 4.0),
                uv: UvRect { u: 0.0, v: 32.0, w: 4.0, h: 12.0 },
                swing: 0.0,
                overlay: true,
                no_bottom: false,
            },
            // 左腿外层 (0,48)
            Part {
                center: V3::new(2.0, 6.0, 0.0),
                size: V3::new(4.0, 12.0, 4.0),
                uv_size: V3::new(4.0, 12.0, 4.0),
                uv: UvRect { u: 0.0, v: 48.0, w: 4.0, h: 12.0 },
                swing: 0.0,
                overlay: true,
                no_bottom: false,
            },
        ];
        parts.extend_from_slice(&overlays);
    }

    parts
}

/// 绘制皮肤预览。返回的 response 支持拖拽旋转。
#[allow(clippy::too_many_arguments)]
pub fn skin_preview(
    ui: &mut Ui,
    p: &Palette,
    texture: Option<&egui::TextureHandle>,
    cutout: Option<&egui::TextureHandle>,
    model: SkinModel,
    has_overlay: bool,
    yaw: &mut f32,
    height: f32,
    animate: bool,
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::drag());

    // 直接借用 painter：克隆一份会把这一帧已绘制的图形全拷一遍，
    // 页面上元素一多就是纯浪费，账号页有皮肤预览时尤其明显
    let painter = ui.painter();
    let bg = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), rect.height()));
    painter.rect_filled(bg, p.corner(0), p.surface_alt);

    if response.dragged() {
        *yaw += response.drag_delta().x * 0.012;
    }
    if animate {
        // 只在拖动时转，不自己转圈：默认就停在斜侧的角度
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }

    // 地面阴影
    let shadow_center = Pos2::new(rect.center().x, rect.top() + height * 0.92);
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
    let parts = build_parts(model == SkinModel::Slim, if animate { time } else { 0.0 }, has_overlay);

    // 模型总高 32 个皮肤像素（脚底 y=0，头顶 y=32），垂直居中放置
    const MODEL_CENTER_Y: f32 = 16.0;
    let scale = height * 0.80 / 34.0;
    let center = Pos2::new(rect.center().x, rect.top() + height * 0.52);
    // 略微俯视：这样才能看到头顶（帽子的第二层就在顶上）
    let pitch = 0.12_f32;
    let yaw_angle = *yaw;

    let mut faces: Vec<(f32, egui::Mesh)> = Vec::new();

    for part in &parts {
        for (index, face) in box_faces(part.size, part.uv_size, part.uv, 64.0)
            .into_iter()
            .enumerate()
        {
            // 帽子层没有底面
            if part.no_bottom && index == 5 {
                continue;
            }
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
            if n.z <= 0.01 {
                continue;
            }

            let mut mesh = egui::Mesh::with_texture(if part.overlay {
                // 第二层用镂空纹理：半透明像素丢掉，不和第一层混色
                cutout.map(|t| t.id()).unwrap_or_else(|| texture.id())
            } else {
                texture.id()
            });
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
            // 第二层稍微靠前一点，保证盖在第一层之上
            if part.overlay {
                depth += 0.02;
            }
            mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
            faces.push((depth, mesh));
        }
    }

    // 远的先画；第二层略靠外，会自然排在基础层之后
    faces.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, mesh) in faces {
        painter.add(egui::Shape::mesh(mesh));
    }

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
pub fn head_preview(ui: &mut Ui, texture: Option<&egui::TextureHandle>, yaw: f32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let Some(texture) = texture else { return };
    let painter = ui.painter();
    let head = Part {
        center: V3::new(0.0, 0.0, 0.0),
        size: V3::new(8.0, 8.0, 8.0),
        uv_size: V3::new(8.0, 8.0, 8.0),
        uv: UvRect { u: 0.0, v: 0.0, w: 8.0, h: 8.0 },
        swing: 0.0,
        overlay: false,
        no_bottom: false,
    };
    let scale = size / 14.0;
    let center = rect.center();
    // 略微俯视，和游戏里看自己角色的角度一致
    let pitch = 0.14;
    let mut faces: Vec<(f32, egui::Mesh)> = Vec::new();
    for face in box_faces(head.size, head.uv_size, head.uv, 64.0) {
        let world: Vec<V3> = face
            .corners
            .iter()
            .map(|c| c.add(head.center).rot_y(yaw).rot_x(pitch))
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
    faces.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, mesh) in faces {
        painter.add(egui::Shape::mesh(mesh));
    }
}
