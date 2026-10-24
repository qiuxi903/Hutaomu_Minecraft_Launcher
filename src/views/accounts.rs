use std::path::PathBuf;

use egui::{Align, Layout, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::{App, CoreEvent, SkinSource};
use crate::core::auth::authlib::{self, AuthlibAuth, AuthlibProfile, AuthlibSession};
use crate::core::auth::microsoft::{self, DeviceCode, PollOutcome};
use crate::core::auth::{self, Account, AuthlibAccount, MicrosoftAccount, OfflineAccount, SkinModel};
use crate::icons;
use crate::ui::{self, widgets, ButtonOpts};

pub struct AccountsState {
    pub tab: usize,
    pub offline_name: String,
    pub new_skin_path: Option<PathBuf>,
    pub ms_client_id: String,
    pub device_code: Option<DeviceCode>,
    pub ms_status: String,
    pub ms_logging_in: bool,
    pub authlib_server: String,
    pub authlib_user: String,
    pub authlib_pass: String,
    pub authlib_status: String,
    pub authlib_logging_in: bool,
    pub just_logged_in: bool,
    /// 皮肤站账号下的角色列表，等待用户挑选
    pub authlib_profiles: Vec<AuthlibProfile>,
    /// 等待选择角色时暂存的会话（服务器、访问令牌、客户端令牌）
    pub authlib_session: Option<AuthlibSession>,
    /// 只有一个角色时自动接着完成登录
    pub pending_profile: Option<AuthlibProfile>,
}

impl Default for AccountsState {
    fn default() -> Self {
        Self {
            tab: 0,
            offline_name: String::new(),
            new_skin_path: None,
            ms_client_id: microsoft::DEFAULT_CLIENT_ID.to_string(),
            device_code: None,
            ms_status: String::new(),
            ms_logging_in: false,
            authlib_server: authlib::DEFAULT_SERVER.to_string(),
            authlib_user: String::new(),
            authlib_pass: String::new(),
            authlib_status: String::new(),
            authlib_logging_in: false,
            just_logged_in: false,
            authlib_profiles: Vec::new(),
            authlib_session: None,
            pending_profile: None,
        }
    }
}

impl AccountsState {
    pub fn on_device_code(&mut self, result: Result<DeviceCode, String>) {
        match result {
            Ok(code) => {
                self.ms_status = "请在浏览器中完成授权，授权后会自动登录".into();
                self.device_code = Some(code);
            }
            Err(err) => {
                self.ms_status = format!("获取验证码失败：{err}");
                self.ms_logging_in = false;
            }
        }
    }

    pub fn on_ms_poll(
        &mut self,
        result: Result<PollOutcome, String>,
        accounts: &mut crate::core::auth::AccountStore,
    ) {
        match result {
            Ok(PollOutcome::Done(auth)) => {
                let account = Account::Microsoft(MicrosoftAccount {
                    id: auth.uuid.clone(),
                    name: auth.name.clone(),
                    uuid: auth.uuid.clone(),
                    access_token: auth.access_token,
                    refresh_token: auth.refresh_token,
                    expires_at: auth.expires_at,
                    xuid: auth.xuid,
                    skin_path: None,
                    skin_model: if auth.skin_variant == "slim" {
                        SkinModel::Slim
                    } else {
                        SkinModel::Classic
                    },
                    skin_url: auth.skin_url,
                });
                accounts.upsert(account.clone());
                accounts.selected = Some(account.id().to_string());
                self.ms_status = "登录成功".into();
                self.ms_logging_in = false;
                self.device_code = None;
                self.just_logged_in = true;
            }
            Ok(PollOutcome::Pending) => {
                self.ms_status = "等待授权中 ...".into();
            }
            Ok(PollOutcome::SlowDown) => {
                self.ms_status = "请稍候，正在等待授权 ...".into();
            }
            Ok(PollOutcome::Declined) => {
                self.ms_status = "授权已被拒绝".into();
                self.ms_logging_in = false;
                self.device_code = None;
            }
            Ok(PollOutcome::Expired) => {
                self.ms_status = "验证码已过期，请重新获取".into();
                self.ms_logging_in = false;
                self.device_code = None;
            }
            Err(err) => {
                self.ms_status = err;
                self.ms_logging_in = false;
                self.device_code = None;
            }
        }
    }

    /// 皮肤站登录换取到会话，等待用户挑选角色
    pub fn on_authlib_session(
        &mut self,
        result: Result<AuthlibSession, String>,
    ) {
        self.authlib_logging_in = false;
        match result {
            Ok(session) => {
                let count = session.profiles.len();
                if count <= 1 {
                    // 只有一个角色时直接完成登录
                    if let Some(profile) = session.profiles.first().cloned() {
                        self.authlib_status = "登录成功，正在载入角色 ...".into();
                        self.authlib_session = Some(session);
                        // 交给外部触发选择
                        self.pending_profile = Some(profile);
                    } else {
                        self.authlib_status = "该账号在皮肤站没有可用的角色".into();
                    }
                } else {
                    self.authlib_status =
                        format!("登录成功，该账号下有 {count} 个角色，请选择一个");
                    self.authlib_profiles = session.profiles.clone();
                    self.authlib_session = Some(session);
                }
            }
            Err(err) => {
                self.authlib_status = err;
                self.authlib_profiles.clear();
                self.authlib_session = None;
            }
        }
    }

    pub fn on_authlib_login(
        &mut self,
        result: Result<AuthlibAuth, String>,
        accounts: &mut crate::core::auth::AccountStore,
    ) {
        self.authlib_logging_in = false;
        match result {
            Ok(auth) => {
                let account = Account::Authlib(AuthlibAccount {
                    id: auth.uuid.clone(),
                    name: auth.name.clone(),
                    uuid: auth.uuid,
                    access_token: auth.access_token,
                    client_token: auth.client_token,
                    server_url: auth.server_url,
                    skin_path: None,
                    skin_model: if auth.skin_model == "slim" {
                        SkinModel::Slim
                    } else {
                        SkinModel::Classic
                    },
                    expires_at: 0,
                    skin_url: auth.skin_url,
                });
                accounts.upsert(account.clone());
                accounts.selected = Some(account.id().to_string());
                self.authlib_status = "登录成功".into();
                self.authlib_pass.clear();
                // 登录完成，清理待选角色的状态
                self.authlib_profiles.clear();
                self.authlib_session = None;
                self.just_logged_in = true;
            }
            Err(err) => {
                self.authlib_status = err;
            }
        }
    }
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(
        ui,
        &p,
        "账号",
        "支持离线登录、微软正版登录，以及兼容 Yggdrasil 协议的皮肤站",
    );

    add_account_card(app, ui);
    ui.add_space(14.0);

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(300.0);
            account_list(app, ui);
        });
        ui.add_space(14.0);
        ui.vertical(|ui| {
            let width = ui.available_width();
            ui.set_width(width);
            account_detail(app, ui);
        });
    });
}

