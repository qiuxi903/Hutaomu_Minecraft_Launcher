//! 本地凭据的加密保存
//!
//! Windows 上用 DPAPI（用户级）加密：密文只有当前 Windows 用户能解开，
//! 换台电脑或换个用户都读不出来，也不用自己管密钥。
//! 其它平台没有系统级的密钥管理，退化成 base64 编码，
//! 至少不会在文件里直接看到明文。

use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// 加密保存一段凭据
pub fn protect(plain: &str) -> Result<String> {
    #[cfg(windows)]
    {
        windows_protect(plain.as_bytes())
    }
    #[cfg(not(windows))]
    {
        Ok(STANDARD.encode(plain.as_bytes()))
    }
}

/// 解开之前 `protect` 保存的凭据
///
/// 解不开就当作没有保存过，让用户重新登录即可。
pub fn unprotect(encoded: &str) -> Option<String> {
    #[cfg(windows)]
    {
        let bytes = windows_unprotect(encoded)?;
        String::from_utf8(bytes).ok()
    }
    #[cfg(not(windows))]
    {
        let bytes = STANDARD.decode(encoded).ok()?;
        String::from_utf8(bytes).ok()
    }
}

/// 当前系统是否支持真正的加密（而不是仅仅编码）
pub fn is_encrypted() -> bool {
    cfg!(windows)
}

#[cfg(windows)]
fn windows_protect(data: &[u8]) -> Result<String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null::<c_void>(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(anyhow::anyhow!("加密凭据失败"));
    }
    let encoded = unsafe {
        STANDARD.encode(std::slice::from_raw_parts(output.pbData, output.cbData as usize))
    };
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(encoded)
}

#[cfg(windows)]
fn windows_unprotect(encoded: &str) -> Option<Vec<u8>> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let raw = STANDARD.decode(encoded).ok()?;
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: raw.len() as u32,
        pbData: raw.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null::<c_void>(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return None;
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData as _);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let secret = "皮肤站密码 p@ssw0rd";
        let encoded = protect(secret).unwrap();
        assert!(!encoded.contains("p@ssw0rd"));
        assert_eq!(unprotect(&encoded).as_deref(), Some(secret));
    }

    #[test]
    fn garbage_is_not_a_secret() {
        assert!(unprotect("不是密文").is_none());
        assert!(unprotect("").is_none());
    }
}
