//! Utilidades Win32 chicas y sin estado.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null;

use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, POINT};
use windows_sys::Win32::System::Threading::CreateMutexW;

/// Texto UTF-16 terminado en 0 para la API de Windows.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn wide_os(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Copia `s` en un buffer fijo de la API (cortando si no entra), siempre terminado en 0.
pub fn copy_into(dst: &mut [u16], s: &str) {
    let mut n = 0;
    for c in s.encode_utf16() {
        if n + 1 >= dst.len() {
            break;
        }
        dst[n] = c;
        n += 1;
    }
    if !dst.is_empty() {
        dst[n] = 0;
    }
}

/// Coordenadas de mouse empaquetadas en LPARAM (con signo: hay monitores en negativo).
pub fn point_from_lparam(lp: isize) -> POINT {
    POINT {
        x: (lp as u32 & 0xFFFF) as i16 as i32,
        y: ((lp as u32 >> 16) & 0xFFFF) as i16 as i32,
    }
}

pub fn is_packaged() -> bool {
    static PACKAGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PACKAGED.get_or_init(|| {
        use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
        use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
        let mut len = 0;
        unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) == ERROR_INSUFFICIENT_BUFFER }
    })
}

/// Mantiene vivo el MTA del proceso para las fábricas WinRT que cachea el crate `windows`.
pub fn ensure_mta() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        let mut cookie = std::ptr::null_mut();
        windows_sys::Win32::System::Com::CoIncrementMTAUsage(&mut cookie);
    });
}

/// `true` si esta es la única instancia. El mutex vive mientras viva el proceso.
pub fn single_instance(name: &str) -> bool {
    let name = wide(name);
    unsafe {
        let h = CreateMutexW(null(), 0, name.as_ptr());
        if h.is_null() {
            return true;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // No retener la otra instancia: si se cierra, el mutex tiene que desaparecer.
            windows_sys::Win32::Foundation::CloseHandle(h);
            return false;
        }
        true
    }
}