// ------------------------------------------------------------------ 添加账号

fn add_account_card(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        let tabs = ["离线登录", "微软正版", "皮肤站"];
        let mut tab = app.accounts_ui.tab;
        if ui::segmented(ui, &p, "acc_tab", &mut tab, &tabs).clicked() {
            app.accounts_ui.tab = tab;
            app.accounts_ui.device_code = None;
            app.accounts_ui.ms_logging_in = false;
        }
        ui.add_space(14.0);

        match app.accounts_ui.tab {
            0 => offline_form(app, ui),
            1 => microsoft_form(app, ui),
            _ => authlib_form(app, ui),
        }
    });
}

fn offline_form(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.horizontal(|ui| {
        let mut name = app.accounts_ui.offline_name.clone();
        let response = ui::text_field(ui, &p, &mut name, "游戏内用户名（3-16 位字母数字下划线）", 300.0, false);
        if response.changed() {
            app.accounts_ui.offline_name = name;
        }
        if ui::button(ui, &p, ButtonOpts::primary("添加离线账号").icon("plus")).clicked() {
            add_offline(app);
        }
    });
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("离线账号无需正版，仅在本机与离线服务器使用；皮肤可在右侧单独设置。")
            .font(ui::font(11.5))
            .color(p.text_faint),
    );
}

