use egui::{Align, Layout, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::icons;
use crate::ui::{self, widgets, ButtonOpts};

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

const LIBRARIES: [(&str, &str, &str); 18] = [
    ("egui / eframe", "MIT OR Apache-2.0", "界面框架与窗口"),
    ("reqwest", "MIT OR Apache-2.0", "HTTP 客户端"),
    ("serde / serde_json", "MIT OR Apache-2.0", "配置与接口数据序列化"),
    ("image", "MIT OR Apache-2.0", "皮肤、背景图与 GIF 解码"),
    ("resvg / tiny-skia", "MPL-2.0", "SVG 图标光栅化"),
    ("zip", "MIT", "压缩包与整合包解压"),
    ("md-5 / sha1", "MIT OR Apache-2.0", "离线 UUID 与文件校验"),
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

const DATA_SOURCES: [(&str, &str, &str); 10] = [
    ("Mojang API", "https://piston-meta.mojang.com", "游戏版本清单、客户端与依赖库"),
    ("BMCLAPI", "https://bmclapi2.bangbang93.com", "国内镜像加速"),
    ("Fabric Meta", "https://meta.fabricmc.net", "Fabric 加载器版本与描述"),
    ("Quilt Meta", "https://meta.quiltmc.org", "Quilt 加载器版本与描述"),
    ("MinecraftForge Maven", "https://maven.minecraftforge.net", "Forge 安装器与依赖"),
    ("NeoForged Maven", "https://maven.neoforged.net", "NeoForge 安装器与依赖"),
    ("Modrinth API", "https://api.modrinth.com", "Mod、光影、资源包与整合包"),
    ("MC 百科 MCMod", "https://www.mcmod.cn", "Mod 中文名、缩写与页面链接"),
    ("MC 镜像站翻译", "https://mod.mcimirror.top", "项目简介的中文翻译"),
    ("authlib-injector", "https://authlib-injector.yushi.moe", "皮肤站登录支持"),
];

pub fn ui(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(ui, &p, "关于", "胡桃木 Minecraft 启动器");

    // 应用信息
    ui::card(ui, &p, |ui| {
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(72.0), Sense::hover());
            icons::paint(ui.ctx(), ui.painter(), "cube", rect, p.accent);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("胡桃木 Minecraft 启动器")
                        .font(ui::font(21.0))
                        .color(p.text)
                        .strong(),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Hutaomu Minecraft Launcher · htmcl")
                        .font(ui::font(12.5))
                        .color(p.text_weak),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    widgets::badge(ui, &p, &format!("v{}", env!("CARGO_PKG_VERSION")), p.accent);
                    widgets::badge(ui, &p, "首个测试版", p.warning);
                    widgets::badge(ui, &p, "AGPL-3.0", p.text_weak);
                    widgets::badge(ui, &p, "Rust + egui", p.text_weak);
                });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    if ui::button(ui, &p, ButtonOpts::secondary("GitHub 仓库").icon("git-branch")).clicked()
                    {
                        let _ = open::that("https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher");
                    }
                    if ui::button(ui, &p, ButtonOpts::secondary("反馈问题").icon("external-link")).clicked()
                    {
                        let _ = open::that("https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher/issues");
                    }
                    if ui::button(ui, &p, ButtonOpts::secondary("打开启动器目录").icon("folder")).clicked()
                    {
                        crate::core::paths::reveal(&crate::core::paths::exe_dir());
                    }
                });
            });
        });
    });

    ui.add_space(14.0);

    // 测试版声明
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "测试版声明");
        ui.label(
            egui::RichText::new(format!(
                "当前版本 {} 是胡桃木启动器的公开测试版（Beta）。功能仍在快速迭代，配置文件字段、目录结构与界面都可能在下个版本变化；升级前请备份 accounts.json 与游戏目录；遇到问题请到 GitHub Issues 反馈。",
                env!("CARGO_PKG_VERSION")
            ))
            .font(ui::font(12.5))
            .color(p.text_weak),
        );
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(
                "已在真实环境验证：版本下载与安装（含资源文件）、原版启动、Fabric 加载器安装与启动、游戏与加载器一次装好、Java 简要搜索、Modrinth 搜索与版本查询、MCMod 中文名与官网跳转、界面各页面。尚未实机验证：Forge / NeoForge 安装器流程、微软账号登录、皮肤站登录（需要真实账号）。",
            )
            .font(ui::font(12.5))
            .color(p.text_weak),
        );
    });

    ui.add_space(14.0);

    // 参考项目
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "参考项目");
        ui.label(
            egui::RichText::new("本项目在交互设计与功能组织上参考了以下开源/公开项目，代码为完全重写，未复制其源码。")
                .font(ui::font(12.0))
                .color(p.text_weak),
        );
        ui.add_space(12.0);
        for (name, url, license, note) in REFERENCE_PROJECTS {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(name)
                                .font(ui::font(13.5))
                                .color(p.text)
                                .strong(),
                        );
                        widgets::badge(ui, &p, license, p.text_faint);
                    });
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(note)
                            .font(ui::font(12.0))
                            .color(p.text_weak),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::ghost(ui, &p, "打开").clicked() {
                        let _ = open::that(url);
                    }
                });
            });
            ui.add_space(10.0);
        }
    });

    ui.add_space(14.0);

    // 开源组件
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "使用的开源组件");
        for (name, license, usage) in LIBRARIES {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .font(ui::font(12.5))
                        .color(p.text),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(license)
                            .font(ui::font(11.0))
                            .color(p.text_faint),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(usage)
                            .font(ui::font(11.5))
                            .color(p.text_weak),
                    );
                });
            });
            ui.add_space(5.0);
        }
    });

    ui.add_space(14.0);

    // 数据来源
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "数据来源");
        for (name, url, usage) in DATA_SOURCES {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(name)
                            .font(ui::font(12.5))
                            .color(p.text),
                    );
                    ui.label(
                        egui::RichText::new(usage)
                            .font(ui::font(11.5))
                            .color(p.text_weak),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::ghost(ui, &p, "访问").clicked() {
                        let _ = open::that(url);
                    }
                    ui.label(
                        egui::RichText::new(url)
                            .font(ui::font(11.0))
                            .color(p.text_faint),
                    );
                });
            });
            ui.add_space(8.0);
        }
    });

    ui.add_space(14.0);

    // 协议与声明
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "开源协议与声明");
        ui.label(
            egui::RichText::new(
                "本项目以 GNU Affero General Public License v3.0（AGPL-3.0）协议开源。你可以自由使用、修改与分发，\
                 但分发修改后的版本时必须同样以 AGPL-3.0 开源，并提供完整源代码。",
            )
            .font(ui::font(12.5))
            .color(p.text_weak),
        );
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(
                "本启动器与 Mojang Studios、Microsoft 无任何关联。「Minecraft」为 Mojang Studios 的商标。\
                 启动器不包含任何游戏本体文件，所有游戏资源均在用户操作下从官方或镜像渠道下载。",
            )
            .font(ui::font(12.5))
            .color(p.text_weak),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("查看 AGPL-3.0 全文").icon("scale")).clicked() {
                let _ = open::that("https://www.gnu.org/licenses/agpl-3.0.html");
            }
            if ui::button(ui, &p, ButtonOpts::secondary("打开本地 LICENSE").icon("book")).clicked() {
                let path = crate::core::paths::exe_dir().join("LICENSE");
                if path.is_file() {
                    let _ = open::that(path);
                } else {
                    let _ = open::that("https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher/blob/main/LICENSE");
                }
            }
        });
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("作者：邱息 / qiuxi903")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
    });

    ui.add_space(14.0);

    // 运行环境
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "运行环境");
        widgets::key_value(ui, &p, "启动器版本", env!("CARGO_PKG_VERSION"));
        widgets::key_value(ui, &p, "构建配置", if cfg!(debug_assertions) { "Debug" } else { "Release" });
        widgets::key_value(ui, &p, "配置文件", &crate::core::paths::config_file().display().to_string());
        widgets::key_value(ui, &p, "账号文件", &crate::core::paths::accounts_file().display().to_string());
        widgets::key_value(ui, &p, "游戏目录", &app.cfg.game_dir_display());
        widgets::key_value(ui, &p, "日志目录", &crate::core::paths::logs_dir().display().to_string());
    });

    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("胡桃木 Minecraft 启动器 · 用 Rust 写的 Minecraft 启动器")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
        let _ = Pos2::ZERO;
        let _ = Rect::NOTHING;
    });
}
