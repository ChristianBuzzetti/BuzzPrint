//! Criptografía de Windows, sin crates: SHA-256 (CNG/BCrypt) para verificar descargas y DPAPI para guardar las
//! API keys de la IA cifradas para el usuario actual (el mismo formato que `ProtectedData` de .NET).

use std::ffi::c_void;
use std::io::Read;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::*;

use crate::win::wide;

/// SHA-256 de un archivo, en hexadecimal minúscula.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    unsafe {
        let mut alg: BCRYPT_ALG_HANDLE = null_mut();
        let name = wide("SHA256");
        if BCryptOpenAlgorithmProvider(&mut alg, name.as_ptr(), null(), 0) < 0 {
            return Err("SHA-256 no disponible".into());
        }
        let mut hash: BCRYPT_HASH_HANDLE = null_mut();
        if BCryptCreateHash(alg, &mut hash, null_mut(), 0, null(), 0, 0) < 0 {
            BCryptCloseAlgorithmProvider(alg, 0);
            return Err("SHA-256 no disponible".into());
        }
        let result = (|| {
            let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                BCryptHashData(hash, buf.as_ptr(), n as u32, 0);
            }
            let mut out = [0u8; 32];
            if BCryptFinishHash(hash, out.as_mut_ptr(), 32, 0) < 0 {
                return Err("SHA-256 falló".to_string());
            }
            Ok(out.iter().map(|b| format!("{b:02x}")).collect())
        })();
        BCryptDestroyHash(hash);
        BCryptCloseAlgorithmProvider(alg, 0);
        result
    }
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

/// Cifra para el usuario actual de Windows (DPAPI), con entropía adicional.
pub fn protect(data: &[u8], entropy: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = blob(data);
        let ent = blob(entropy);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
        if CryptProtectData(&input, null(), &ent, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) == 0 {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as *mut c_void);
        Some(v)
    }
}

pub fn unprotect(data: &[u8], entropy: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = blob(data);
        let ent = blob(entropy);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
        if CryptUnprotectData(&input, null_mut(), &ent, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) == 0 {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as *mut c_void);
        Some(v)
    }
}

/// Base64 estándar (para mandar imágenes a la IA).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// Base64 estándar a bytes (`None` si hay caracteres inválidos).
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let s: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let s = s.strip_suffix(b"==").or_else(|| s.strip_suffix(b"=")).unwrap_or(&s);
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
}

#[cfg(test)]
mod b64_tests {
    #[test]
    fn roundtrip() {
        for data in [&b""[..], b"a", b"ab", b"abc", b"abcd", b"\x89PNG\r\n\x1a\n\x00\xff"] {
            assert_eq!(super::base64_decode(&super::base64(data)).unwrap(), data);
        }
    }
}