fn add_offline(app: &mut App) {
    let name = app.accounts_ui.offline_name.trim().to_string();
    if let Err(err) = crate::core::paths::validate_player_name(&name) {
        app.error(err);
        return;
    }
    let account = Account::Offline(OfflineAccount {
        id: auth::offline::offline_uuid(&name),
        name: name.clone(),
        skin_path: None,
        skin_model: SkinModel::Classic,
        created_at: chrono::Utc::now().timestamp(),
    });
    let id = account.id().to_string();
    app.accounts.upsert(account);
    app.accounts.selected = Some(id.clone());
    app.save_accounts();
    app.ensure_skin(&id);
    app.accounts_ui.offline_name.clear();
    app.success(format!("已添加离线账号 {name}"));
}

fn microsoft_form(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let device = app.accounts_ui.device_code.clone();

    if let Some(code) = device {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("验证码")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.label(
                egui::RichText::new(&code.user_code)
                    .font(ui::mono(22.0))
                    .color(p.accent)
                    .strong(),
            );
            if ui::button(ui, &p, ButtonOpts::secondary("复制")).clicked() {
                ui.ctx().copy_text(code.user_code.clone());
                app.info("验证码已复制");
            }
            if ui::button(ui, &p, ButtonOpts::primary("打开登录页面").icon("external-link")).clicked() {
                let _ = open::that(&code.verification_uri);
            }
        });
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!(
                "在浏览器打开 {} 并输入上面的验证码。",
                code.verification_uri
            ))
            .font(ui::font(12.0))
            .color(p.text_weak),
        );
    } else {
        ui.horizontal(|ui| {
            let enabled = !app.accounts_ui.ms_logging_in;
            let label = if app.accounts_ui.ms_logging_in {
                "正在等待授权"
            } else {
                "使用微软账号登录"
            };
            if ui::button(
                ui,
                &p,
                ButtonOpts::primary(label)
                    .icon("account-microsoft")
                    .enabled(enabled),
            )
            .clicked()
            {
                start_ms_login(app);
            }
            if app.accounts_ui.ms_logging_in {
                ui::spinner(ui, &p, 16.0);
            }
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("登录会在浏览器完成，启动器只保存登录令牌，不会接触你的密码。")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
    }

    if !app.accounts_ui.ms_status.is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(&app.accounts_ui.ms_status)
                .font(ui::font(12.0))
                .color(p.text_weak),
        );
    }

    ui.add_space(8.0);
    ui.collapsing("高级：自定义 Azure 客户端 ID", |ui| {
        ui.label(
            egui::RichText::new("默认客户端 ID 适用于大多数情况，一般无需修改。")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
        ui.add_space(4.0);
        let mut id = app.accounts_ui.ms_client_id.clone();
        if ui::text_field(ui, &p, &mut id, "Azure Application (client) ID", 360.0, false).changed() {
            app.accounts_ui.ms_client_id = id;
        }
    });
}

