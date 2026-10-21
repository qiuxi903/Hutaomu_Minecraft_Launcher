use anyhow::{anyhow, Context, Result};
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
#[serde(default, rename_all = "camelCase")]
struct YggdrasilResponse {
    access_token: Option<String>,
    client_token: Option<String>,
    selected_profile: Option<YggdrasilProfile>,
    available_profiles: Option<Vec<YggdrasilProfile>>,
    error: Option<String>,
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
    let resp = post_yggdrasil(dl, &format!("{server}/authserver/authenticate"), &body)?;

    if let Some(err) = resp.error.clone() {
        let message = resp.error_message.clone().unwrap_or(err);
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
    let resp = post_yggdrasil(dl, &format!("{server}/authserver/refresh"), &body)?;

    if let Some(err) = resp.error.clone() {
        return Err(anyhow!("{}", resp.error_message.clone().unwrap_or(err)));
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

/// Yggdrasil 在 4xx 时也会返回 JSON 错误体，这里统一解析
fn post_yggdrasil(
    dl: &Downloader,
    url: &str,
    body: &serde_json::Value,
) -> Result<YggdrasilResponse> {
    let resp = dl
        .client_for_auth(url)
        .post(url)
        .json(body)
        .send()
        .with_context(|| format!("连接失败: {url}"))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();

    let parsed: YggdrasilResponse = serde_json::from_str(&text).unwrap_or_default();
    if parsed.error.is_some() {
        return Ok(parsed);
    }
    if !status.is_success() {
        return Err(anyhow!(
            "皮肤站返回 HTTP {}：{}",
            status.as_u16(),
            crate::core::download::truncate(&text, 160)
        ));
    }
    // 成功响应也必须能解析出 accessToken，否则视为接口不兼容
    if parsed.access_token.is_none() {
        return Err(anyhow!(
            "皮肤站返回的数据无法识别：{}",
            crate::core::download::truncate(&text, 160)
        ));
    }
    Ok(parsed)
}

fn translate_error(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("invalid credentials") {
        return "用户名或密码错误".to_string();
    }
    if lower.contains("is not found") || lower.contains("not found") {
        return "皮肤站上没有这个账号".to_string();
    }
    if lower.contains("token has expired") {
        return "登录已过期，请重新登录".to_string();
    }
    if lower.contains("forbidden operation for token") {
        return "登录状态已失效，请重新登录".to_string();
    }
    message.to_string()
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
