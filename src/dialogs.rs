//! Diálogos nativos de Windows (guardar archivo, elegir carpeta, elegir color).
//! Son modales: se llaman solo fuera de un frame (con `Cmd::Run`).

use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_EXPLORER, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
use windows_sys::Win32::UI::Shell::{SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW};
use windows_sys::Win32::System::Com::CoTaskMemFree;

use crate::win::wide;

/// `filters`: pares (nombre, patrón), por ejemplo ("PNG", "*.png"). Devuelve la ruta y el índice del filtro (1..).
pub fn save_file(owner: HWND, title: &str, file_name: &str, filters: &[(&str, &str)], dir: Option<&str>) -> Option<(PathBuf, u32)> {
    let mut filter: Vec<u16> = Vec::new();
    for (n, p) in filters {
        filter.extend(n.encode_utf16());
        filter.push(0);
        filter.extend(p.encode_utf16());
        filter.push(0);
    }
    filter.push(0);
    let mut buf = vec![0u16; 1024];
    for (i, c) in file_name.encode_utf16().take(1000).enumerate() {
        buf[i] = c;
    }
    let title = wide(title);
    let dir_w = dir.map(wide);
    let def_ext = filters.first().map(|(_, p)| wide(p.trim_start_matches("*.")));
    let mut ofn: OPENFILENAMEW = unsafe { zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.nFilterIndex = 1;
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    if let Some(d) = &dir_w {
        ofn.lpstrInitialDir = d.as_ptr();
    }
    if let Some(e) = &def_ext {
        ofn.lpstrDefExt = e.as_ptr();
    }
    ofn.Flags = OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST;
    unsafe {
        if GetSaveFileNameW(&mut ofn) == 0 {
            return None;
        }
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some((PathBuf::from(String::from_utf16_lossy(&buf[..n])), ofn.nFilterIndex))
}

/// Elegir un archivo existente. `filters`: pares (nombre, patrones separados por ';').
pub fn open_file(owner: HWND, title: &str, filters: &[(&str, &str)]) -> Option<PathBuf> {
    let mut filter: Vec<u16> = Vec::new();
    for (n, p) in filters {
        filter.extend(n.encode_utf16());
        filter.push(0);
        filter.extend(p.encode_utf16());
        filter.push(0);
    }
    filter.push(0);
    let mut buf = vec![0u16; 1024];
    let title = wide(title);
    let mut ofn: OPENFILENAMEW = unsafe { zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.nFilterIndex = 1;
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST;
    unsafe {
        if GetOpenFileNameW(&mut ofn) == 0 {
            return None;
        }
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

pub fn pick_folder(owner: HWND, title: &str) -> Option<PathBuf> {
    let title = wide(title);
    let mut bi: BROWSEINFOW = unsafe { zeroed() };
    bi.hwndOwner = owner;
    bi.lpszTitle = title.as_ptr();
    bi.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
    unsafe {
        let pidl = SHBrowseForFolderW(&bi);
        if pidl.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let ok = SHGetPathFromIDListW(pidl, buf.as_mut_ptr()) != 0;
        CoTaskMemFree(pidl as _);
        if !ok {
            return None;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
    }
}

#[allow(dead_code)]
fn _null() -> HWND {
    null_mut()
}