fn start_ms_login(app: &mut App) {
    let client_id = app.accounts_ui.ms_client_id.trim().to_string();
    let client_id = if client_id.is_empty() {
        microsoft::DEFAULT_CLIENT_ID.to_string()
    } else {
        client_id
    };
    app.accounts_ui.ms_logging_in = true;
    app.accounts_ui.ms_status = "正在获取验证码 ...".into();
    app.accounts_ui.device_code = None;
    let timeout = app.cfg.download.timeout_secs;

    app.spawn("微软账号登录", true, move |job| {
        let dl = match crate::core::download::Downloader::new(
            crate::core::download::DownloadSource::Official,
            timeout,
        ) {
            Ok(dl) => dl,
            Err(err) => {
                job.send(CoreEvent::MsPoll(Err(format!("初始化网络失败：{err}"))));
                job.done(true, "");
                return;
            }
        };
        let code = match microsoft::request_device_code(&dl, &client_id) {
            Ok(code) => code,
            Err(err) => {
                job.send(CoreEvent::MsPoll(Err(format!("获取验证码失败：{err}"))));
                job.done(true, "");
                return;
            }
        };
        job.send(CoreEvent::DeviceCode(Ok(code.clone())));

        let mut interval = code.interval.max(2) as u64;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(code.expires_in.max(60) as u64);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_secs(interval));
            match microsoft::poll_device_code(&dl, &client_id, &code.device_code) {
                Ok(PollOutcome::Pending) => continue,
                Ok(PollOutcome::SlowDown) => {
                    interval += 2;
                    continue;
                }
                Ok(other) => {
                    job.send(CoreEvent::MsPoll(Ok(other)));
                    job.done(true, "");
                    return;
                }
                Err(err) => {
                    job.send(CoreEvent::MsPoll(Err(format!("{err}"))));
                    job.done(true, "");
                    return;
                }
            }
        }
        job.send(CoreEvent::MsPoll(Ok(PollOutcome::Expired)));
        job.done(true, "");
    });
}

fn authlib_form(app: &mut App, ui: &mut Ui) {
    let p = app.palette;

    // 账号下只有一个角色时直接接着完成登录
    if let Some(profile) = app.accounts_ui.pending_profile.take() {
        start_authlib_select(app, profile);
    }

    // ---------- 皮肤站地址 ----------
    ui.horizontal(|ui| {
        let mut server = app.accounts_ui.authlib_server.clone();
        if ui::text_field(ui, &p, &mut server, "皮肤站地址", 330.0, false).changed() {
            app.accounts_ui.authlib_server = server;
        }
        if ui::button(ui, &p, ButtonOpts::secondary("打开皮肤站").icon("external-link")).clicked() {
            let root = app
                .accounts_ui
                .authlib_server
                .trim()
                .trim_end_matches('/')
                .trim_end_matches("/api/yggdrasil")
                .to_string();
            let _ = open::that(if root.is_empty() {
                "https://skin.hutaomu.cn".to_string()
            } else {
                root
            });
        }
    });
    ui.add_space(10.0);

    // ---------- 账号与密码：上下分行，避免两个输入框挤在一起 ----------
    let mut user = app.accounts_ui.authlib_user.clone();
    if ui::text_field(ui, &p, &mut user, "邮箱 / 用户名", 320.0, false).changed() {
        app.accounts_ui.authlib_user = user;
    }
    ui.add_space(8.0);
    let mut pass = app.accounts_ui.authlib_pass.clone();
    if ui::text_field(ui, &p, &mut pass, "密码", 320.0, true).changed() {
        app.accounts_ui.authlib_pass = pass;
    }
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        let enabled = !app.accounts_ui.authlib_logging_in;
        if ui::button(ui, &p, ButtonOpts::primary("登录").icon("account-skin").enabled(enabled))
            .clicked()
        {
            start_authlib_login(app);
        }
        if app.accounts_ui.authlib_logging_in {
            ui::spinner(ui, &p, 16.0);
            ui.label(
                egui::RichText::new("正在登录 ...")
                    .font(ui::font(12.0))
                    .color(p.text_faint),
            );
        }
    });
    ui.add_space(10.0);

    // ---------- 选择角色 ----------
    let profiles = app.accounts_ui.authlib_profiles.clone();
    if !profiles.is_empty() {
        ui::card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("选择角色")
                        .font(ui::font(14.0))
                        .color(p.text)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("该账号下有 {} 个角色", profiles.len()))
                        .font(ui::font(11.5))
                        .color(p.text_faint),
                );
            });
            ui.add_space(8.0);
            for profile in &profiles {
                let response = ui::card_flat(ui, &p, |ui| {
                    ui.horizontal(|ui| {
                        icons::paint(
                            ui.ctx(),
                            ui.painter(),
                            "user",
                            Rect::from_center_size(
                                Pos2::new(ui.cursor().left() + 10.0, ui.cursor().center().y),
                                Vec2::splat(18.0),
                            ),
                            p.text_weak,
                        );
                        ui.add_space(28.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(&profile.name)
                                    .font(ui::font(13.0))
                                    .color(p.text)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(profile.id.replace('-', ""))
                                    .font(ui::font(11.0))
                                    .color(p.text_faint),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let enabled = !app.accounts_ui.authlib_logging_in;
                            if ui::button(
                                ui,
                                &p,
                                ButtonOpts::secondary("使用这个角色").enabled(enabled),
                            )
                            .clicked()
                            {
                                start_authlib_select(app, profile.clone());
                            }
                        });
                    });
                });
                let _ = response;
                ui.add_space(6.0);
            }
        });
        ui.add_space(10.0);
    }

    ui.label(
        egui::RichText::new("默认使用 skin.hutaomu.cn，兼容 Blessing Skin 等皮肤站；登录后启动时会自动挂载 authlib-injector。")
            .font(ui::font(11.5))
            .color(p.text_faint),
    );
    if !app.accounts_ui.authlib_status.is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(&app.accounts_ui.authlib_status)
                .font(ui::font(12.0))
                .color(p.text_weak),
        );
    }
}

