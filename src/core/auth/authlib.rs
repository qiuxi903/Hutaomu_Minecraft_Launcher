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

/// 从 JWT 里取过期时间（秒级时间戳）
///
/// 皮肤站签发的 accessToken 是 JWT，过期时间在 `exp` 里。
/// 不解析的话启动器不知道令牌什么时候失效。
pub fn jwt_expires_at(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    let exp = json.get("exp")?;
    // exp 可能是整数，也可能是带小数的浮点时间戳
    exp.as_i64()
        .or_else(|| exp.as_f64().map(|f| f as i64))
}

/// 把角色 id 统一成带连字符的 36 位形式
///
/// 皮肤站的角色 id 可能在不同接口里格式不一致（有连字符的与没有的），
/// 统一之后再发给服务端，避免服务端认不出角色。
pub fn normalize_profile_id(id: &str) -> String {
    let hex: String = id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return id.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// 从请求体里取出角色 id 的两种可能写法
///
/// 优先用原样，服务端不认再试另一种。
fn profile_id_candidates(id: &str) -> Vec<String> {
    let normalized = normalize_profile_id(id);
    let plain: String = normalized.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut list = vec![id.to_string()];
    if !normalized.is_empty() && !list.contains(&normalized) {
        list.push(normalized);
    }
    if plain.len() == 32 && !list.contains(&plain) {
        list.push(plain);
    }
    list
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
    /// 访问令牌的过期时间（秒级时间戳），0 表示未知
    pub expires_at: i64,
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
    /// 访问令牌的过期时间（秒级时间戳），0 表示未知
    pub expires_at: i64,
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
    let expires_at = jwt_expires_at(&access_token).unwrap_or(0);

    // 角色列表：availableProfiles 为准，缺失时退回 selectedProfile
    let profiles = extract_profiles(resp.available_profiles, resp.selected_profile);
    if profiles.is_empty() {
        return Err(anyhow!("该账号在皮肤站没有可用的角色"));
    }

    log::info!(
        "皮肤站登录成功：{} 个角色，令牌有效期到 {}",
        profiles.len(),
        if expires_at > 0 {
            chrono::DateTime::from_timestamp(expires_at, 0)
                .map(|t| t.with_timezone(&chrono::Local).to_string())
                .unwrap_or_else(|| "未知".into())
        } else {
            "未知".into()
        }
    );

    Ok(AuthlibSession {
        server,
        access_token,
        client_token,
        profiles,
        expires_at,
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
    let url = format!("{server}/authserver/refresh");

    // 角色 id 的格式在不同接口里可能不一致，服务端不认就换一种写法再试
    let mut last_err: Option<String> = None;
    for id in profile_id_candidates(&profile.id) {
        let body = serde_json::json!({
            "accessToken": access_token,
            "clientToken": client_token,
            "requestUser": true,
            "selectedProfile": { "id": id, "name": profile.name }
        });
        let resp = post_yggdrasil(dl, &url, &body)?;
        if let Some(err) = resp.error.clone() {
            let message = resp.error_message.clone().unwrap_or(err);
            let text = translate_error(&message);
            last_err = Some(text);
            continue;
        }
        let token = resp
            .access_token
            .ok_or_else(|| anyhow!("皮肤站未返回访问令牌"))?;
        let client = resp.client_token.unwrap_or_else(|| client_token.to_string());
        let expires_at = jwt_expires_at(&token).unwrap_or(0);

        // refresh 返回的 selectedProfile 才是真正生效的角色
        let chosen = resp
            .selected_profile
            .map(|p| AuthlibProfile {
                id: p.id,
                name: p.name,
            })
            .unwrap_or_else(|| profile.clone());

        let (skin_url, skin_model) = fetch_skin(dl, &server, &chosen.id, &token)
            .unwrap_or((None, "classic".into()));

        log::info!("皮肤站登录成功：角色 {}（{}）", chosen.name, chosen.id);
        return Ok(AuthlibAuth {
            access_token: token,
            client_token: client,
            uuid: chosen.id,
            name: chosen.name,
            server_url: server,
            skin_url,
            skin_model,
            expires_at,
        });
    }

    let message = last_err.unwrap_or_else(|| "皮肤站没有接受这个角色".to_string());
    log::warn!("皮肤站登录失败（角色 {}）：{message}", profile.name);
    Err(anyhow!("{message}"))
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
        let message = resp.error_message.clone().unwrap_or(err);
        return Err(anyhow!("{}", translate_error(&message)));
    }

    let token = resp
        .access_token
        .ok_or_else(|| anyhow!("皮肤站未返回访问令牌"))?;
    let expires_at = jwt_expires_at(&token).unwrap_or(0);
    // 优先沿用原角色，避免刷新后莫名其妙换了一个角色
    let profile = resp
        .available_profiles
        .unwrap_or_default()
        .into_iter()
        .find(|p| keep_profile_id.map(|id| p.id == id || normalize_profile_id(id) == normalize_profile_id(&p.id)).unwrap_or(false))
        .or_else(|| resp.selected_profile.clone())
        .ok_or_else(|| anyhow!("该账号在皮肤站没有可用的角色"))?;

    let (skin_url, skin_model) = fetch_skin(dl, &server, &profile.id, &token)
        .unwrap_or((None, "classic".into()));

    log::info!("皮肤站令牌已刷新：角色 {}", profile.name);
    Ok(AuthlibAuth {
        access_token: token,
        client_token: resp.client_token.unwrap_or_else(|| client_token.to_string()),
        uuid: profile.id,
        name: profile.name,
        server_url: server,
        skin_url,
        skin_model,
        expires_at,
    })
}

/// 从 validate 的响应里判断令牌是否已经失效
///
/// 皮肤站返回 403 且提到 AccessToken 时才算失效，
/// 其它错误（比如服务不可用）不能当成失效，否则连不上皮肤站也会拦着用户。
pub fn token_invalid(status: u16, body: &str) -> bool {
    if status != 403 {
        return false;
    }
    let lower = body.to_lowercase();
    lower.contains("accesstoken") || lower.contains("access token")
}

/// 检查访问令牌是否还有效
///
/// 返回 `Ok(false)` 表示令牌已明确失效，需要重新登录；
/// 网络不通返回 `Err`，不要把这种情况当成失效。
pub fn validate(
    dl: &Downloader,
    server_url: &str,
    access_token: &str,
    client_token: &str,
) -> Result<bool> {
    let server = normalize_api_root(server_url);
    let url = format!("{server}/authserver/validate");
    let body = serde_json::json!({
        "accessToken": access_token,
        "clientToken": client_token
    });
    let resp = dl
        .client_for_auth(&url)
        .post(&url)
        .json(&body)
        .send()
        .with_context(|| format!("连接皮肤站失败: {url}"))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if token_invalid(status.as_u16(), &text) {
        log::warn!("皮肤站令牌已失效：{}", crate::core::download::truncate(&text, 120));
        return Ok(false);
    }
    Ok(true)
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
        log::warn!(
            "皮肤站返回错误 {}：{}",
            parsed.error.clone().unwrap_or_default(),
            parsed.error_message.clone().unwrap_or_default()
        );
        return Ok(parsed);
    }
    if !status.is_success() {
        log::warn!(
            "皮肤站返回 HTTP {}：{}",
            status.as_u16(),
            crate::core::download::truncate(&text, 200)
        );
        return Err(anyhow!(
            "皮肤站返回 HTTP {}：{}",
            status.as_u16(),
            crate::core::download::truncate(&text, 160)
        ));
    }
    // 成功响应也必须能解析出 accessToken，否则视为接口不兼容
    if parsed.access_token.is_none() {
        log::warn!(
            "皮肤站返回的数据无法识别：{}",
            crate::core::download::truncate(&text, 200)
        );
        return Err(anyhow!(
            "皮肤站返回的数据无法识别：{}",
            crate::core::download::truncate(&text, 160)
        ));
    }
    Ok(parsed)
}

fn translate_error(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("invalid credentials")
        || lower.contains("invalid username")
        || lower.contains("invalid password")
        || lower.contains("wrong password")
    {
        return "用户名或密码错误".to_string();
    }
    if lower.contains("is not found") || lower.contains("not found") {
        return "皮肤站上没有这个账号".to_string();
    }
    if lower.contains("token has expired") {
        return "登录已过期，请重新登录".to_string();
    }
    if lower.contains("invalid accesstoken") || lower.contains("invalid access token") {
        return "登录已失效，请重新登录".to_string();
    }
    if lower.contains("forbidden operation for token") {
        return "登录状态已失效，请重新登录".to_string();
    }
    if lower.contains("too frequent requests") {
        return "操作太频繁，请稍等片刻再试".to_string();
    }
    if lower.contains("already bound") || lower.contains("already logged in") {
        return "该账号已在别处登录，请先在皮肤站退出".to_string();
    }
    if lower.contains("two factor") || lower.contains("2fa") || lower.contains("require captcha") {
        return "该账号开启了两步验证，请先在皮肤站关闭或完成验证".to_string();
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
    /// 皮肤站的令牌是 JWT，过期时间要从 exp 里解析出来
    #[test]
    fn parses_jwt_expiry() {
        let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI4ZWI1Y2NhZjExYzg1YWIzOGE1MDY1YTVkYTQxNDVmYSIsInlnZ3QiOiJkZmNkMWNjODQxZDE0N2NkOTk5YzA5YzVjMGYyNzY0NSIsImlzcyI6IllnZ2RyYXNpbC1BdXRoIiwiZXhwIjoxNzkyOTg4MTcxLjI2MDI3MywiaWF0IjoxNzkyNzI4OTcxLjI2MDI3M30.wqtvVov9oBT041XtGt3pQMMJvXANoCgiM9WB-uS2ZPE";
        assert_eq!(jwt_expires_at(token), Some(1792988171));
        // 不是 JWT 就返回 None，不能瞎猜
        assert_eq!(jwt_expires_at("abc"), None);
        assert_eq!(jwt_expires_at(""), None);
    }

    /// 角色 id 统一成带连字符的 36 位
    #[test]
    fn normalizes_profile_id() {
        assert_eq!(
            normalize_profile_id("f95cffca26ea339c91afc4c70b45edac"),
            "f95cffca-26ea-339c-91af-c4c70b45edac"
        );
        // 本来就带连字符的不变
        assert_eq!(
            normalize_profile_id("f95cffca-26ea-339c-91af-c4c70b45edac"),
            "f95cffca-26ea-339c-91af-c4c70b45edac"
        );
        // 不是 32 位的原样返回
        assert_eq!(normalize_profile_id("abc"), "abc");
    }

    /// 角色 id 准备了几种写法，服务端不认就换一种
    #[test]
    fn profile_id_candidates_cover_both_formats() {
        let plain = profile_id_candidates("f95cffca26ea339c91afc4c70b45edac");
        assert!(plain.contains(&"f95cffca26ea339c91afc4c70b45edac".to_string()));
        assert!(plain.contains(&"f95cffca-26ea-339c-91af-c4c70b45edac".to_string()));

        let dashed = profile_id_candidates("f95cffca-26ea-339c-91af-c4c70b45edac");
        assert!(dashed.contains(&"f95cffca26ea339c91afc4c70b45edac".to_string()));
    }

    #[test]
    fn translates_common_skin_site_errors() {
        assert_eq!(
            translate_error("Invalid credentials."),
            "用户名或密码错误"
        );
        assert_eq!(
            translate_error("User foo is not found."),
            "皮肤站上没有这个账号"
        );
        assert_eq!(
            translate_error("Invalid AccessToken. Please re-login."),
            "登录已失效，请重新登录"
        );
        assert_eq!(
            translate_error("Too frequent requests. Please try after 1 seconds."),
            "操作太频繁，请稍等片刻再试"
        );
        // 没见过的错误原样返回，不丢信息
        assert_eq!(translate_error("Something odd"), "Something odd");
    }
    /// 只有明确提到令牌无效才算失效，服务不可用不能算
    #[test]
    fn detects_invalid_token_response() {
        assert!(token_invalid(
            403,
            r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid AccessToken. Please re-login."}"#
        ));
        // 403 但说的是别的事，不算令牌失效
        assert!(!token_invalid(403, r#"{"error":"ForbiddenOperationException"}"#));
        // 其它状态码不算
        assert!(!token_invalid(500, "Internal Server Error"));
        assert!(!token_invalid(200, "{}"));
    }
}
