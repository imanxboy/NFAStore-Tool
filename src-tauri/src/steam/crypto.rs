use base64::Engine;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

// Steam formats the CRC32 key as hex with leading zeros stripped and a trailing "1".
pub(crate) fn compute_crc32(data: &str) -> String {
    let crc32_value = crc32fast::hash(data.as_bytes());
    let hex = format!("{crc32_value:08x}");
    let trimmed = hex.trim_start_matches('0');
    if trimmed.is_empty() {
        "01".to_string()
    } else {
        format!("{trimmed}1")
    }
}

// DPAPI (CryptProtectData) with the account name as entropy and Steam's "BObfuscateBuffer" description blob.
pub(crate) fn steam_encrypt(token: &str, account_name: &str) -> Result<String, String> {
    let data_to_encrypt = token.as_bytes();
    let byte_string =
        b"B\x00O\x00b\x00f\x00u\x00s\x00c\x00a\x00t\x00e\x00B\x00u\x00f\x00f\x00e\x00r\x00\x00\x00";
    let account_name_bytes = account_name.as_bytes();

    let data_in = CRYPT_INTEGER_BLOB {
        cbData: data_to_encrypt.len() as u32,
        pbData: data_to_encrypt.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: account_name_bytes.len() as u32,
        pbData: account_name_bytes.as_ptr() as *mut u8,
    };

    let description = String::from_utf8_lossy(byte_string);
    let description_wide: Vec<u16> = description.encode_utf16().chain(Some(0)).collect();
    let description_pcwstr = windows::core::PCWSTR(description_wide.as_ptr());
    let mut data_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        let success = CryptProtectData(
            &data_in,
            description_pcwstr,
            Some(&entropy),
            None,
            None,
            0x11,
            &mut data_out,
        );
        if success.is_err() {
            return Err("CryptProtectData failed".to_string());
        }

        let encrypted_slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let hex_string = encrypted_slice
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn LocalFree(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }
        LocalFree(data_out.pbData as *mut std::ffi::c_void);

        Ok(hex_string)
    }
}

/// Reverse of [`steam_encrypt`]: recover a token Steam saved in its own
/// ConnectCache. Steam sealed it with DPAPI under the current Windows user and
/// the account name as entropy, so the same user — which is who this app runs as
/// — can open it. This is how an account that was signed in through the Steam
/// client, and so has no token of ours, still gets a token to check its rank.
pub(crate) fn steam_decrypt(encrypted_hex: &str, account_name: &str) -> Result<String, String> {
    let hex = encrypted_hex.trim();
    if hex.is_empty() || hex.len() % 2 != 0 {
        return Err("ConnectCache value is not valid hex.".to_string());
    }
    let bytes = hex.as_bytes();
    let mut raw = Vec::with_capacity(hex.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char)
            .to_digit(16)
            .ok_or("ConnectCache value is not valid hex.")?;
        let lo = (bytes[i + 1] as char)
            .to_digit(16)
            .ok_or("ConnectCache value is not valid hex.")?;
        raw.push(((hi << 4) | lo) as u8);
        i += 2;
    }

    let account_name_bytes = account_name.as_bytes();
    let data_in = CRYPT_INTEGER_BLOB {
        cbData: raw.len() as u32,
        pbData: raw.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: account_name_bytes.len() as u32,
        pbData: account_name_bytes.as_ptr() as *mut u8,
    };
    let mut data_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(
            &data_in,
            None,
            Some(&entropy),
            None,
            None,
            UI_FORBIDDEN,
            &mut data_out,
        )
        .map_err(|_| "CryptUnprotectData failed".to_string())?;

        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let plain = String::from_utf8_lossy(slice).into_owned();
        local_free(data_out.pbData);
        Ok(plain)
    }
}

// ---------------------------------------------------------------------------
// At-rest protection for our own account store.
//
// Steam's copy of the token is already DPAPI-encrypted above, in Steam's exact
// format. Our store is a second copy, kept so a sign-in can re-provision Steam
// after a cache reset — and left in plain text it would hand every token to
// anything that can read the user's %APPDATA%. So it gets the same DPAPI
// treatment, scoped to the current user, with a fixed entropy string of our own
// so a blob lifted from our file is not interchangeable with Steam's.
// ---------------------------------------------------------------------------

const STORE_ENTROPY: &[u8] = b"ir.nfastore.tool/accounts";

/// CRYPTPROTECT_UI_FORBIDDEN — never raise a prompt, fail instead.
const UI_FORBIDDEN: u32 = 0x1;

/// DPAPI-encrypt a string for the current Windows user, base64 for JSON.
pub(crate) fn protect_for_user(plain: &str) -> Result<String, String> {
    let data = plain.as_bytes();
    let data_in = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: STORE_ENTROPY.len() as u32,
        pbData: STORE_ENTROPY.as_ptr() as *mut u8,
    };
    let mut data_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(
            &data_in,
            windows::core::PCWSTR::null(),
            Some(&entropy),
            None,
            None,
            UI_FORBIDDEN,
            &mut data_out,
        )
        .map_err(|_| "CryptProtectData failed".to_string())?;

        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let encoded = base64::engine::general_purpose::STANDARD.encode(slice);
        local_free(data_out.pbData);
        Ok(encoded)
    }
}

/// Reverse of `protect_for_user`. Fails on another user account or another PC,
/// which is the point.
pub(crate) fn unprotect_for_user(encoded: &str) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Stored token is not valid base64.".to_string())?;

    let data_in = CRYPT_INTEGER_BLOB {
        cbData: raw.len() as u32,
        pbData: raw.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: STORE_ENTROPY.len() as u32,
        pbData: STORE_ENTROPY.as_ptr() as *mut u8,
    };
    let mut data_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(
            &data_in,
            None,
            Some(&entropy),
            None,
            None,
            UI_FORBIDDEN,
            &mut data_out,
        )
        .map_err(|_| "CryptUnprotectData failed".to_string())?;

        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let plain = String::from_utf8_lossy(slice).into_owned();
        local_free(data_out.pbData);
        Ok(plain)
    }
}

// Only ever called with a buffer DPAPI just handed back, which is exactly what
// LocalFree expects.
fn local_free(ptr: *mut u8) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    unsafe {
        LocalFree(ptr as *mut std::ffi::c_void);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protect_round_trips() {
        let secret = "eyJ0eXAiOiJKV1QifQ.payload.signature";
        let sealed = protect_for_user(secret).expect("protect");
        assert_ne!(sealed, secret);
        assert_eq!(unprotect_for_user(&sealed).expect("unprotect"), secret);
    }

    #[test]
    fn rejects_garbage() {
        assert!(unprotect_for_user("not base64 at all !!").is_err());
        assert!(unprotect_for_user("aGVsbG8gd29ybGQ=").is_err());
    }

    #[test]
    fn steam_encrypt_round_trips() {
        // What we write into Steam's ConnectCache we must be able to read back,
        // so an account signed in through Steam can be rank-checked. The entropy
        // is the account name — a different name must not open the blob.
        let token = "eyJ0eXAiOiJKV1QifQ.eyJzdWIiOiI3NjU2MTE5OTAwMDAwMDAwMCJ9.sig";
        let name = "some_account";
        let sealed = steam_encrypt(token, name).expect("encrypt");
        assert_ne!(sealed, token);
        assert_eq!(steam_decrypt(&sealed, name).expect("decrypt"), token);
        assert!(steam_decrypt(&sealed, "other_account").is_err());
        assert!(steam_decrypt("nothex", name).is_err());
    }
}