fn start_authlib_login(app: &mut App) {
    let server = app.accounts_ui.authlib_server.trim().to_string();
    let user = app.accounts_ui.authlib_user.trim().to_string();
    let pass = app.accounts_ui.authlib_pass.clone();
    if user.is_empty() || pass.is_empty() {
        app.error("请输入皮肤站账号和密码");
        return;
    }
    app.accounts_ui.authlib_logging_in = true;
    app.accounts_ui.authlib_status = "正在登录 ...".into();
    app.accounts_ui.authlib_profiles.clear();
    app.accounts_ui.authlib_session = None;
    let timeout = app.cfg.download.timeout_secs;

    app.spawn("皮肤站登录", true, move |job| {
        let result = (|| -> anyhow::Result<AuthlibSession> {
            let dl = crate::core::download::Downloader::new(
                crate::core::download::DownloadSource::Official,
                timeout,
            )?;
            authlib::authenticate(&dl, &server, &user, &pass)
        })();
        job.send(CoreEvent::AuthlibSession(result.map_err(|e| format!("{e}"))));
        job.done(true, "");
    });
}

/// 选定角色并完成登录
fn start_authlib_select(app: &mut App, profile: AuthlibProfile) {
    let Some(session) = app.accounts_ui.authlib_session.clone() else {
        return;
    };
    app.accounts_ui.authlib_logging_in = true;
    app.accounts_ui.authlib_status = format!("正在使用角色 {} ...", profile.name);
    let timeout = app.cfg.download.timeout_secs;

    app.spawn("皮肤站选择角色", true, move |job| {
        let result = (|| -> anyhow::Result<AuthlibAuth> {
            let dl = crate::core::download::Downloader::new(
                crate::core::download::DownloadSource::Official,
                timeout,
            )?;
            authlib::select_profile(
                &dl,
                &session.server,
                &session.access_token,
                &session.client_token,
                &profile,
            )
        })();
        job.send(CoreEvent::AuthlibLogin(
            result.map_err(|e| format!("{e}")),
        ));
        job.done(true, "");
    });
}

// ------------------------------------------------------------------ 账号列表

