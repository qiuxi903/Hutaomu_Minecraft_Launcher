use md5::{Digest, Md5};

/// 离线模式 UUID：MD5("OfflinePlayer:" + name)，按 v3 UUID 规则设置版本位
pub fn offline_uuid(name: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes = hasher.finalize();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

pub fn create(name: &str) -> super::Account {
    let uuid = offline_uuid(name);
    super::Account::Offline(super::OfflineAccount {
        id: uuid,
        name: name.to_string(),
        skin_path: None,
        skin_model: super::SkinModel::Classic,
        created_at: chrono::Utc::now().timestamp(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_matches_mojang_offline_algorithm() {
        // 已知结果：Notch 的离线 UUID
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }
}
