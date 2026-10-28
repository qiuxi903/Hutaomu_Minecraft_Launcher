use egui::{Align, Color32, Layout, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::icons;
use crate::ui::{self, widgets, ButtonOpts};
use crate::theme::Palette;

const REFERENCE_PROJECTS: [(&str, &str, &str, &str); 4] = [
    (
        "SJMCL",
        "https://github.com/UNIkeEN/SJMCL",
        "MIT",
        "界面布局、交互组织与整体信息架构的参考",
    ),
    (
        "PCL2 (Plain Craft Launcher 2)",
        "https://github.com/Hex-Dragon/PCL2",
        "自定义许可",
        "版本管理、游戏目录与启动流程的参考",
    ),
    (
        "HMCL (Hello Minecraft! Launcher)",
        "https://github.com/HMCL-dev/HMCL",
        "GPL-3.0",
        "加载器安装、Java 搜索与下载源的参考",
    ),
    (
        "Bakaxl",
        "https://github.com/Bakaxl",
        "MIT",
        "主题系统与视觉细节的参考",
    ),
];

const LIBRARIES: [(&str, &str, &str); 19] = [
    ("egui / eframe", "MIT OR Apache-2.0", "界面框架与窗口"),
    ("reqwest", "MIT OR Apache-2.0", "HTTP 客户端"),
    ("serde / serde_json", "MIT OR Apache-2.0", "配置与接口数据序列化"),
    ("image", "MIT OR Apache-2.0", "皮肤、背景图与 GIF 解码"),
    ("resvg / tiny-skia", "MPL-2.0", "SVG 图标光栅化"),
    ("zip", "MIT", "压缩包与整合包解压"),
    ("md-5 / sha1", "MIT OR Apache-2.0", "离线 UUID 与文件校验"),
    ("murmur2", "Apache-2.0", "CurseForge 文件指纹反查"),
    ("uuid", "MIT OR Apache-2.0", "账号与客户端标识"),
    ("rfd", "MIT", "系统文件选择对话框"),
    ("walkdir", "MIT OR Unlicense", "目录遍历与全盘 Java 搜索"),
    ("open", "MIT", "调用系统默认程序打开链接"),
    ("chrono", "MIT OR Apache-2.0", "时间处理"),
    ("anyhow / thiserror", "MIT OR Apache-2.0", "错误处理"),
    ("rand", "MIT OR Apache-2.0", "随机数"),
    ("base64", "MIT OR Apache-2.0", "皮肤属性解码"),
    ("percent-encoding", "MIT OR Apache-2.0", "URL 编码"),
    ("num_cpus", "MIT OR Apache-2.0", "并发线程数"),
    ("windows-sys / winresource", "MIT OR Apache-2.0", "Windows 系统接口与可执行文件图标"),
];

const DATA_SOURCES: [(&str, &str, &str); 12] = [
    ("Mojang API", "https://piston-meta.mojang.com", "游戏版本清单、客户端与依赖库"),
    ("BMCLAPI", "https://bmclapi2.bangbang93.com", "国内镜像加速"),
    ("Fabric Meta", "https://meta.fabricmc.net", "Fabric 加载器版本与描述"),
    ("Quilt Meta", "https://meta.quiltmc.org", "Quilt 加载器版本与描述"),
    ("MinecraftForge Maven", "https://maven.minecraftforge.net", "Forge 安装器与依赖"),
    ("NeoForged Maven", "https://maven.neoforged.net", "NeoForge 安装器与依赖"),
    ("Modrinth API", "https://api.modrinth.com", "Mod、光影、资源包与整合包"),
    (
        "CurseForge API",
        "https://api.curseforge.com",
        "CurseForge 平台的 Mod、光影、资源包与整合包",
    ),
    (
        "CurseForge 社区镜像",
        "https://api.curse.tools",
        "官方搜索接口临时不可用时的备用地址",
    ),
    ("MC 百科 MCMod", "https://www.mcmod.cn", "Mod 中文名、缩写与页面链接"),
    ("MC 镜像站翻译", "https://mod.mcimirror.top", "项目简介的中文翻译"),
    ("authlib-injector", "https://authlib-injector.yushi.moe", "皮肤站登录支持"),
];

/// 已经在真实环境里跑通的功能
const VERIFIED: [&str; 12] = [
    "版本下载与安装（含资源文件）",
    "原版启动",
    "Fabric 加载器安装与启动",
    "游戏与加载器一次装好",
    "Java 简要搜索",
    "Modrinth 搜索与版本查询",
    "CurseForge 搜索、下载与整合包安装",
    "CurseForge 本地文件指纹反查",
    "MCMod 中文名与官网跳转",
    "整合包独立安装",
    "皮肤站登录与续期",
    "界面各页面与自检截图",
];

/// 还没有实机验证的功能
const UNVERIFIED: [&str; 3] = [
    "Forge / NeoForge 安装器流程",
    "微软账号登录",
    "整合包首次安装（需真实整合包）",
];

pub fn ui(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(ui, &p, "关于", "");

    // ------------------------------------------------------------ 应用信息
    ui::card(ui, &p, |ui| {
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(72.0), Sense::hover());
            icons::paint(ui.ctx(), ui.painter(), "cube", rect, p.accent);
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("胡桃木 Minecraft 启动器")
                        .font(ui::font(21.0))
                        .color(p.text)
                        .strong(),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    widgets::badge(ui, &p, &format!("v{}", env!("CARGO_PKG_VERSION")), p.accent);
                    widgets::badge(ui, &p, "首个测试版", p.warning);
                    widgets::badge(ui, &p, "AGPL-3.0", p.text_weak);
                    widgets::badge(ui, &p, "Rust + egui", p.text_weak);
                });
            });
            // 按钮摆在右侧，两行两列，别拉成一长条
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui::button(ui, &p, ButtonOpts::secondary("GitHub 仓库").icon("git-branch"))
                            .clicked()
                        {
                            let _ = open::that("https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher");
                        }
                        if ui::button(ui, &p, ButtonOpts::secondary("更新日志").icon("clock")).clicked() {
                            let _ = open::that(
                                "https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher/commits/main",
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui::button(ui, &p, ButtonOpts::secondary("反馈问题").icon("external-link"))
                            .clicked()
                        {
                            let _ = open::that(
                                "https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher/issues",
                            );
                        }
                        if ui::button(ui, &p, ButtonOpts::secondary("打开启动器目录").icon("folder"))
                            .clicked()
                        {
                            crate::core::paths::reveal(&crate::core::paths::exe_dir());
                        }
                    });
                });
            });
        });
    });

    ui.add_space(12.0);

    // ------------------------------------------------------------ 功能验证
    ui.columns(2, |cols| {
        // 左：已验证与待验证
        let left_width = cols[0].available_width();
        ui::card(&mut cols[0], &p, |ui| {
            // 卡片自带内边距，宽度要自己收一下，否则会顶出这一栏
            ui.set_width((left_width - 36.0).max(220.0));
            widgets::section_title(ui, &p, "功能验证情况");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("已验证 {} 项", VERIFIED.len()))
                    .font(ui::font(13.0))
                    .color(p.success)
                    .strong(),
            );
            ui.add_space(8.0);
            // 一项一行，字号不大才排得下
            for item in VERIFIED.iter() {
                check_item(ui, &p, item, p.success, "check");
                ui.add_space(3.0);
            }
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(format!("待验证 {} 项", UNVERIFIED.len()))
                    .font(ui::font(13.0))
                    .color(p.warning)
                    .strong(),
            );
            ui.add_space(8.0);
            for item in UNVERIFIED {
                check_item(ui, &p, item, p.warning, "clock");
                ui.add_space(3.0);
            }
        });

        // 右：运行环境
        let right_width = cols[1].available_width();
        ui::card(&mut cols[1], &p, |ui| {
            ui.set_width((right_width - 36.0).max(220.0));
            widgets::section_title(ui, &p, "运行环境");
            // 六个键值排两列，一眼能扫完
            let rows: Vec<(&str, String)> = vec![
                ("启动器版本", env!("CARGO_PKG_VERSION").to_string()),
                (
                    "构建配置",
                    if cfg!(debug_assertions) { "Debug" } else { "Release" }.to_string(),
                ),
            ];
            // 一行一项，路径长了才有地方显示
            for (key, value) in rows.iter() {
                kv_row(ui, &p, key, value);
                ui.add_space(7.0);
            }
            ui.add_space(8.0);
            widgets::divider(ui, &p);
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("本地文件")
                    .font(ui::font(13.0))
                    .color(p.text)
                    .strong(),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui::button(ui, &p, ButtonOpts::ghost("启动器目录").icon("folder")).clicked() {
                    crate::core::paths::reveal(&crate::core::paths::exe_dir());
                }
                if ui::button(ui, &p, ButtonOpts::ghost("日志目录").icon("terminal")).clicked() {
                    crate::core::paths::ensure_dir(&crate::core::paths::logs_dir());
                    crate::core::paths::reveal(&crate::core::paths::logs_dir());
                }
                if ui::button(ui, &p, ButtonOpts::ghost("配置文件").icon("file-text")).clicked() {
                    crate::core::paths::reveal(&crate::core::paths::config_file());
                }
                if ui::button(ui, &p, ButtonOpts::ghost("游戏目录").icon("cube")).clicked() {
                    crate::core::paths::reveal(&app.cfg.game_dir);
                }
            });
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("作者：邱息 / qiuxi903")
                    .font(ui::font(12.0))
                    .color(p.text_faint),
            );
        });
    });

    ui.add_space(12.0);

    // ------------------------------------------------------------ 参考项目
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "参考项目");
        ui.add_space(6.0);
        ui.columns(2, |cols| {
            for (i, (name, url, license, note)) in REFERENCE_PROJECTS.into_iter().enumerate() {
                let col = if i % 2 == 0 { &mut cols[0] } else { &mut cols[1] };
                // 卡片自带内边距，宽度要自己收一下，否则会顶出这一栏
                let width = col.available_width();
                ui::card(col, &p, |ui| {
                    ui.set_width((width - 36.0).max(160.0));
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(name)
                                .font(ui::font(13.5))
                                .color(p.text)
                                .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui::ghost(ui, &p, "打开").clicked() {
                                let _ = open::that(url);
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        widgets::badge(ui, &p, license, p.text_faint);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(note)
                                .font(ui::font(11.5))
                                .color(p.text_weak),
                        );
                    });
                });
                col.add_space(10.0);
            }
        });
    });

    ui.add_space(12.0);

    // ------------------------------------------------------------ 组件与数据源
    ui.columns(2, |cols| {
        // 左：开源组件
        let left_width = cols[0].available_width();
        ui::card(&mut cols[0], &p, |ui| {
            // 卡片自带内边距，宽度要自己收一下，否则会顶出这一栏
            ui.set_width((left_width - 36.0).max(220.0));
            widgets::section_title(ui, &p, "使用的开源组件");
            ui.add_space(6.0);
            for (name, license, usage) in LIBRARIES.into_iter() {
                component_row(ui, &p, name, usage, license);
            }
        });

        // 右：数据来源
        let right_width = cols[1].available_width();
        ui::card(&mut cols[1], &p, |ui| {
            ui.set_width((right_width - 36.0).max(220.0));
            widgets::section_title(ui, &p, "数据来源");
            ui.add_space(6.0);
            for (name, url, usage) in DATA_SOURCES.into_iter() {
                source_row(ui, &p, name, url, usage);
            }
        });
    });

    ui.add_space(12.0);

    // ------------------------------------------------------------ 协议与声明
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "开源协议与声明");
        ui.label(
            egui::RichText::new(
                "本启动器与 Mojang Studios、Microsoft 无任何关联。「Minecraft」为 Mojang Studios 的商标。\
                 启动器不包含任何游戏本体文件，所有游戏资源均在用户操作下从官方或镜像渠道下载。",
            )
            .font(ui::font(12.5))
            .color(p.text_weak),
        );
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("查看 AGPL-3.0 全文").icon("scale")).clicked() {
                let _ = open::that("https://www.gnu.org/licenses/agpl-3.0.html");
            }
            if ui::button(ui, &p, ButtonOpts::secondary("打开本地 LICENSE").icon("book")).clicked() {
                let path = crate::core::paths::exe_dir().join("LICENSE");
                if path.is_file() {
                    let _ = open::that(path);
                } else {
                    let _ = open::that(
                        "https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher/blob/main/LICENSE",
                    );
                }
            }
        });
    });

}