fn account_list(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("账号列表")
                    .font(ui::font(14.5))
                    .color(p.text)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} 个", app.accounts.accounts.len()))
                        .font(ui::font(11.5))
                        .color(p.text_faint),
                );
            });
        });
        ui.add_space(10.0);

        if app.accounts.accounts.is_empty() {
            ui::empty_state(ui, &p, "user", "还没有账号", "在上方添加一个账号");
            return;
        }

        let accounts = app.accounts.accounts.clone();
        let mut select: Option<String> = None;
        let mut remove: Option<(String, String)> = None;

        for account in &accounts {
            let id = account.id().to_string();
            let selected = app.accounts.selected.as_deref() == Some(id.as_str());
            if let Some(skin) = app.skin_of(&id) {
                let _ = skin;
            } else {
                app.ensure_skin(&id);
            }
            let texture = app.skin_of(&id).map(|s| s.head.clone());

            let response = ui.scope(|ui| {
                let frame = egui::Frame::NONE
                    .fill(if selected { p.accent_weak } else { p.surface })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if selected { p.accent } else { p.border },
                    ))
                    .corner_radius(p.corner_sm())
                    .inner_margin(egui::Margin::symmetric(10, 8));
                frame
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(texture) = &texture {
                                let _ = ui::avatar(ui, &p, texture, 32.0);
                            } else {
                                let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                                ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
                                icons::paint(
                                    ui.ctx(),
                                    ui.painter(),
                                    "user",
                                    rect.shrink(8.0),
                                    p.text_faint,
                                );
                            }
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(account.name())
                                        .font(ui::font(13.5))
                                        .color(p.text)
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(account.kind_label())
                                        .font(ui::font(11.0))
                                        .color(p.text_faint),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui::icon_button(ui, &p, "trash", "删除账号").clicked() {
                                    remove = Some((
                                        account.id().to_string(),
                                        account.kind_label().to_string(),
                                    ));
                                }
                                if selected {
                                    icons::paint(
                                        ui.ctx(),
                                        ui.painter(),
                                        "check-circle",
                                        Rect::from_center_size(
                                            Pos2::new(ui.cursor().right() - 8.0, ui.cursor().center().y),
                                            Vec2::splat(16.0),
                                        ),
                                        p.accent,
                                    );
                                }
                            });
                        });
                    })
                    .response
            })
            .inner;
            let clickable = response.interact(Sense::click());
            if clickable.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if clickable.clicked() {
                select = Some(id.clone());
            }
            ui.add_space(6.0);
        }

        if let Some(id) = select {
            app.accounts.selected = Some(id.clone());
            app.save_accounts();
            app.ensure_skin(&id);
        }
        if let Some((id, kind)) = remove {
            app.accounts.remove(&id, &kind);
            app.save_accounts();
            app.skin_cache.remove(&id);
            app.info("账号已删除");
        }
    });
}

// ------------------------------------------------------------------ 账号详情

