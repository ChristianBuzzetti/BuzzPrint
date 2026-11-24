//! Arrastrar capturas a otras apps (Outlook, Teams, Word, el Explorador) con el arrastre del shell de Windows:
//! los archivos se ofrecen como en el Explorador (CF_HDROP y formatos del shell). Es modal: se llama fuera de
//! un frame, con `Cmd::Run`.

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Ole::{OleInitialize, DROPEFFECT_COPY};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHCreateDataObject, SHDoDragDrop};

use crate::win::wide_os;

const IID_IDATAOBJECT: GUID = GUID::from_u128(0x0000010e_0000_0000_c000_000000000046);

fn ole() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| unsafe { OleInitialize(null()) >= 0 })
}

/// Libera una interfaz COM (llama a IUnknown::Release por su tabla virtual).
unsafe fn release(obj: *mut c_void) {
    if obj.is_null() {
        return;
    }
    let vtbl = *(obj as *const *const usize);
    let release: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(*vtbl.add(2));
    release(obj);
}

pub fn drag_files(hwnd: HWND, files: &[PathBuf]) {
    if files.is_empty() || !ole() {
        return;
    }
    unsafe {
        let pidls: Vec<*mut ITEMIDLIST> = files.iter().map(|f| ILCreateFromPathW(wide_os(f.as_os_str()).as_ptr())).filter(|p| !p.is_null()).collect();
        if pidls.is_empty() {
            return;
        }
        let mut data: *mut c_void = null_mut();
        let hr = SHCreateDataObject(null(), pidls.len() as u32, pidls.as_ptr() as *const *const ITEMIDLIST, null_mut(), &IID_IDATAOBJECT, &mut data);
        if hr >= 0 && !data.is_null() {
            let mut effect = 0u32;
            SHDoDragDrop(hwnd, data as _, null_mut(), DROPEFFECT_COPY, &mut effect);
            release(data);
        }
        for p in pidls {
            ILFree(p);
        }
    }
}