// ------------------------------------------------------------------ 小部件

/// 带勾的条目，图标跟文字对齐
fn check_item(ui: &mut Ui, p: &Palette, text: &str, color: Color32, icon: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        icons::paint(
            ui.ctx(),
            ui.painter(),
            icon,
            Rect::from_center_size(rect.center(), Vec2::splat(13.0)),
            color,
        );
        ui.add_space(7.0);
        // 剩下的宽度放不下就截断，文字不会顶出这一栏
        ui.add(
            egui::Label::new(egui::RichText::new(text).font(ui::font(12.0)).color(p.text_weak))
                .truncate(),
        );
    });
}

/// 运行环境里的一行：左边是名字，右边是值
///
/// 值一般是路径，可能很长，截断后把完整内容放进悬停提示里。
fn kv_row(ui: &mut Ui, p: &Palette, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(key).font(ui::font(12.5)).color(p.text_weak))
                .truncate(),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let resp = ui.add(
                egui::Label::new(
                    egui::RichText::new(value)
                        .font(ui::font(12.5))
                        .color(p.accent)
                        .strong(),
                )
                .truncate(),
            );
            // 路径被截断时，悬停能看到完整内容
            resp.on_hover_text(value);
        });
    });
}

/// 开源组件一行：名字、用途、许可
///
/// 名字占左边，许可靠右对齐。右对齐那一格里只放一个标签，
/// 放两个就会互相压字。
fn component_row(ui: &mut Ui, p: &Palette, name: &str, usage: &str, license: &str) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(name).font(ui::font(12.0)).color(p.text)).truncate(),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(license)
                        .font(ui::font(10.5))
                        .color(p.text_faint),
                )
                .truncate(),
            );
        });
    });
    ui.label(
        egui::RichText::new(usage)
            .font(ui::font(10.5))
            .color(p.text_weak),
    );
    ui.add_space(6.0);
}

/// 数据来源一行：名字、用途、域名
fn source_row(ui: &mut Ui, p: &Palette, name: &str, url: &str, usage: &str) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(name).font(ui::font(12.0)).color(p.text)).truncate(),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::ghost(ui, p, "访问").clicked() {
                let _ = open::that(url);
            }
        });
    });
    // 地址太长会挤成一片，这里只显示域名，完整地址放悬停提示
    let short = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    ui.label(
        egui::RichText::new(short)
            .font(ui::font(10.5))
            .color(p.text_faint),
    )
    .on_hover_text(url);
    ui.label(
        egui::RichText::new(usage)
            .font(ui::font(10.5))
            .color(p.text_weak),
    );
    ui.add_space(7.0);
}
