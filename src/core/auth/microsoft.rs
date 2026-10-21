use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::core::download::Downloader;

/// 微软 OAuth 客户端 ID。可在设置中替换为自己的 Azure 应用 ID。
pub const DEFAULT_CLIENT_ID: &str = "00000000402b5328";

const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub interval: i64,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

#[derive(Debug, Clone)]
pub struct MicrosoftAuth {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub uuid: String,
    pub name: String,
    pub xuid: String,
    pub skin_url: Option<String>,
    pub skin_variant: String,
}

pub enum PollOutcome {
    Pending,
    SlowDown,
    Declined,
    Expired,
    Done(Box<MicrosoftAuth>),
}

pub fn request_device_code(dl: &Downloader, client_id: &str) -> Result<DeviceCode> {
    let code: DeviceCode = dl.post_form(
        DEVICE_CODE_URL,
        &[("client_id", client_id), ("scope", SCOPE)],
    )?;
    Ok(code)
}

pub fn poll_device_code(dl: &Downloader, client_id: &str, device_code: &str) -> Result<PollOutcome> {
    let resp = dl
        .client()
        .post(TOKEN_URL)
        .form(&[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:device_code",
            ),
            ("client_id", client_id),
            ("device_code", device_code),
        ])
        .send()?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    let parsed: TokenResponse = serde_json::from_str(&text).unwrap_or(TokenResponse {
        access_token: String::new(),
        refresh_token: String::new(),
        expires_in: 0,
        error: format!("http_{}", status.as_u16()),
        error_description: crate::core::download::truncate(&text, 200),
    });

    if !parsed.access_token.is_empty() {
        let auth = finish_login(dl, parsed.access_token, parsed.refresh_token, parsed.expires_in)?;
        return Ok(PollOutcome::Done(Box::new(auth)));
    }

    match parsed.error.as_str() {
        "authorization_pending" => Ok(PollOutcome::Pending),
        "slow_down" => Ok(PollOutcome::SlowDown),
        "authorization_declined" => Ok(PollOutcome::Declined),
        "expired_token" | "bad_verification_code" => Ok(PollOutcome::Expired),
        other => Err(anyhow!(
            "登录失败: {} {}",
            other,
            parsed.error_description
        )),
    }
}

pub fn refresh(dl: &Downloader, client_id: &str, refresh_token: &str) -> Result<MicrosoftAuth> {
    let parsed: TokenResponse = dl.post_form(
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ],
    )?;
    if parsed.access_token.is_empty() {
        return Err(anyhow!(
            "刷新登录状态失败: {} {}",
            parsed.error,
            parsed.error_description
        ));
    }
    let refresh = if parsed.refresh_token.is_empty() {
        refresh_token.to_string()
    } else {
        parsed.refresh_token
    };
    finish_login(dl, parsed.access_token, refresh, parsed.expires_in)
}

fn finish_login(
    dl: &Downloader,
    ms_token: String,
    refresh_token: String,
    expires_in: i64,
) -> Result<MicrosoftAuth> {
    let (xbl_token, uhs) = xbox_authenticate(dl, &ms_token)?;
    let (xsts_token, xsts_uhs) = xsts_authorize(dl, &xbl_token)?;
    let mc_token = minecraft_login(dl, &xsts_uhs, &xsts_token)?;
    let profile = minecraft_profile(dl, &mc_token)?;

    Ok(MicrosoftAuth {
        access_token: mc_token,
        refresh_token,
        expires_at: chrono::Utc::now().timestamp() + expires_in.max(3600),
        uuid: profile.id,
        name: profile.name,
        xuid: uhs,
        skin_url: profile.skins.first().map(|s| s.url.clone()),
        skin_variant: profile
            .skins
            .first()
            .map(|s| s.variant.clone())
            .unwrap_or_else(|| "classic".into()),
    })
}

#[derive(Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
}

fn xbox_authenticate(dl: &Downloader, ms_token: &str) -> Result<(String, String)> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    let resp: XblResponse = dl
        .post_json(XBL_URL, &body)
        .map_err(|e| anyhow!("Xbox Live 认证失败: {e}"))?;
    let uhs = resp
        .display_claims
        .xui
        .first()
        .map(|x| x.uhs.clone())
        .ok_or_else(|| anyhow!("Xbox Live 返回数据异常"))?;
    Ok((resp.token, uhs))
}

fn xsts_authorize(dl: &Downloader, xbl_token: &str) -> Result<(String, String)> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });
    let resp = dl.client().post(XSTS_URL).json(&body).send()?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();

    if !status.is_success() {
        // XSTS 错误码
        if text.contains("2148916233") {
            return Err(anyhow!(
                "该微软账号还没有 Xbox 档案，请先登录 xbox.com 创建档案后再试"
            ));
        }
        if text.contains("2148916238") {
            return Err(anyhow!("该账号为未成年人账号，需要由家庭组成人账号添加到家庭后才能登录"));
        }
        if text.contains("2148916227") {
            return Err(anyhow!("该账号已被 Xbox Live 封禁"));
        }
        return Err(anyhow!(
            "XSTS 授权失败: {}",
            crate::core::download::truncate(&text, 200)
        ));
    }

    let parsed: XblResponse = serde_json::from_str(&text)
        .map_err(|_| anyhow!("XSTS 返回数据异常: {}", crate::core::download::truncate(&text, 200)))?;
    let uhs = parsed
        .display_claims
        .xui
        .first()
        .map(|x| x.uhs.clone())
        .ok_or_else(|| anyhow!("XSTS 返回数据异常"))?;
    Ok((parsed.token, uhs))
}

#[derive(Deserialize)]
struct McLoginResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    expires_in: i64,
}

fn minecraft_login(dl: &Downloader, uhs: &str, xsts_token: &str) -> Result<String> {
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
    });
    let resp: McLoginResponse = dl
        .post_json(MC_LOGIN_URL, &body)
        .map_err(|e| anyhow!("Minecraft 登录失败: {e}"))?;
    if resp.access_token.is_empty() {
        return Err(anyhow!("Minecraft 登录失败：未获取到访问令牌"));
    }
    let _ = resp.expires_in;
    Ok(resp.access_token)
}

#[derive(Debug, Deserialize)]
pub struct McProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<McSkin>,
}

#[derive(Debug, Deserialize)]
pub struct McSkin {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub variant: String,
}

pub fn minecraft_profile(dl: &Downloader, access_token: &str) -> Result<McProfile> {
    let bearer = format!("Bearer {access_token}");
    let resp = dl
        .client()
        .get(MC_PROFILE_URL)
        .header("Authorization", bearer)
        .send()?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if status.as_u16() == 404 {
        return Err(anyhow!("该微软账号尚未拥有 Minecraft：Java 版"));
    }
    if !status.is_success() {
        return Err(anyhow!(
            "获取 Minecraft 档案失败 (HTTP {}): {}",
            status.as_u16(),
            crate::core::download::truncate(&text, 200)
        ));
    }
    serde_json::from_str(&text).map_err(|_| anyhow!("Minecraft 档案数据异常"))
}
