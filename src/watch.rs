//! Vigilante de las carpetas de capturas y grabaciones: un hilo que duerme en el kernel y solo despierta
//! cuando algo cambia (otra app, el Explorador, la versión .NET). Avisa al núcleo para refrescar la biblioteca.

use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE,
};
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent, Sleep, WaitForMultipleObjects, INFINITE};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::win::wide_os;

pub const WM_LIBRARY_CHANGED: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 6;

static STOP: AtomicIsize = AtomicIsize::new(0);
static GEN: AtomicU64 = AtomicU64::new(0);

/// Empieza (o reinicia, tras cambiar de carpeta) la vigilancia.
pub fn restart() {
    let (cap, rec) = crate::store::with(|s| (s.capture_dir.clone(), s.record_dir.clone()));
    let main = crate::app::main_hwnd() as isize;
    let old = STOP.swap(0, Ordering::SeqCst);
    if old != 0 {
        unsafe { SetEvent(old as HANDLE) };
    }
    let generation = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let stop = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    STOP.store(stop as isize, Ordering::SeqCst);
    let stop = stop as isize;
    let _ = std::thread::Builder::new().name("watch".into()).stack_size(64 * 1024).spawn(move || run(vec![cap, rec], stop, main, generation));
}

fn run(dirs: Vec<PathBuf>, stop: isize, main: isize, generation: u64) {
    unsafe {
        let mut handles: Vec<HANDLE> = vec![stop as HANDLE];
        for d in &dirs {
            let h = FindFirstChangeNotificationW(wide_os(d.as_os_str()).as_ptr(), 0, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE);
            if h != INVALID_HANDLE_VALUE {
                handles.push(h);
            }
        }
        // También la carpeta oculta de metadatos: títulos, secciones y fijadas cambian ahí.
        for d in &dirs {
            for side in [".orig", ".meta"] {
                let p = d.join(side);
                if p.exists() {
                    let h = FindFirstChangeNotificationW(wide_os(p.as_os_str()).as_ptr(), 0, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE);
                    if h != INVALID_HANDLE_VALUE {
                        handles.push(h);
                    }
                }
            }
        }
        loop {
            let r = WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, INFINITE);
            if r == WAIT_OBJECT_0 || r >= WAIT_OBJECT_0 + handles.len() as u32 || GEN.load(Ordering::SeqCst) != generation {
                break;
            }
            // Agrupar ráfagas (copiar muchas capturas): un solo aviso.
            Sleep(250);
            PostMessageW(main as _, WM_LIBRARY_CHANGED, 0, 0);
            let idx = (r - WAIT_OBJECT_0) as usize;
            FindNextChangeNotification(handles[idx]);
        }
        for h in &handles[1..] {
            FindCloseChangeNotification(*h);
        }
        CloseHandle(stop as HANDLE);
    }
}
