pub mod authlib;
pub mod microsoft;
pub mod offline;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::minecraft::launch::LaunchIdentity;
use crate::core::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkinModel {
    #[default]
    Classic,
    Slim,
}

impl SkinModel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "经典（4px 手臂）",
            Self::Slim => "纤细（3px 手臂）",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Classic => "经典",
            Self::Slim => "纤细",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineAccount {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skin_path: Option<PathBuf>,
    #[serde(default)]
    pub skin_model: SkinModel,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrosoftAccount {
    pub id: String,
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub xuid: String,
    #[serde(default)]
    pub skin_path: Option<PathBuf>,
    #[serde(default)]
    pub skin_model: SkinModel,
    #[serde(default)]
    pub skin_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthlibAccount {
    pub id: String,
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    #[serde(default)]
    pub client_token: String,
    pub server_url: String,
    #[serde(default)]
    pub skin_path: Option<PathBuf>,
    #[serde(default)]
    pub skin_model: SkinModel,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub skin_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Account {
    Offline(OfflineAccount),
    Microsoft(MicrosoftAccount),
    Authlib(AuthlibAccount),
}

impl Account {
    pub fn id(&self) -> &str {
        match self {
            Self::Offline(a) => &a.id,
            Self::Microsoft(a) => &a.id,
            Self::Authlib(a) => &a.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Offline(a) => &a.name,
            Self::Microsoft(a) => &a.name,
            Self::Authlib(a) => &a.name,
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Offline(_) => "离线登录",
            Self::Microsoft(_) => "微软正版",
            Self::Authlib(_) => "皮肤站",
        }
    }

    pub fn skin_path(&self) -> Option<&Path> {
        match self {
            Self::Offline(a) => a.skin_path.as_deref(),
            Self::Microsoft(a) => a.skin_path.as_deref(),
            Self::Authlib(a) => a.skin_path.as_deref(),
        }
    }

    pub fn skin_model(&self) -> SkinModel {
        match self {
            Self::Offline(a) => a.skin_model,
            Self::Microsoft(a) => a.skin_model,
            Self::Authlib(a) => a.skin_model,
        }
    }

    pub fn set_skin_path(&mut self, path: Option<PathBuf>) {
        match self {
            Self::Offline(a) => a.skin_path = path,
            Self::Microsoft(a) => a.skin_path = path,
            Self::Authlib(a) => a.skin_path = path,
        }
    }

    pub fn set_skin_model(&mut self, model: SkinModel) {
        match self {
            Self::Offline(a) => a.skin_model = model,
            Self::Microsoft(a) => a.skin_model = model,
            Self::Authlib(a) => a.skin_model = model,
        }
    }

    pub fn set_skin_url(&mut self, url: Option<String>) {
        match self {
            Self::Offline(_) => {}
            Self::Microsoft(a) => a.skin_url = url,
            Self::Authlib(a) => a.skin_url = url,
        }
    }

    pub fn skin_url(&self) -> Option<&str> {
        match self {
            Self::Offline(_) => None,
            Self::Microsoft(a) => a.skin_url.as_deref(),
            Self::Authlib(a) => a.skin_url.as_deref(),
        }
    }

    pub fn uuid(&self) -> &str {
        match self {
            Self::Offline(a) => &a.id,
            Self::Microsoft(a) => &a.uuid,
            Self::Authlib(a) => &a.uuid,
        }
    }

    pub fn authlib_url(&self) -> Option<&str> {
        match self {
            Self::Authlib(a) => Some(&a.server_url),
            _ => None,
        }
    }

    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline(_))
    }

    pub fn is_expired(&self) -> bool {
        let expires = match self {
            Self::Offline(_) => return false,
            Self::Microsoft(a) => a.expires_at,
            Self::Authlib(a) => a.expires_at,
        };
        expires != 0 && expires <= chrono::Utc::now().timestamp() + 60
    }

    pub fn needs_refresh(&self) -> bool {
        let expires = match self {
            Self::Offline(_) => return false,
            Self::Microsoft(a) => a.expires_at,
            Self::Authlib(a) => a.expires_at,
        };
        expires != 0 && expires <= chrono::Utc::now().timestamp() + 3600
    }

    pub fn to_identity(&self) -> LaunchIdentity {
        match self {
            Self::Offline(a) => LaunchIdentity {
                username: a.name.clone(),
                uuid: a.id.replace('-', ""),
                access_token: "0".into(),
                user_type: "legacy".into(),
                xuid: String::new(),
                client_id: String::new(),
                user_properties: "{}".into(),
                authlib_url: None,
            },
            Self::Microsoft(a) => LaunchIdentity {
                username: a.name.clone(),
                uuid: a.uuid.replace('-', ""),
                access_token: a.access_token.clone(),
                user_type: "msa".into(),
                xuid: a.xuid.clone(),
                client_id: String::new(),
                user_properties: "{}".into(),
                authlib_url: None,
            },
            Self::Authlib(a) => LaunchIdentity {
                username: a.name.clone(),
                uuid: a.uuid.replace('-', ""),
                access_token: a.access_token.clone(),
                user_type: "mojang".into(),
                xuid: String::new(),
                client_id: String::new(),
                user_properties: "{}".into(),
                authlib_url: Some(a.server_url.clone()),
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
    pub selected: Option<String>,
}

impl AccountStore {
    pub fn load() -> Self {
        let path = paths::accounts_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                log::warn!("账号文件解析失败: {err}");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = paths::accounts_file();
        paths::ensure_parent(&path)?;
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn add(&mut self, account: Account) {
        if let Some(existing) = self
            .accounts
            .iter_mut()
            .find(|a| a.id() == account.id() && a.kind_label() == account.kind_label())
        {
            *existing = account;
        } else {
            self.accounts.push(account);
        }
        if self.selected.is_none() {
            self.selected = self.accounts.last().map(|a| a.id().to_string());
        }
    }

    pub fn upsert(&mut self, account: Account) {
        let id = account.id().to_string();
        let kind = account.kind_label();
        if let Some(existing) = self
            .accounts
            .iter_mut()
            .find(|a| a.id() == id && a.kind_label() == kind)
        {
            *existing = account;
        } else {
            self.accounts.push(account);
        }
    }

    pub fn remove(&mut self, id: &str, kind: &str) {
        self.accounts
            .retain(|a| !(a.id() == id && a.kind_label() == kind));
        if self.selected.as_deref() == Some(id) {
            self.selected = self.accounts.first().map(|a| a.id().to_string());
        }
    }

    pub fn get(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id() == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.id() == id)
    }

    pub fn selected_account(&self) -> Option<&Account> {
        self.selected
            .as_deref()
            .and_then(|id| self.get(id))
            .or_else(|| self.accounts.first())
    }

    pub fn selected_account_mut(&mut self) -> Option<&mut Account> {
        let id = self
            .selected
            .clone()
            .or_else(|| self.accounts.first().map(|a| a.id().to_string()))?;
        self.get_mut(&id)
    }

    pub fn offline_count(&self) -> usize {
        self.accounts.iter().filter(|a| a.is_offline()).count()
    }
}
