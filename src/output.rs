//! Portapapeles (imagen como CF_DIB + PNG, archivos como CF_HDROP), abrir archivos y pegar en otra app.

use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{GlobalFree, HWND};
use windows_sys::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use windows_sys::Win32::System::Threading::Sleep;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V};
use windows_sys::Win32::UI::Shell::{ShellExecuteW, DROPFILES};
use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, SW_SHOWNORMAL};

use crate::image::{png_bytes, Rgba};
use crate::win::{wide, wide_os};

unsafe fn global_from(bytes: &[u8]) -> *mut core::ffi::c_void {
    let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
    if mem.is_null() {
        return mem;
    }
    let p = GlobalLock(mem) as *mut u8;
    if p.is_null() {
        GlobalFree(mem);
        return null_mut();
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    GlobalUnlock(mem);
    mem
}

unsafe fn open_clipboard(owner: HWND) -> bool {
    for _ in 0..10 {
        if OpenClipboard(owner) != 0 {
            return true;
        }
        Sleep(15);
    }
    false
}

/// Publica HTML de Windows y el mismo resultado como texto para las apps que solo pegan texto plano.
pub fn copy_html(owner: HWND, fragment: &str, plain: &str) -> bool {
    let html = clipboard_html(fragment);
    let format = unsafe { RegisterClipboardFormatW(wide("HTML Format").as_ptr()) };
    format != 0 && copy_text_formats(owner, plain, Some((format, html.as_bytes())))
}

pub fn copy_text(owner: HWND, plain: &str) -> bool {
    copy_text_formats(owner, plain, None)
}

fn copy_text_formats(owner: HWND, plain: &str, html: Option<(u32, &[u8])>) -> bool {
    let plain = wide(&plain.replace("\r\n", "\n").replace('\n', "\r\n"));
    let plain_bytes = unsafe { std::slice::from_raw_parts(plain.as_ptr().cast(), plain.len() * 2) };
    unsafe {
        if !open_clipboard(owner) {
            return false;
        }
        let html_mem = html.map(|(format, bytes)| (format, global_from(bytes)));
        let plain_mem = global_from(plain_bytes);
        let ready = html_mem.is_none_or(|(_, mem)| !mem.is_null()) && !plain_mem.is_null() && EmptyClipboard() != 0;
        let html_ok = html_mem.is_none_or(|(format, mem)| {
            let ok = ready && !SetClipboardData(format, mem).is_null();
            if !ok && !mem.is_null() {
                GlobalFree(mem);
            }
            ok
        });
        let plain_ok = ready && !SetClipboardData(CF_UNICODETEXT as u32, plain_mem).is_null();
        if !plain_ok && !plain_mem.is_null() {
            GlobalFree(plain_mem);
        }
        CloseClipboard();
        html_ok && plain_ok
    }
}

fn clipboard_html(fragment: &str) -> String {
    const BEFORE: &str = "<html><head><meta charset=\"utf-8\"></head><body><!--StartFragment-->";
    const AFTER: &str = "<!--EndFragment--></body></html>";
    let header = |html_start, html_end, fragment_start, fragment_end| {
        format!("Version:0.9\r\nStartHTML:{html_start:010}\r\nEndHTML:{html_end:010}\r\nStartFragment:{fragment_start:010}\r\nEndFragment:{fragment_end:010}\r\n")
    };
    let html_start = header(0, 0, 0, 0).len();
    let fragment_start = html_start + BEFORE.len();
    let fragment_end = fragment_start + fragment.len();
    let html_end = fragment_end + AFTER.len();
    format!("{}{BEFORE}{fragment}{AFTER}\0", header(html_start, html_end, fragment_start, fragment_end))
}

fn dib_bytes(img: &Rgba) -> Vec<u8> {
    let (w, h) = (img.w as usize, img.h as usize);
    let hdr = size_of::<BITMAPINFOHEADER>();
    let stride = w * 4;
    let mut v = vec![0u8; hdr + stride * h];
    let mut bih: BITMAPINFOHEADER = unsafe { zeroed() };
    bih.biSize = hdr as u32;
    bih.biWidth = w as i32;
    bih.biHeight = h as i32; // de abajo hacia arriba: lo más compatible
    bih.biPlanes = 1;
    bih.biBitCount = 32;
    bih.biCompression = BI_RGB as _;
    bih.biSizeImage = (stride * h) as u32;
    unsafe { std::ptr::copy_nonoverlapping(&bih as *const _ as *const u8, v.as_mut_ptr(), hdr) };
    for (y, row) in v[hdr..].chunks_exact_mut(stride).enumerate() {
        let src = &img.px[(h - 1 - y) * stride..][..stride];
        for (d, s) in row.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = 255;
        }
    }
    v
}