fn account_detail(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let Some(account) = app.selected_account().cloned() else {
        ui::card(ui, &p, |ui| {
            ui::empty_state(ui, &p, "skin-model", "选择一个账号", "右侧会显示 3D 皮肤与账号设置");
        });
        return;
    };
    let id = account.id().to_string();
    app.ensure_skin(&id);
    let skin = app.skin_of(&id);
    let model = skin.map(|s| s.model).unwrap_or(account.skin_model());
    let texture = skin.map(|s| s.texture.clone());
    let has_overlay = skin.map(|s| s.has_overlay).unwrap_or(true);
    let source = skin.map(|s| s.source).unwrap_or(SkinSource::Generated);

    ui::card(ui, &p, |ui| {
        ui.label(
            egui::RichText::new("账号详情")
                .font(ui::font(14.5))
                .color(p.text)
                .strong(),
        );
        ui.add_space(12.0);

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(260.0);
                ui::skin3d::skin_preview(
                    ui,
                    &p,
                    texture.as_ref(),
                    model,
                    has_overlay,
                    &mut app.preview_yaw,
                    300.0,
                    app.cfg.ui.animations,
                );
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(account.name())
                        .font(ui::font(19.0))
                        .color(p.text)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    widgets::badge(ui, &p, account.kind_label(), p.accent);
                    widgets::badge(ui, &p, model.short(), p.text_weak);
                    if account.is_expired() {
                        widgets::badge(ui, &p, "登录已过期", p.warning);
                    }
                });
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("UUID  {}", account.uuid()))
                        .font(ui::mono(11.0))
                        .color(p.text_faint),
                );
                if let Some(url) = account.authlib_url() {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(url)
                            .font(ui::font(11.0))
                            .color(p.text_faint),
                    );
                }
                ui.add_space(16.0);

                // 皮肤模型
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("皮肤模型")
                            .font(ui::font(12.5))
                            .color(p.text_weak),
                    );
                    let mut value = if model == SkinModel::Slim { 1 } else { 0 };
                    if ui::segmented(ui, &p, "skin_model", &mut value, &["经典", "纤细"]).clicked() {
                        let new_model = if value == 1 {
                            SkinModel::Slim
                        } else {
                            SkinModel::Classic
                        };
                        if let Some(acc) = app.accounts.get_mut(&id) {
                            acc.set_skin_model(new_model);
                        }
                        if let Some(skin) = app.skin_cache.get_mut(&id) {
                            skin.model = new_model;
                        }
                        app.save_accounts();
                    }
                });
                ui.add_space(10.0);

                // 皮肤来源
                ui.label(
                    egui::RichText::new(match (account.is_offline(), source) {
                        (true, SkinSource::Local) => "皮肤来源：本地图片",
                        (true, _) => "皮肤来源：按用户名自动生成",
                        (false, SkinSource::Local) | (false, SkinSource::Remote) => {
                            "皮肤来源：账号皮肤（已缓存）"
                        }
                        _ => "皮肤来源：账号皮肤",
                    })
                    .font(ui::font(12.0))
                    .color(p.text_weak),
                );
                ui.add_space(10.0);

                ui.horizontal_wrapped(|ui| {
                    if ui::button(ui, &p, ButtonOpts::secondary("选择皮肤文件").icon("image")).clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("皮肤图片", &["png"])
                            .set_title("选择 64x64 皮肤")
                            .pick_file()
                        {
                            match crate::core::skin::SkinImage::load(&path) {
                                Ok(image) => {
                                    let detected = if image.detect_slim() {
                                        SkinModel::Slim
                                    } else {
                                        SkinModel::Classic
                                    };
                                    if let Some(acc) = app.accounts.get_mut(&id) {
                                        acc.set_skin_path(Some(path.clone()));
                                        acc.set_skin_model(detected);
                                    }
                                    app.save_accounts();
                                    app.upload_skin(&id, &image, detected, SkinSource::Local);
                                    app.success("皮肤已更新");
                                }
                                Err(err) => app.error(format!("{err}")),
                            }
                        }
                    }
                    if ui::button(ui, &p, ButtonOpts::secondary("使用默认皮肤").icon("refresh")).clicked()
                    {
                        if let Some(acc) = app.accounts.get_mut(&id) {
                            acc.set_skin_path(None);
                        }
                        app.save_accounts();
                        app.skin_cache.remove(&id);
                        app.ensure_skin(&id);
                    }
                    if let Some(url) = account.authlib_url() {
                        if ui::button(ui, &p, ButtonOpts::secondary("打开皮肤站").icon("external-link")).clicked()
                        {
                            let root = url.trim_end_matches("/api/yggdrasil").to_string();
                            let _ = open::that(root);
                        }
                    }
                    if !account.is_offline() {
                        if ui::button(ui, &p, ButtonOpts::secondary("刷新皮肤").icon("download")).clicked()
                        {
                            app.skin_cache.remove(&id);
                            app.skin_pending.remove(&id);
                            app.ensure_skin(&id);
                        }
                    }
                });
            });
        });
    });
}
