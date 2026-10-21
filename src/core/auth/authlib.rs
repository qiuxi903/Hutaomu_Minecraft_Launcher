use anyhow::{anyhow, Result};
use base64::Engine;
use serde::Deserialize;

use crate::core::download::Downloader;

/// 默认皮肤站（Blessing Skin 的 Yggdrasil API 根地址）
pub const DEFAULT_SERVER: &str = "https://skin.hutaomu.cn/api/yggdrasil";

/// 把用户输入的地址规整为 Yggdrasil API 根地址
pub fn normalize_api_root(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return DEFAULT_SERVER.to_string();
    }
    if trimmed.ends_with("/api/yggdrasil") || trimmed.ends_with("/yggdrasil") {
        return trimmed.to_string();
    }
    format!("{trimmed}/api/yggdrasil")
}

#[derive(Debug, Clone, Deserialize)]
struct YggdrasilProfile {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct YggdrasilResponse {
    access_token: Option<String>,
    client_token: Option<String>,
    selected_profile: Option<YggdrasilProfile>,
    available_profiles: Option<Vec<YggdrasilProfile>>,
    error: Option<String>,
    #[serde(rename = "errorMessage")]
    error_message: Option<String>,
}

impl Default for YggdrasilResponse {
    fn default() -> Self {
        Self {
            access_token: None,
            client_token: None,
            selected_profile: None,
            available_profiles: None,
            error: None,
            error_message: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthlibAuth {
    pub access_token: String,
    pub client_token: String,
    pub uuid: String,
    pub name: String,
    pub server_url: String,
    pub skin_url: Option<String>,
    pub skin_model: String,
}

pub fn login(
    dl: &Downloader,
    server_url: &str,
    username: &str,
    password: &str,
) -> Result<AuthlibAuth> {
    let server = normalize_api_root(server_url);
    let client_token = uuid::Uuid::new_v4().to_string();
    let body = serde_json::json!({
        "username": username,
        "password": password,
        "clientToken": client_token,
        "agent": { "name": "Minecraft", "version": 1 },
        "requestUser": true
    });
    let resp: YggdrasilResponse = dl
        .post_json(&format!("{server}/authserver/authenticate"), &body)
        .map_err(|e| anyhow!("登录失败: {e}"))?;

    if let Some(err) = resp.error {
        let message = resp.error_message.unwrap_or(err);
        return Err(anyhow!("{}", translate_error(&message)));
    }

    let access_token = resp
        .access_token
        .ok_or_else(|| anyhow!("皮肤站未返回访问令牌"))?;
    let client_token = resp.client_token.unwrap_or(client_token);
    let profile = resp
        .selected_profile
        .or_else(|| resp.available_profiles.and_then(|mut list| list.drain(..).next()))
        .ok_or_else(|| anyhow!("该账号在皮肤站没有可用的角色"))?;

    let (skin_url, skin_model) = fetch_skin(dl, &server, &profile.id, &access_token)
        .unwrap_or((None, "classic".into()));

    Ok(AuthlibAuth {
        access_token,
        client_token,
        uuid: profile.id,
        name: profile.name,
        server_url: server,
        skin_url,
        skin_model,
    })
}

pub fn refresh(
    dl: &Downloader,
    server_url: &str,
    access_token: &str,
    client_token: &str,
) -> Result<AuthlibAuth> {
    let server = normalize_api_root(server_url);
    let body = serde_json::json!({
        "accessToken": access_token,
        "clientToken": client_token,
        "requestUser": true
    });
    let resp: YggdrasilResponse = dl
        .post_json(&format!("{server}/authserver/refresh"), &body)
        .map_err(|e| anyhow!("刷新登录状态失败: {e}"))?;

    if let Some(err) = resp.error {
        return Err(anyhow!("{}", resp.error_message.unwrap_or(err)));
    }

    let access_token = resp
        .access_token
        .ok_or_else(|| anyhow!("皮肤站未返回访问令牌"))?;
    let profile = resp
        .selected_profile
        .or_else(|| resp.available_profiles.and_then(|mut list| list.drain(..).next()))
        .ok_or_else(|| anyhow!("该账号在皮肤站没有可用的角色"))?;

    let (skin_url, skin_model) = fetch_skin(dl, &server, &profile.id, &access_token)
        .unwrap_or((None, "classic".into()));

    Ok(AuthlibAuth {
        access_token,
        client_token: resp.client_token.unwrap_or_else(|| client_token.to_string()),
        uuid: profile.id,
        name: profile.name,
        server_url: server,
        skin_url,
        skin_model,
    })
}

pub fn validate(
    dl: &Downloader,
    server_url: &str,
    access_token: &str,
    client_token: &str,
) -> bool {
    let server = normalize_api_root(server_url);
    let body = serde_json::json!({
        "accessToken": access_token,
        "clientToken": client_token
    });
    dl.post_json::<_, serde_json::Value>(&format!("{server}/authserver/validate"), &body)
        .is_ok()
}

fn translate_error(message: &str) -> String {
    match message {
        "Invalid credentials. Invalid username or password." => {
            "用户名或密码错误".to_string()
        }
        "Invalid credentials." => "账号或密码错误".to_string(),
        "Forbidden operation for token." => "登录状态已失效，请重新登录".to_string(),
        "Token has expired." => "登录已过期，请重新登录".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct ProfileResponse {
    #[serde(default)]
    properties: Vec<ProfileProperty>,
}

#[derive(Debug, Deserialize)]
struct ProfileProperty {
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: String,
}

#[derive(Debug, Deserialize)]
struct TexturesPayload {
    #[serde(default)]
    textures: Textures,
}

#[derive(Debug, Default, Deserialize)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<SkinTexture>,
}

#[derive(Debug, Deserialize)]
struct SkinTexture {
    #[serde(default)]
    url: String,
    #[serde(default)]
    metadata: SkinMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct SkinMetadata {
    #[serde(default)]
    model: String,
}

/// 获取皮肤地址，返回 (url, model)
pub fn fetch_skin(
    dl: &Downloader,
    server_url: &str,
    uuid: &str,
    access_token: &str,
) -> Result<(Option<String>, String)> {
    let server = normalize_api_root(server_url);
    let uuid_plain = uuid.replace('-', "");

    // 1. session server（皮肤站通用）
    let url = format!("{server}/sessionserver/session/minecraft/profile/{uuid_plain}");
    if let Ok(resp) = dl.get_json_direct::<ProfileResponse>(&url) {
        if let Some(value) = resp.properties.iter().find(|p| p.name == "textures") {
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&value.value) {
                if let Ok(payload) = serde_json::from_slice::<TexturesPayload>(&decoded) {
                    if let Some(skin) = payload.textures.skin {
                        if !skin.url.is_empty() {
                            let model = if skin.metadata.model == "slim" {
                                "slim"
                            } else {
                                "classic"
                            };
                            return Ok((Some(skin.url), model.to_string()));
                        }
                    }
                }
            }
        }
    }

    // 2. 部分皮肤站支持 /minecraft/profile
    let bearer = format!("Bearer {access_token}");
    let url = format!("{server}/minecraft/profile");
    if let Ok(resp) = dl.get_json_with_headers::<McProfileSkin>(&url, &[("Authorization", &bearer)]) {
        if let Some(skin) = resp.skins.first() {
            return Ok((Some(skin.url.clone()), skin.variant.clone()));
        }
    }

    Ok((None, "classic".into()))
}

#[derive(Debug, Deserialize)]
struct McProfileSkin {
    #[serde(default)]
    skins: Vec<SkinEntry>,
}

#[derive(Debug, Deserialize)]
struct SkinEntry {
    #[serde(default)]
    url: String,
    #[serde(default)]
    variant: String,
}
