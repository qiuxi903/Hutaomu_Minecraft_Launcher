# 胡桃木 Minecraft 启动器 · htmcl

Hutaomu Minecraft Launcher —— 用 Rust 编写的 Windows Minecraft 启动器。单文件分发，配置、账号、主题与游戏数据全部位于启动器同目录；不写注册表，不使用 WebView，界面由 egui 直接在 GPU 上渲染。

[![Status](https://img.shields.io/badge/status-beta-orange.svg)](#测试版声明)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.82%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-Windows%20x64-lightgrey.svg)](#构建)

---

## 下载

| 版本 | 说明 |
| --- | --- |
| [v0.1.0-beta.4](https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher/releases/tag/v0.1.0-beta.4) | 第四个公开测试版（Pre-release），Windows x64 |

发行包 `htmcl-v0.1.0-beta.4-windows-x64.zip` 内含 `htmcl.exe`、`README.md` 与 `LICENSE`。解压后双击 `htmcl.exe` 即可，无需安装。校验值见发行页的 `SHA256SUMS.txt`。

从源码构建请见 [构建](#构建)。

---

## 目录

- [测试版声明](#测试版声明)
- [功能](#功能)
- [界面](#界面)
- [技术栈与架构](#技术栈与架构)
- [构建](#构建)
- [运行与命令行](#运行与命令行)
- [配置文件](#配置文件)
- [数据来源与网络策略](#数据来源与网络策略)
- [已知限制与路线图](#已知限制与路线图)
- [参与开发](#参与开发)
- [参考项目](#参考项目)
- [许可证与声明](#许可证与声明)

---

## 测试版声明

当前版本 **v0.1.0-beta.4** 是项目的第四个公开测试版（Beta）。

- 功能仍在快速迭代，配置文件字段、目录结构与界面布局都可能在下个版本变化；
- 升级前请备份 `accounts.json` 与游戏目录；
- 已在真实环境验证：版本下载与安装、原版启动、Fabric 加载器安装与启动、Java 简要搜索、Modrinth 搜索与版本查询、三个中文资讯源、皮肤站接口错误处理、全部界面页面；
- **尚未实机验证**：Forge / NeoForge 安装器流程、微软账号登录、皮肤站登录（这三项需要更完整的网络环境或真实账号）；
- 遇到问题请提交 [Issue](https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher/issues)，并附上 `logs/` 下的日志。

## 功能

### 游戏与版本

- 从 Mojang 官方源或 BMCLAPI 镜像获取版本清单（当前 900+ 版本，含正式版、快照、远古版本）；
- 完整安装流程：版本描述、客户端 jar、依赖库（按 `rules` 过滤平台）、natives 解压、资源索引与资源对象、日志配置、旧版 virtual 资源目录；
- 下载支持多线程并发、SHA1 校验、失败重试；镜像缺失的文件自动回退官方源；
- 版本隔离：每个版本使用独立的 `mods`、`saves`、`config` 目录；
- 单版本可单独覆盖内存、Java 与 JVM 参数；
- 启动参数按版本 JSON 的 `arguments` / `minecraftArguments` 模板展开，兼容 1.6 之前的老版本；
- 游戏进程输出实时写入 `logs/game-*.log`，界面内可查看并强制结束；
- **下载中心**：右下角悬浮面板显示每个任务的进度、字节数、实时速度、已用时间与当前动作，可随时取消；所有下载与内容安装都会自动弹出。

### 加载器

| 加载器 | 实现方式 |
| --- | --- |
| Fabric | Fabric Meta `profile/json`，写入版本目录并补齐依赖库 |
| Quilt | Quilt Meta `profile/json` |
| Forge | 解析 Maven 元数据，下载官方安装器并执行 `--installClient` |
| NeoForge | 同上；1.20.1 走 `net/neoforged/forge`，更高版本走 `net/neoforged/neoforge` |

安装加载器时会自动补齐对应的原版版本。安装器输出会实时回传到任务进度。

### 内容

接入 [Modrinth](https://modrinth.com) 开放 API：

- 搜索 Mod、光影、资源包、整合包，可按当前游戏版本与加载器过滤；
- 安装到对应目录：`mods/`、`shaderpacks/`、`resourcepacks/`；
- Mod 的必需前置依赖自动安装（一层，避免递归）；
- `.mrpack` 整合包：解析 `modrinth.index.json`、按清单下载、写入 `overrides` 与 `client-overrides`。

### 版本管理

- 左侧版本列表，支持按「正式版 / 快照 / 加载器 / 其他」筛选与搜索；
- 右侧详情分为「概览 / 设置 / 存档 / 截图」四个标签；
- 概览显示 MC 版本、加载器、继承关系、启动次数、最后启动时间、目录与占用空间；
- 设置可为单个版本覆盖内存、Java、JVM 参数与独立游戏目录；
- 存档页列出所有含 `level.dat` 的世界，截图页显示缩略图；
- 支持设为当前、启动、重命名、复制、删除、打开版本目录与游戏目录。

### 账号

| 类型 | 说明 |
| --- | --- |
| 离线登录 | 按 Mojang 规则生成 UUID：`MD5("OfflinePlayer:" + name)` 并设置 v3 UUID 版本位 |
| 微软正版 | OAuth 2.0 设备码流程 → Xbox Live → XSTS → Minecraft Services → 档案与皮肤 |
| 皮肤站 | Yggdrasil / Blessing Skin 协议，默认 `https://skin.hutaomu.cn/api/yggdrasil`，兼容自定义地址 |

- 皮肤站账号启动时自动下载并挂载 [authlib-injector](https://github.com/yushijinhun/authlib-injector)；
- 皮肤来源优先级：本地 PNG → 账号皮肤 → 按用户名程序化生成的默认皮肤；
- 支持经典（4px 手臂）与纤细（3px 手臂）两种模型，可自动识别；
- 3D 皮肤预览由项目自行实现（立方体模型 + 纹理映射 + 背面剔除 + 深度排序），支持第二层（帽子 / 外套），可拖拽旋转；
- 账号列表与侧边栏头像取自皮肤头部（含帽子层），而非整张皮肤。

### Java

- **简要搜索**：注册表 `JavaSoft`、常见厂商目录（Zulu / Temurin / Microsoft / Corretto / Semeru / GraalVM / Liberica / Dragonwell 等）、`JAVA_HOME`、`PATH`、官方启动器自带运行时；
- **详细搜索**：遍历所有固定磁盘查找 `bin/java.exe`，跳过系统与缓存目录，支持随时取消；
- 解析 `java -version` 得到版本、主版本、位数与厂商；
- 按游戏版本自动匹配：1.17+ → 16+、1.18+ → 17+、1.20.5+ → 21+；2026 起的 `26.x` 版本号视为 21+；
- 支持手动指定 `java.exe`，搜索结果缓存到配置。

### 界面与主题

- 无边框窗口，最小化 / 最大化 / 关闭按钮自绘，支持拖动与双击最大化；Windows 11 下使用 DWM 圆角；
- 94 个 24×24 线性 SVG 图标，构建时内嵌进可执行文件，无 emoji；
- 6 套内置主题 + `themes/` 目录下的自定义主题；
- 支持静态图片与 GIF 动态背景，可调不透明度与内容遮罩；
- 首页按「启动区 + 资讯 + 皮肤预览」组织：启动是最主要的动作放在首屏顶部，资讯作为主内容区，皮肤与快捷入口在右栏；
- 首页的「游戏配置」是二级弹窗，通用游戏设置集中在「设置 - 游戏」，按通用 / 内存 / 启动参数 / 游戏目录分类；
- 设置页使用左侧分类导航（外观 / 下载 / 游戏 / Java / 资讯 / 其他）；
- 界面动效：按钮悬停与按压插值、侧栏选中指示条、页面切换淡入、弹窗与下拉展开动画、提示条滑入、资讯卡片悬停；
- 主题色、圆角、字体缩放、界面动画均可调整；
- 内置三个中文资讯源：Minecraft 官方资讯（与 PCL2-CE 同源）、苦力怕论坛 RSS、中文 Minecraft Wiki 最近更改；也可切换为自定义 JSON / RSS 地址。

## 界面

| 首页：启动区 + 资讯 | 下载游戏 |
| --- | --- |
| ![首页](docs/screenshots/01-home.png) | ![下载游戏](docs/screenshots/02-download.png) |

| Mod 搜索 | 版本管理 |
| --- | --- |
| ![Mod](docs/screenshots/04-mod.png) | ![版本管理](docs/screenshots/05-versions.png) |

| 账号与 3D 皮肤 | 设置（左侧分类） |
| --- | --- |
| ![账号](docs/screenshots/06-accounts.png) | ![设置](docs/screenshots/07-settings-game.png) |

| 游戏配置（二级弹窗） | 下载中心 |
| --- | --- |
| ![游戏配置](docs/screenshots/08-home-config.png) | ![下载中心](docs/screenshots/09-download-center.png) |

## 技术栈与架构

### 技术选型

| 领域 | 选择 | 原因 |
| --- | --- | --- |
| 语言 | Rust 2021 | 单文件静态分发，无运行时依赖 |
| GUI | egui / eframe 0.35（glow） | 即时模式，便于自绘控件与标题栏；不引入 WebView |
| HTTP | reqwest（blocking + native-tls） | 在后台线程同步下载，避免引入异步运行时 |
| 图片 / GIF | image | 皮肤、背景与动态背景帧解码 |
| SVG | resvg / tiny-skia | 图标按主题色光栅化并缓存 |
| 压缩包 | zip | natives 解压与 `.mrpack` |
| 哈希 | md-5 / sha1（RustCrypto） | 离线 UUID 与文件校验 |
| 平台 | windows-sys / winresource | DWM 圆角、注册表读取、exe 图标资源 |

### 模块结构

```
src/
├── main.rs              入口、窗口选项、命令行解析
├── app.rs               应用状态、事件循环、页面路由、标题栏、任务与图片队列
├── selftest.rs          端到端自检
├── icons.rs             SVG 光栅化与纹理缓存
├── theme/mod.rs         主题定义、调色板推导、背景帧加载
├── ui/
│   ├── mod.rs           字体注入（系统中文字体）与 egui 样式安装
│   ├── widgets.rs       通用控件：按钮、输入框、开关、下拉、滑块、模态、Toast 等
│   └── skin3d.rs        软件 3D 皮肤渲染
├── views/               首页 / 下载 / 版本管理 / 账号 / 设置 / 关于
└── core/                业务逻辑（不依赖 UI）
    ├── config.rs        配置模型与原子写入
    ├── paths.rs         目录解析与校验
    ├── download.rs      多线程下载、镜像重定向、系统代理、进度
    ├── java.rs          Java 搜索与版本匹配
    ├── log.rs           文件 logger 与内存日志缓冲
    ├── news.rs          资讯解析（JSON / RSS / Atom）
    ├── skin.rs          皮肤解析与默认皮肤程序化生成
    ├── auth/            offline / microsoft / authlib
    ├── content/         modrinth 搜索、版本、安装、整合包
    └── minecraft/       manifest / version / install / launch / loaders
```

`core` 不引用任何 UI 类型；UI 通过后台任务与 `CoreEvent` 与业务层通信。

### 线程与事件模型

- UI 运行在主线程（winit 事件循环 + egui）；
- 所有网络与文件操作在 `std::thread` 工作线程执行；
- 工作线程通过 `mpsc::Sender<CoreEvent>` 回传结果与进度，`App::handle_events` 每帧 `try_recv()` 消费；
- 工作线程持有 `egui::Context` 克隆，发送事件后调用 `request_repaint()` 唤醒界面；
- 下载进度使用 `Arc<DownloadProgress>`（原子计数 + 消息锁），由独立的观察线程按 180ms 推送；
- 远程图片使用固定的 3 个工作线程 + 共享队列，避免为每张图新建连接。

### 关键设计

**版本继承**：加载器版本通过 `inheritsFrom` 指向原版。解析时沿链递归加载并合并（库列表拼接、参数拼接、其余字段子版本优先），最终得到可用于启动的完整描述。

**规则求值**：库与参数中的 `rules` 按 `os.name` / `os.arch` / `os.version` 与 features 求值，仅保留 Windows x64 且不含 demo / quick-play 特性的条目。

**下载批次分级**：

- 核心文件（主 jar、依赖库、natives、资源索引、日志配置）采用严格批次，任一失败即报错；
- 资源对象采用宽松批次，允许部分失败并写入 `InstallReport`，界面提示缺失数量，可重新安装补齐；
- 失败文件达到阈值时熔断，避免在网络不可用时长时间重试。

**网络策略**：同一文件准备「镜像地址 + 原始地址」两个候选，镜像 404 时自动回退官方；镜像站始终直连，其余主机在启用时走系统代理（环境变量优先，其次 Windows 注册表）。

**3D 皮肤渲染**：把皮肤纹理映射到 6 个立方体（头、身、双臂、双腿），每帧做 Yaw/Pitch 旋转、背面剔除与按面深度排序，写入 `egui::Mesh` 后提交给 GPU。

## 构建

### 环境要求

- Rust 1.82 或更高（开发与验证使用 1.94.0，MSVC 工具链）
- Windows 10/11 x64
- 首次构建需要编译 egui、resvg、reqwest 等依赖，耗时较长

### 命令

```bash
git clone https://github.com/qiuxi903/Hutaomu_Minecraft_Launcher.git
cd Hutaomu_Minecraft_Launcher

cargo build            # 调试构建
cargo build --release  # 发布构建
```

产物：`target/release/htmcl.exe`（约 13 MB，已 strip 并启用 thin LTO）。

### 可执行文件图标

仓库已包含 `assets/htmcl.ico`。修改 `assets/icon.svg` 后重新生成：

```bash
cargo run --example gen_icon
```

该示例会用 resvg 渲染 16/24/32/48/64/128/256 多个尺寸并组装 ICO。

## 运行与命令行

直接运行 `htmcl.exe` 即可。首次启动会在同目录创建配置文件与缓存目录。

```bash
htmcl.exe                            # 正常启动
htmcl.exe --selftest                 # 端到端自检（默认验证 1.20.1）
htmcl.exe --selftest 1.20.1          # 指定版本
htmcl.exe --selftest 1.20.1 fabric   # 额外安装 Fabric 后启动
```

环境变量：

| 变量 | 作用 |
| --- | --- |
| `HTMCL_SHOT=<目录>` | 依次切换 8 个界面场景截图后退出，用于界面回归自检 |

自检会真实执行：获取版本清单 → 搜索 Java → 下载安装 → 构建启动命令 → 启动并观察 90 秒 → Modrinth 搜索与版本查询。

## 配置文件

启动器数据全部位于 `htmcl.exe` 同目录的 `htmcl/` 文件夹；游戏数据默认在 `.minecraft/`：

```
htmcl.exe
htmcl/
├── htmcl.json      启动器配置
├── accounts.json   账号（含明文令牌）
├── themes/         自定义主题
├── logs/           启动器与游戏日志
└── cache/          authlib-injector、皮肤、图片、安装器等缓存
.minecraft/         默认游戏目录（可在设置中修改）
```

旧版本（配置直接放在 exe 同目录）首次启动时会自动迁移到 `htmcl/`，不会覆盖已有数据。

### htmcl.json

```jsonc
{
  "schema": 1,
  "game_dir": "D:/Games/.minecraft",
  "selected_version": "1.20.1",
  "selected_account": "35016cc9-5a73-3322-a7ca-b5eeb4861a74",

  "java": {
    "auto": true,
    "path": null,               // 指定 java.exe；null 表示按版本自动选择
    "memory_mb": 4096,
    "jvm_args": ""
  },

  "launch": {
    "fullscreen": false,
    "width": 1280,
    "height": 720,
    "extra_args": "",           // 额外游戏参数，例如 --server example.com
    "isolation": true,          // 版本隔离
    "show_game_log": true
  },

  "download": {
    "source": "official",       // "official" | "bmclapi"
    "threads": 8,
    "timeout_secs": 30,
    "auto_install_dependencies": true,
    "use_system_proxy": true
  },

  "ui": {
    "theme": "light",
    "accent": null,             // 形如 "#B0703F"；null 表示跟随主题
    "radius": 10,
    "font_scale": 1.0,
    "background": {
      "kind": "none",           // "none" | "image"
      "path": null,
      "opacity": 0.35,
      "dim": 0.55
    },
    "sidebar_collapsed": false,
    "window_size": [1120, 720],
    "animations": true
  },

  "news": {
    "enabled": true,
    "url": "",                  // 留空显示内置使用提示
    "refresh_minutes": 60,
    "last_fetch": null
  },

  "java_cache": [],
  "version_overrides": {
    "1.20.1": { "memory_mb": 8192, "java_path": null, "jvm_args": "", "game_dir": null }
  }
}
```

写入采用「临时文件 + 重命名」，避免中途崩溃损坏配置。

### accounts.json

```jsonc
{
  "selected": "<uuid>",
  "accounts": [
    {
      "kind": "offline",
      "id": "<uuid>",
      "name": "Steve",
      "skin_path": null,
      "skin_model": "classic",
      "created_at": 0
    },
    {
      "kind": "microsoft",
      "id": "<uuid>", "name": "...", "uuid": "<uuid>",
      "access_token": "...", "refresh_token": "...",
      "expires_at": 0, "xuid": "...",
      "skin_path": null, "skin_model": "slim", "skin_url": null
    },
    {
      "kind": "authlib",
      "id": "<uuid>", "name": "...", "uuid": "<uuid>",
      "access_token": "...", "client_token": "...",
      "server_url": "https://skin.hutaomu.cn/api/yggdrasil",
      "skin_path": null, "skin_model": "classic",
      "expires_at": 0, "skin_url": null
    }
  ]
}
```

> 令牌以明文保存。请勿把 `accounts.json` 提交到任何仓库，也不要分享整个启动器目录。

### 主题

在 `themes/` 下新建文件夹，放入 `theme.json`（设置页的「生成示例主题」可直接生成模板）：

```jsonc
{
  "id": "my-theme",
  "name": "我的主题",
  "dark": false,
  "accent": "#B0703F",
  "bg": "#F5F6F8",
  "surface": "#FFFFFF",
  "surface_alt": "#F1F2F5",
  "border": "#E7E9ED",
  "text": "#1D2025",
  "text_weak": "#6E7681",
  "radius": 10,
  "background": "background.png",  // 相对主题目录，支持 png / jpg / gif / webp / bmp
  "background_opacity": 0.35,
  "background_dim": 0.55
}
```

主题色与圆角会被 `htmcl.json` 中的用户覆盖值覆盖（如果设置过）。

## 数据来源与网络策略

| 来源 | 用途 |
| --- | --- |
| `piston-meta.mojang.com` / `piston-data.mojang.com` | 版本清单、版本描述、客户端 jar、日志配置 |
| `resources.download.minecraft.net` | 资源对象 |
| `libraries.minecraft.net` | 依赖库 |
| `bmclapi2.bangbang93.com` | 上述内容的国内镜像 |
| `meta.fabricmc.net` / `meta.quiltmc.org` | 加载器版本与 profile |
| `maven.minecraftforge.net` / `maven.neoforged.net` | 加载器版本与安装器 |
| `api.modrinth.com` | Mod / 光影 / 资源包 / 整合包 |
| `login.microsoftonline.com`、`xsts.auth.xboxlive.com`、`api.minecraftservices.com` | 微软登录 |
| `authlib-injector.yushi.moe` | authlib-injector 最新版本 |
| 皮肤站 Yggdrasil API | 皮肤站登录与皮肤 |
| `net-secondary.web.minecraft-services.net` | Minecraft 官方中文资讯 |
| `klpbbs.com` | 苦力怕论坛 RSS |
| `zh.minecraft.wiki` | 中文 Minecraft Wiki 最近更改 |

说明：

- 所有下载文件都带 SHA1 校验（源未提供校验值的除外）；
- 镜像站（`*.bangbang93.com`）始终直连，其余主机在启用时走系统代理；
- Modrinth 请求携带带项目名的 User-Agent；
- 启动器不包含任何游戏资源，全部在用户操作下按需下载。

## 已知限制与路线图

**已知限制**

- 仅面向 Windows x64；其他平台未经测试；
- Forge / NeoForge 安装器流程、微软登录、皮肤站登录尚未实机验证；
- 不支持 OptiFine 安装、CurseForge 内容、Mod 更新检查、存档 / 世界下载；
- 下载中断后不会复用 `.part` 文件，只按 SHA1 跳过已完成文件；
- 资源 CDN 不稳定时可能出现部分资源缺失，重新安装可补齐；
- 版本重命名不会自动修正指向它的子版本（`inheritsFrom`），重命名基础版本后需重新安装相关加载器版本；
- 资讯源依赖第三方站点可用性，接口变化时需要更新解析逻辑；
- 下载中心只保留进行中的任务，完成后不保留历史记录。

**路线图**

1. 实机验证 Forge / NeoForge 安装器，补齐失败场景的错误提示；
2. 版本管理补齐导出整合包、安装 Mod / 资源到指定版本；
3. OptiFine 安装；
4. CurseForge 内容源；
5. Mod 更新检查与批量管理；
6. 下载断点续传与下载历史。

## 参与开发

- 提交前保证 `cargo build` 与 `cargo build --release` 均通过且**无警告**；
- 代码风格遵循 rustfmt；`core` 不得依赖 `ui` / `views`；
- 提交说明使用 `类型: 内容`，类型为 `feat` / `fix` / `ui` / `docs` / `build` / `refactor`；
- 涉及下载、安装、启动的改动，请先跑 `htmcl.exe --selftest` 验证；
- 涉及界面的改动，可用 `HTMCL_SHOT` 生成截图自查；
- **不要提交** `logs/`、`AI协作规则.md`、`htmcl.json`、`accounts.json`、`themes/`、`.minecraft/`、`target/`；
- 新增第三方依赖需在 PR 中说明用途，并在「关于」页面补充许可信息。

## 参考项目

在交互设计与功能组织上参考了以下项目，**代码为完全重写，未复制其源码**：

- [SJMCL](https://github.com/UNIkeEN/SJMCL)（MIT）—— 界面布局与信息架构
- [PCL2](https://github.com/Hex-Dragon/PCL2)（自定义许可）—— 版本管理与启动流程
- [HMCL](https://github.com/HMCL-dev/HMCL)（GPL-3.0）—— 加载器安装、Java 搜索与下载源
- [Bakaxl](https://github.com/Bakaxl)（MIT）—— 主题系统与视觉细节
- [PCL2-CE](https://github.com/PCL-Community/PCL-CE)（GPL-3.0）—— 版本管理的信息架构与内置资讯源

## 许可证与声明

本项目以 [GNU Affero General Public License v3.0](LICENSE) 开源。分发修改后的版本时，必须同样以 AGPL-3.0 授权并提供完整源代码。

本启动器与 Mojang Studios、Microsoft 无任何关联，「Minecraft」为 Mojang Studios 的商标。启动器不包含任何游戏本体文件。

---

作者：邱息 / [qiuxi903](https://github.com/qiuxi903)
