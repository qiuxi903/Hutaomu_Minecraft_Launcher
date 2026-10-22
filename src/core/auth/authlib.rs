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

/// 皮肤站账号下的一个角色
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthlibProfile {
    /// 角色 UUID
    pub id: String,
    /// 角色名
    pub name: String,
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

/// 登录后的会话，尚未选定角色
#[derive(Debug, Clone)]
pub struct AuthlibSession {
    /// 规整后的 Yggdrasil API 根地址
    pub server: String,
    pub access_token: String,
    pub client_token: String,
    /// 该账号下的全部角色
    pub profiles: Vec<AuthlibProfile>,
}

/// 用账号密码换取访问令牌与角色列表
///
/// Yggdrasil 的 authenticate 会返回该账号下的所有角色（availableProfiles），
/// 多数皮肤站会顺带把第一个填进 selectedProfile。
/// 这里把角色列表原样带回，由界面让用户挑选。
pub fn authenticate(
    dl: &Downloader,
    server_url: &str,
    username: &str,
    password: &str,
) -> Result<AuthlibSession> {
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

    // 角色列表：availableProfiles 为准，缺失时退回 selectedProfile
    let profiles = extract_profiles(resp.available_profiles, resp.selected_profile);
    if profiles.is_empty() {
        return Err(anyhow!("该账号在皮肤站没有可用的角色"));
    }

    Ok(AuthlibSession {
        server,
        access_token,
        client_token,
        profiles,
    })
}

/// 从 Yggdrasil 响应里取出角色列表
///
/// availableProfiles 才是该账号下的**全部**角色，
/// selectedProfile 只是皮肤站顺手预选的一个。
fn extract_profiles(
    available: Option<Vec<YggdrasilProfile>>,
    selected: Option<YggdrasilProfile>,
) -> Vec<AuthlibProfile> {
    let mut profiles: Vec<AuthlibProfile> = Vec::new();
    for profile in available.unwrap_or_default() {
        profiles.push(AuthlibProfile {
            id: profile.id,
            name: profile.name,
        });
    }
    if profiles.is_empty() {
        if let Some(profile) = selected {
            profiles.push(AuthlibProfile {
                id: profile.id,
                name: profile.name,
            });
        }
    }
    profiles
}

/// 选定角色并完成登录
///
/// 走 Yggdrasil 的 refresh 接口并带上 selectedProfile，
/// 返回的 selectedProfile 即最终使用的角色。
pub fn select_profile(
    dl: &Downloader,
    server_url: &str,
    access_token: &str,
    client_token: &str,
    profile: &AuthlibProfile,
) -> Result<AuthlibAuth> {
    let server = normalize_api_root(server_url);
    let body = serde_json::json!({
        "accessToken": access_token,
        "clientToken": client_token,
        "requestUser": true,
        "selectedProfile": { "id": profile.id, "name": profile.name }
    });
    let resp = post_yggdrasil(dl, &format!("{server}/authserver/refresh"), &body)?;

    if let Some(err) = resp.error.clone() {
        return Err(anyhow!("{}", translate_error(&resp.error_message.unwrap_or(err))));
    }

    let access_token = resp
        .access_token
        .ok_or_else(|| anyhow!("皮肤站未返回访问令牌"))?;
    let client_token = resp.client_token.unwrap_or_else(|| client_token.to_string());

    // refresh 返回的 selectedProfile 才是真正生效的角色
    let chosen = resp
        .selected_profile
        .map(|p| AuthlibProfile {
            id: p.id,
            name: p.name,
        })
        .unwrap_or_else(|| profile.clone());

    let (skin_url, skin_model) = fetch_skin(dl, &server, &chosen.id, &access_token)
        .unwrap_or((None, "classic".into()));

    Ok(AuthlibAuth {
        access_token,
        client_token,
        uuid: chosen.id,
        name: chosen.name,
        server_url: server,
        skin_url,
        skin_model,
    })
}

/// 账号密码直接登录（只有一个角色时使用）
pub fn login(
    dl: &Downloader,
    server_url: &str,
    username: &str,
    password: &str,
) -> Result<AuthlibAuth> {
    let session = authenticate(dl, server_url, username, password)?;
    let profile = session
        .profiles
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("该账号在皮肤站没有可用的角色"))?;
    select_profile(
        dl,
        server_url,
        &session.access_token,
        &session.client_token,
        &profile,
    )
}

/// 刷新会话令牌
///
/// `keep_profile_id` 为该账号当前绑定的角色，刷新时保持该角色不变；
/// 传 None 时才退回取第一个。
pub fn refresh(
    dl: &Downloader,
    server_url: &str,
    access_token: &str,
    client_token: &str,
    keep_profile_id: Option<&str>,
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
    // 优先沿用原角色，避免刷新后莫名其妙换了一个角色
    let profile = resp
        .available_profiles
        .unwrap_or_default()
        .into_iter()
        .find(|p| keep_profile_id.map(|id| p.id == id).unwrap_or(false))
        .or_else(|| resp.selected_profile.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, name: &str) -> YggdrasilProfile {
        YggdrasilProfile {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn extracts_all_available_profiles() {
        // availableProfiles 里有多个角色时要全部保留，不能只取 selectedProfile
        let profiles = extract_profiles(
            Some(vec![profile("aaa", "角色一"), profile("bbb", "角色二")]),
            Some(profile("aaa", "角色一")),
        );
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].name, "角色一");
        assert_eq!(profiles[1].name, "角色二");
    }

    #[test]
    fn falls_back_to_selected_profile() {
        let profiles = extract_profiles(None, Some(profile("aaa", "唯一角色")));
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "aaa");
    }

    #[test]
    fn returns_empty_when_nothing_available() {
        assert!(extract_profiles(None, None).is_empty());
        assert!(extract_profiles(Some(vec![]), None).is_empty());
    }

    #[test]
    fn normalizes_api_root() {
        assert_eq!(
            normalize_api_root("https://example.com"),
            "https://example.com/api/yggdrasil"
        );
        assert_eq!(
            normalize_api_root("https://example.com/api/yggdrasil/"),
            "https://example.com/api/yggdrasil"
        );
        assert_eq!(normalize_api_root(""), DEFAULT_SERVER);
    }
}