/// Copia una imagen: CF_DIB para todas las apps y "PNG" para las que lo prefieren (Office, navegadores).
pub fn copy_image(owner: HWND, img: &Rgba) -> bool {
    let dib = dib_bytes(img);
    let png = png_bytes(img).unwrap_or_default();
    unsafe {
        if !open_clipboard(owner) {
            return false;
        }
        EmptyClipboard();
        let mut ok = false;
        let h = global_from(&dib);
        if !h.is_null() {
            ok = !SetClipboardData(CF_DIB as u32, h).is_null();
            if !ok {
                GlobalFree(h);
            }
        }
        if !png.is_empty() {
            let fmt = RegisterClipboardFormatW(wide("PNG").as_ptr());
            let h = global_from(&png);
            if !h.is_null() && SetClipboardData(fmt, h).is_null() {
                GlobalFree(h);
            }
        }
        CloseClipboard();
        ok
    }
}

/// Copia archivos (como "Copiar" en el Explorador): para grabaciones y para pegar capturas como archivo.
pub fn copy_files(owner: HWND, files: &[PathBuf]) -> bool {
    let mut list: Vec<u16> = Vec::new();
    for f in files {
        list.extend(wide_os(f.as_os_str()));
    }
    list.push(0);
    let hdr = size_of::<DROPFILES>();
    let mut bytes = vec![0u8; hdr + list.len() * 2];
    let mut df: DROPFILES = unsafe { zeroed() };
    df.pFiles = hdr as u32;
    df.fWide = 1;
    unsafe {
        std::ptr::copy_nonoverlapping(&df as *const _ as *const u8, bytes.as_mut_ptr(), hdr);
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, bytes.as_mut_ptr().add(hdr), list.len() * 2);
        if !open_clipboard(owner) {
            return false;
        }
        EmptyClipboard();
        let h = global_from(&bytes);
        let ok = !h.is_null() && !SetClipboardData(CF_HDROP as u32, h).is_null();
        if !ok && !h.is_null() {
            GlobalFree(h);
        }
        CloseClipboard();
        ok
    }
}

/// Abre un archivo o carpeta con la app predeterminada.
pub fn shell_open(path: &Path) {
    let verb = wide("open");
    let file = wide_os(path.as_os_str());
    unsafe {
        ShellExecuteW(null_mut(), verb.as_ptr(), file.as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

/// Abre el Explorador con el archivo seleccionado.
pub fn reveal(path: &Path) {
    let verb = wide("open");
    let exe = wide("explorer.exe");
    let args = wide(&format!("/select,\"{}\"", path.display()));
    unsafe {
        ShellExecuteW(null_mut(), verb.as_ptr(), exe.as_ptr(), args.as_ptr(), null(), SW_SHOWNORMAL);
    }
}

pub fn open_url(url: &str) -> bool {
    let verb = wide("open");
    let u = wide(url);
    unsafe {
        ShellExecuteW(null_mut(), verb.as_ptr(), u.as_ptr(), null(), null(), SW_SHOWNORMAL) as isize > 32
    }
}

#[cfg(test)]
mod tests {
    use super::clipboard_html;

    #[test]
    fn clipboard_offsets_are_utf8_byte_positions() {
        let fragment = "<p>Capturé un error · 日本語</p>";
        let payload = clipboard_html(fragment);
        let offset = |key: &str| -> usize {
            payload.lines().find_map(|line| line.strip_prefix(key)).unwrap().parse().unwrap()
        };
        assert_eq!(&payload[offset("StartFragment:")..offset("EndFragment:")], fragment);
        assert!(payload[offset("StartHTML:")..offset("EndHTML:")].starts_with("<html>"));
        assert!(payload[..offset("EndHTML:")].ends_with("</html>"));
        assert_eq!(&payload[offset("EndHTML:")..], "\0");
    }
}

/// Vuelve a la ventana que estaba activa y pega (Ctrl+V).
pub fn paste_into(previous: HWND) {
    unsafe {
        if previous.is_null() {
            return;
        }
        SetForegroundWindow(previous);
        Sleep(120);
        let key = |vk: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 } },
        };
        let inputs = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
        SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32);
    }
}
