//! Proceso residente: ventana oculta, ícono en la bandeja, atajos globales y cola de comandos.
//! En reposo no hay trabajo: el único hilo duerme en `GetMessageW`.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_NOSOUND, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::capture::Frame;
use crate::gui::host;
use crate::i18n::{f, t};
use crate::image::png_bytes;
use crate::output::{copy_files, copy_image};
use crate::store;
use crate::win::{copy_into, wide};

const WM_TRAY: u32 = WM_APP + 1;
const WM_CMD: u32 = WM_APP + 2;
pub const WM_SHOW_LIBRARY: u32 = WM_APP + 3;
pub const WM_RECORD_DONE: u32 = WM_APP + 4;
/// Ejecuta un comando del menú de la bandeja (wParam = id). Sirve para automatizar pruebas desde afuera.
pub const WM_MENU_CMD: u32 = WM_APP + 5;
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;
const TIMER_DELAY: usize = 7;
const TIMER_HOTKEY_RETRY: usize = 8;

const HK_CAPTURE: i32 = 1;
const HK_FULL: i32 = 2;
const HK_PICK: i32 = 3;
const HK_RECORD: i32 = 4;

/// Comandos que cualquier parte de la app (o una ventana durante su frame) le pide al núcleo.
/// Se ejecutan después, fuera del frame, así nunca se abren ventanas ni diálogos desde adentro de otro.
pub enum Cmd {
    CaptureRegion,
    CaptureFull,
    CaptureDelayed,
    RecordVideo,
    RecordAudio,
    /// Detener desde un atajo, la bandeja o un menú (la barra pasa a "guardando").
    StopRecording(bool),
    /// Lo pide la barra: detener de verdad y armar el archivo.
    FinishRecording(bool),
    QuickPick,
    ShowLibrary,
    OpenEditor(Option<String>),
    OpenExport(Vec<String>),
    OpenSettings,
    OpenPersonalize,
    CopyItems(Vec<String>),
    /// Copia y pega en la ventana que estaba activa (menú rápido).
    PasteInto(Vec<String>, isize),
    Toast(String, String),
    LibraryChanged,
    WindowClosed,
    /// Trabajo que no puede correr dentro de un frame (diálogos modales de Windows, abrir ventanas).
    Run(Box<dyn FnOnce()>),
    Quit,
}

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Region,
    Full,
}

struct State {
    hwnd: Cell<HWND>,
    icon: Cell<HICON>,
    taskbar_created: Cell<u32>,
    busy: Cell<bool>,
    queue: RefCell<VecDeque<Cmd>>,
    keys: RefCell<[String; 5]>, // texto de cada atajo registrado (índice = id)
    toast_item: RefCell<Option<String>>,
}

thread_local! {
    static S: State = State {
        hwnd: Cell::new(null_mut()),
        icon: Cell::new(null_mut()),
        taskbar_created: Cell::new(0),
        busy: Cell::new(false),
        queue: RefCell::new(VecDeque::new()),
        keys: RefCell::new(Default::default()),
        toast_item: RefCell::new(None),
    };
}

pub fn main_hwnd() -> HWND {
    S.with(|s| s.hwnd.get())
}

/// Trabajo que mandan otros hilos (descargas, OCR, IA) para correr en el hilo de la interfaz.
type Remote = Box<dyn FnOnce() + Send>;
static REMOTE: std::sync::Mutex<Vec<Remote>> = std::sync::Mutex::new(Vec::new());
static MAIN_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Desde cualquier hilo: corre `f` en el hilo de la interfaz, fuera de todo frame.
pub fn post_from_thread(f: impl FnOnce() + Send + 'static) {
    REMOTE.lock().unwrap().push(Box::new(f));
    let h = MAIN_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, WM_CMD, 0, 0) };
    }
}

/// Encola un comando y despierta al núcleo.
pub fn post(cmd: Cmd) {
    S.with(|s| s.queue.borrow_mut().push_back(cmd));
    unsafe {
        PostMessageW(main_hwnd(), WM_CMD, 0, 0);
    }
}

/// Texto del atajo de captura que quedó activo (para mostrar en menús y ayudas).
pub fn capture_key() -> String {
    S.with(|s| s.keys.borrow()[HK_CAPTURE as usize].clone())
}

pub fn key_text(id: i32) -> String {
    S.with(|s| s.keys.borrow().get(id as usize).cloned().unwrap_or_default())
}

pub fn run(start_hidden: bool) {
    unsafe {
        // El hilo de la interfaz es STA (lo exigen WebView2 y arrastrar a otras apps).
        windows_sys::Win32::System::Ole::OleInitialize(null());
        let start_hidden = start_hidden || crate::gui::settings::launched_at_startup();
        let hinst = GetModuleHandleW(null());
        let class = wide("BuzzPrintRsMain");
        let wc = WNDCLASSEXW { cbSize: size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: Some(wndproc), hInstance: hinst, lpszClassName: class.as_ptr(), ..zeroed() };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(0, class.as_ptr(), wide("BuzzPrint").as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null());
        if hwnd.is_null() {
            return;
        }
        // La bandeja muestra el logo tal como está en Personalizar (forma, colores y acento).
        let mut icon = crate::gui::markicon::current_icon(tray_icon_size());
        if icon.is_null() {
            icon = LoadImageW(hinst, 1usize as *const u16, IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), LR_DEFAULTCOLOR) as HICON;
        }
        if icon.is_null() {
            icon = LoadIconW(null_mut(), IDI_APPLICATION);
        }
        MAIN_HWND.store(hwnd as isize, std::sync::atomic::Ordering::SeqCst);
        S.with(|s| {
            s.hwnd.set(hwnd);
            s.icon.set(icon);
            s.taskbar_created.set(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()));
        });

        register_hotkeys(hwnd, true);
        tray_add(hwnd);
        crate::watch::restart();
        if start_hidden {
            let k = capture_key();
            notify(&t("BuzzPrint está activo"), &format!("{k} {} · {} {}", t("para capturar"), store::settings().hotkey_pick, t("para elegir una captura")), None);
        } else {
            post(Cmd::ShowLibrary);
        }
        // Primer arranque: la guía y (con permiso) ImprPant para BuzzPrint. Después, el OCR de lo que falte.
        post(Cmd::Run(Box::new(|| {
            crate::gui::guide::offer_once();
            offer_print_screen();
            crate::ocr::refresh();
        })));
        trim_memory();

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        for id in [HK_CAPTURE, HK_FULL, HK_PICK, HK_RECORD, 11] {
            UnregisterHotKey(hwnd, id);
        }
        let n = tray_data(hwnd);
        Shell_NotifyIconW(NIM_DELETE, &n);
    }
}

// ------------------------------------------------------------------ atajos

/// "Ctrl+Alt+V", "PrintScreen", "Ctrl+PrintScreen"… (mismo formato que .NET / WinForms.Keys).
pub fn parse_hotkey(spec: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut key = 0u32;
    for part in spec.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" => mods |= MOD_WIN,
            "printscreen" | "snapshot" | "prtsc" => key = VK_SNAPSHOT as u32,
            "pause" => key = VK_PAUSE as u32,
            "insert" => key = VK_INSERT as u32,
            "delete" => key = VK_DELETE as u32,
            "home" => key = VK_HOME as u32,
            "end" => key = VK_END as u32,
            "pageup" | "prior" => key = VK_PRIOR as u32,
            "pagedown" | "next" => key = VK_NEXT as u32,
            "space" => key = VK_SPACE as u32,
            k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() => key = k.to_ascii_uppercase().as_bytes()[0] as u32,
            k if k.len() == 2 && k.starts_with('d') && k.as_bytes()[1].is_ascii_digit() => key = k.as_bytes()[1] as u32,
            k if k.starts_with('f') && k[1..].parse::<u32>().is_ok_and(|n| (1..=24).contains(&n)) => key = VK_F1 as u32 + k[1..].parse::<u32>().unwrap() - 1,
            _ => return None,
        }
    }
    (key != 0).then_some((mods, key))
}

fn register(hwnd: HWND, id: i32, spec: &str) -> bool {
    unsafe {
        UnregisterHotKey(hwnd, id);
        match parse_hotkey(spec) {
            Some((m, k)) => RegisterHotKey(hwnd, id, m | MOD_NOREPEAT, k) != 0,
            None => false,
        }
    }
}

/// Windows 11 trae "Usar ImprPant para abrir la captura de pantalla" encendido: en ese caso ImprPant no
/// llega a ninguna app. Sin valor = el predeterminado de Windows (encendido desde Windows 11).
fn snipping_owns_print_screen() -> bool {
    let key = wide("Control Panel\\Keyboard");
    let name = wide("PrintScreenKeyForSnippingEnabled");
    let mut value: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, null_mut(), &mut value as *mut u32 as *mut _, &mut size) };
    if rc == 0 {
        return value != 0;
    }
    windows_build() >= 22000
}

/// Pregunta una vez, con permiso, si ImprPant pasa a BuzzPrint (el mismo ajuste por usuario que Configuración →
/// Accesibilidad → Teclado). Nunca se cambia sin preguntar. Comparte SnippingPromptAnswered con .NET.
fn offer_print_screen() {
    let st = store::settings();
    if st.snipping_prompt_answered || !st.hotkey_capture.eq_ignore_ascii_case("PrintScreen") || !snipping_owns_print_screen() {
        return;
    }
    let text = format!(
        "{}\n\n{}\n{}",
        t("Windows está usando la tecla ImprPant para abrir la Herramienta Recortes, así que no llega a BuzzPrint."),
        t("Ajuste: Configuración → Accesibilidad → Teclado → Usar ImprPant para abrir la captura de pantalla"),
        f("Atajo alternativo: {0} sigue funcionando", &[&st.hotkey_capture_alt])
    );
    fn answered() {
        store::with(|s| {
            s.settings.snipping_prompt_answered = true;
            s.save_settings();
        })
    }
    crate::gui::prompt::confirm_or(
        &t("ImprPant abre Recortes"),
        &text,
        &t("Usar ImprPant en BuzzPrint"),
        &t("Dejar así"),
        || {
            answered();
            let key = wide("Control Panel\\Keyboard");
            let name = wide("PrintScreenKeyForSnippingEnabled");
            let zero: u32 = 0;
            let rc = unsafe {
                windows_sys::Win32::System::Registry::RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), windows_sys::Win32::System::Registry::REG_DWORD, &zero as *const u32 as *const _, 4)
            };
            if rc != 0 {
                crate::gui::prompt::info(&t("ImprPant abre Recortes"), &t("No se pudo cambiar el ajuste. Desactivalo a mano en Configuración → Accesibilidad → Teclado."));
                return;
            }
            register_hotkeys(main_hwnd(), false);
            crate::gui::prompt::confirm(
                &t("ImprPant es de BuzzPrint"),
                &format!("{}\n\n{}", t("Listo. Windows puede seguir reteniendo la tecla hasta que cerrás sesión."), t("Para que tome efecto ya: abrí el ajuste, encendelo y apagalo una vez.")),
                &t("Abrir el ajuste"),
                &t("Más tarde"),
                || { crate::output::open_url("ms-settings:easeofaccess-keyboard"); },
            );
        },
        answered,
    );
}

fn windows_build() -> u32 {
    let key = wide("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
    let name = wide("CurrentBuildNumber");
    let mut buf = [0u16; 16];
    let mut size = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE, key.as_ptr(), name.as_ptr(), windows_sys::Win32::System::Registry::RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr() as *mut _, &mut size)
    };
    if rc != 0 {
        return 0;
    }
    String::from_utf16_lossy(&buf).trim_matches('\0').parse().unwrap_or(0)
}

/// Registra los atajos de la configuración. Lo que esté tomado se reintenta cada pocos segundos.
pub fn register_hotkeys(hwnd: HWND, announce: bool) {
    let st = store::settings();
    let mut keys: [String; 5] = Default::default();
    let mut fails: Vec<String> = Vec::new();
    let print_screen_first = !(st.hotkey_capture.eq_ignore_ascii_case("PrintScreen") && snipping_owns_print_screen());
    if print_screen_first && register(hwnd, HK_CAPTURE, &st.hotkey_capture) {
        keys[1] = st.hotkey_capture.clone();
    } else if register(hwnd, HK_CAPTURE, &st.hotkey_capture_alt) {
        keys[1] = st.hotkey_capture_alt.clone();
        fails.push(f("{0} está tomada (¿Windows la usa para la Herramienta Recortes?). Se usa {1}.", &[&st.hotkey_capture, &st.hotkey_capture_alt]));
    } else {
        fails.push(f("No se pudo registrar {0} ni {1}.", &[&st.hotkey_capture, &st.hotkey_capture_alt]));
    }
    for (id, spec) in [(HK_FULL, &st.hotkey_full), (HK_PICK, &st.hotkey_pick), (HK_RECORD, &st.hotkey_record)] {
        if register(hwnd, id, spec) {
            keys[id as usize] = spec.clone();
        } else {
            fails.push(f("No se pudo registrar {0}.", &[spec]));
        }
    }
    let missing = keys[1] != st.hotkey_capture || keys[2].is_empty() || keys[3].is_empty() || keys[4].is_empty();
    S.with(|s| *s.keys.borrow_mut() = keys);
    unsafe {
        if missing {
            SetTimer(hwnd, TIMER_HOTKEY_RETRY, 3000, None);
        } else {
            KillTimer(hwnd, TIMER_HOTKEY_RETRY);
        }
    }
    if announce && !fails.is_empty() {
        notify(&t("Atajos de teclado"), &fails.join("\n"), None);
    }
}

/// Reintento de atajos tomados por otra app (por ejemplo BuzzPrint .NET abierto al mismo tiempo).
fn retry_hotkeys(hwnd: HWND) {
    let st = store::settings();
    let mut keys = S.with(|s| s.keys.borrow().clone());
    let mut changed = false;
    if keys[1] != st.hotkey_capture && !(st.hotkey_capture.eq_ignore_ascii_case("PrintScreen") && snipping_owns_print_screen()) {
        if register(hwnd, 11, &st.hotkey_capture) {
            unsafe { UnregisterHotKey(hwnd, HK_CAPTURE) };
            keys[1] = st.hotkey_capture.clone();
            changed = true;
            notify(&t("Atajos de teclado"), &f("{0} quedó libre: ya captura con {0}.", &[&st.hotkey_capture]), None);
        }
    }
    for (id, spec) in [(HK_FULL, &st.hotkey_full), (HK_PICK, &st.hotkey_pick), (HK_RECORD, &st.hotkey_record)] {
        if keys[id as usize].is_empty() && register(hwnd, id, spec) {
            keys[id as usize] = spec.clone();
            changed = true;
        }
    }
    let done = keys[1] == st.hotkey_capture && !keys[2].is_empty() && !keys[3].is_empty() && !keys[4].is_empty();
    S.with(|s| *s.keys.borrow_mut() = keys);
    if done {
        unsafe { KillTimer(hwnd, TIMER_HOTKEY_RETRY) };
    }
    if changed {
        unsafe { tray_update_tip(hwnd) };
    }
}

// ------------------------------------------------------------------ ventana principal (oculta)

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            match wp as i32 {
                HK_CAPTURE | 11 => capture(Mode::Region),
                HK_FULL => capture(Mode::Full),
                HK_PICK => {
                    let fg = GetForegroundWindow();
                    crate::gui::quickpick::toggle(fg);
                }
                HK_RECORD => {
                    if crate::record::is_recording() {
                        post(Cmd::StopRecording(true));
                    } else {
                        post(Cmd::RecordVideo);
                    }
                }
                _ => {}
            }
            0
        }
        WM_TRAY => {
            match lp as u32 {
                WM_LBUTTONUP => {
                    if crate::record::is_recording() {
                        post(Cmd::StopRecording(true));
                    } else {
                        crate::gui::quickpick::toggle(null_mut());
                    }
                }
                WM_LBUTTONDBLCLK => post(Cmd::ShowLibrary),
                // Menú propio con el tema de la app (el de Windows salía blanco).
                WM_RBUTTONUP | WM_CONTEXTMENU => crate::gui::traymenu::toggle(),
                NIN_BALLOONUSERCLICK => {
                    if let Some(id) = S.with(|s| s.toast_item.borrow().clone()) {
                        match store::with(|s| s.find(&id)) {
                            Some(it) if it.is_recording() => crate::output::shell_open(&it.path),
                            _ => post(Cmd::OpenEditor(Some(id))),
                        }
                    }
                }
                _ => {}
            }
            0
        }
        WM_CMD => {
            run_queue();
            0
        }
        WM_RECORD_DONE => {
            recording_done(wp != 0);
            0
        }
        WM_MENU_CMD => {
            menu_command(wp as i32);
            0
        }
        WM_SHOW_LIBRARY => {
            post(Cmd::ShowLibrary);
            0
        }
        crate::watch::WM_LIBRARY_CHANGED => {
            store::with(|s| s.changed());
            host::wake_all();
            0
        }
        WM_TIMER if wp == TIMER_DELAY => {
            KillTimer(hwnd, TIMER_DELAY);
            capture(Mode::Region);
            0
        }
        WM_TIMER if wp == TIMER_HOTKEY_RETRY => {
            retry_hotkeys(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        m if m != 0 && m == S.with(|s| s.taskbar_created.get()) => {
            tray_add(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn run_queue() {
    let remote: Vec<Remote> = std::mem::take(&mut *REMOTE.lock().unwrap());
    for f in remote {
        f();
    }
    loop {
        let Some(cmd) = S.with(|s| s.queue.borrow_mut().pop_front()) else { break };
        match cmd {
            Cmd::CaptureRegion => capture(Mode::Region),
            Cmd::CaptureFull => capture(Mode::Full),
            Cmd::CaptureDelayed => {
                let secs = store::settings().delay_seconds.max(1);
                notify(&t("Captura con demora"), &f("Capturando en {0} s… abrí el menú que quieras capturar.", &[&secs]), None);
                unsafe { SetTimer(main_hwnd(), TIMER_DELAY, secs as u32 * 1000, None) };
            }
            Cmd::RecordVideo => record_video(),
            Cmd::RecordAudio => {
                if crate::record::ffmpeg().is_none() {
                    crate::deps::offer_ffmpeg(|| post(Cmd::RecordAudio));
                } else if !crate::record::is_recording() {
                    crate::gui::recbar::open(crate::record::Kind::Audio, windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 }, t("Solo audio"));
                }
            }
            Cmd::StopRecording(keep) => match host::find("recbar") {
                Some(h) => {
                    host::with_view::<crate::gui::recbar::RecBar, _>(h, |b| b.stop(keep));
                }
                None => crate::record::stop(keep),
            },
            Cmd::FinishRecording(keep) => crate::record::stop(keep),
            Cmd::QuickPick => crate::gui::quickpick::toggle(null_mut()),
            Cmd::ShowLibrary => crate::gui::library::show(),
            Cmd::OpenEditor(id) => crate::gui::editor::open(id),
            Cmd::OpenExport(ids) => crate::gui::export::open(ids),
            Cmd::OpenSettings => crate::gui::settings::open(false),
            Cmd::OpenPersonalize => crate::gui::settings::open(true),
            Cmd::CopyItems(ids) => copy_items(&ids),
            Cmd::PasteInto(ids, prev) => {
                copy_items(&ids);
                crate::output::paste_into(prev as HWND);
            }
            Cmd::Toast(title, text) => notify(&title, &text, None),
            Cmd::LibraryChanged => host::wake_all(),
            Cmd::Run(f) => f(),
            Cmd::WindowClosed => {
                trim_memory();
            }
            Cmd::Quit => unsafe {
                for h in host::open_kinds() {
                    windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(h);
                }
                DestroyWindow(main_hwnd());
            },
        }
    }
}

/// Copia capturas: una sola = imagen; varias = archivos. Grabaciones = archivo.
pub fn copy_items(ids: &[String]) {
    let items: Vec<store::Item> = ids.iter().filter_map(|id| store::with(|s| s.find(id))).collect();
    if items.is_empty() {
        return;
    }
    let owner = main_hwnd();
    if items.len() == 1 && !items[0].is_recording() {
        if let Ok(img) = crate::image::load_png(&items[0].path) {
            copy_image(owner, &img);
        }
    } else {
        copy_files(owner, &items.iter().map(|i| i.path.clone()).collect::<Vec<PathBuf>>());
    }
}

pub fn menu_command(cmd: i32) {
    match cmd {
        10 => capture(Mode::Region),
        11 => capture(Mode::Full),
        12 => post(Cmd::CaptureDelayed),
        20 => crate::gui::quickpick::toggle(null_mut()),
        21 => post(Cmd::ShowLibrary),
        22 => post(Cmd::OpenEditor(None)),
        23 => post(Cmd::OpenPersonalize),
        24 => post(Cmd::OpenSettings),
        30 => post(Cmd::StopRecording(true)),
        31 => post(Cmd::RecordVideo),
        32 => post(Cmd::RecordAudio),
        33..=36 => {
            crate::record::set_next_audio(cmd == 34 || cmd == 36, cmd == 35 || cmd == 36);
            post(Cmd::RecordVideo);
        }
        // Diagnóstico (solo por WM_MENU_CMD, no están en ningún menú): abrir cada ventana para revisarla.
        80 => post(Cmd::Run(Box::new(crate::gui::ai::open_settings))),
        81 => post(Cmd::Run(Box::new(|| crate::gui::guide::open(None)))),
        82 => post(Cmd::Run(Box::new(crate::gui::guide::about))),
        83 => post(Cmd::Run(Box::new(crate::gui::guide::components))),
        84 => post(Cmd::Run(Box::new(crate::gui::templates::open))),
        85 => post(Cmd::Run(Box::new(|| {
            let first = crate::store::with(|s| s.all().iter().find(|i| !i.is_recording()).map(|i| i.id.clone()));
            crate::gui::editor::open(first);
        }))),
        86 => post(Cmd::Run(Box::new(|| crate::gui::prompt::confirm("Confirmar", "¿Eliminar 3 capturas? Se mandan a la papelera.", "Eliminar", "Cancelar", || {})))),
        87 => post(Cmd::Run(Box::new(|| crate::gui::prompt::info("Aviso", "Texto de prueba de un aviso informativo.")))),
        88 => post(Cmd::Run(Box::new(|| {
            let ids: Vec<String> = crate::store::with(|s| s.all().iter().filter(|i| !i.is_recording()).take(4).map(|i| i.id.clone()).collect());
            crate::gui::export::open(ids);
        }))),
        89 => post(Cmd::Run(Box::new(|| {
            if let Some(it) = crate::store::with(|s| s.all().iter().find(|i| !i.is_recording()).cloned()) {
                crate::gui::ai::open_ask(it);
            }
        }))),
        90 => post(Cmd::Run(Box::new(|| crate::gui::ai::open_text("Título y resumen", "Resultado", "Título: Ejemplo\n\nResumen de prueba para revisar cómo se ve la ventana de resultado.".into())))),
        91 => post(Cmd::Run(Box::new(|| {
            let rows = vec![("Region_19_11_2026_18_28_01".to_string(), "Biblioteca con el panel de opciones".to_string()), ("19/11 17:35".to_string(), "Registro de errores de la consola".to_string())];
            crate::gui::ai::open_review("Ordenar la biblioteca", "Títulos sugeridos", rows, "Aplicar", Box::new(|_| {}));
        }))),
        92 => post(Cmd::Run(Box::new(|| {
            let p = crate::gui::prompt::progress("Exportando");
            p.report(3, Some(8));
            p.status("Armando la página 3 de 8…");
        }))),
        93 => post(Cmd::Run(Box::new(crate::gui::traymenu::toggle))),
        99 => post(Cmd::Quit),
        _ => {}
    }
}

// ------------------------------------------------------------------ captura

fn capture(mode: Mode) {
    if S.with(|s| s.busy.replace(true)) {
        return;
    }
    let result = run_capture(mode);
    S.with(|s| s.busy.set(false));
    match result {
        Ok(Some(item)) => after_capture(item),
        Ok(None) => {}
        Err(e) => notify(&t("Error al capturar: ").trim_end_matches([':', ' ']).to_string(), &e, None),
    }
    trim_memory();
}

fn run_capture(mode: Mode) -> Result<Option<store::Item>, String> {
    let frame = Frame::grab().ok_or_else(|| t("No se pudo leer la pantalla."))?;
    let (rect, source) = match mode {
        Mode::Full => (frame.full_rect(), t("Pantalla completa")),
        Mode::Region => match crate::overlay::select(&frame, false) {
            None => return Ok(None),
            Some(p) => (p.rect, p.source),
        },
    };
    let img = frame.crop_rgba(rect).ok_or_else(|| t("La región está vacía."))?;
    drop(frame); // liberar la pantalla completa antes de comprimir
    if store::settings().copy_after_capture {
        copy_image(main_hwnd(), &img);
    }
    let png = png_bytes(&img).map_err(|e| e.to_string())?;
    drop(img);
    let item = store::with(|s| s.add_png(&png, &source)).map_err(|e| e.to_string())?;
    Ok(Some(item))
}

fn after_capture(item: store::Item) {
    let st = store::settings();
    host::wake_all();
    crate::ocr::refresh();
    crate::gui::ai::auto_caption(item.clone());
    if st.open_editor_after_capture {
        post(Cmd::OpenEditor(Some(item.id)));
    } else {
        let title = if st.copy_after_capture { t("Captura copiada y guardada") } else { t("Captura guardada") };
        let text = format!("{} · {} {}", t("Clic acá para editarla"), key_text(HK_PICK), t("para elegir otra"));
        notify(&title, &text, Some(item.id));
    }
}

/// Video: elegir región, ventana o pantalla con el mismo selector (rótulo REC), y abrir la barra con la cuenta.
fn record_video() {
    if crate::record::is_recording() {
        post(Cmd::StopRecording(true));
        return;
    }
    if crate::record::ffmpeg().is_none() {
        crate::deps::offer_ffmpeg(|| post(Cmd::RecordVideo));
        return;
    }
    if S.with(|s| s.busy.replace(true)) {
        return;
    }
    let pick = Frame::grab().and_then(|frame| crate::overlay::select(&frame, true).map(|p| (p, frame.x, frame.y)));
    S.with(|s| s.busy.set(false));
    trim_memory();
    let Some((p, ox, oy)) = pick else {
        // Cancelado: el audio elegido en el menú no queda guardado para otra vez.
        crate::record::clear_next_audio();
        return;
    };
    let area = windows_sys::Win32::Foundation::RECT { left: p.rect.left + ox, top: p.rect.top + oy, right: p.rect.right + ox, bottom: p.rect.bottom + oy };
    if area.right - area.left < 16 || area.bottom - area.top < 16 {
        return;
    }
    crate::gui::recbar::open(crate::record::Kind::Video, area, p.source);
}

/// La grabación terminó de guardarse (o se descartó).
fn recording_done(keep: bool) {
    if let Some(h) = host::find("recbar") {
        host::close(h);
    }
    let result = crate::record::DONE.lock().unwrap().take();
    store::with(|s| s.changed());
    host::wake_all();
    if !keep {
        return;
    }
    match result {
        Some(Ok(id)) => {
            let Some(item) = store::with(|s| s.find(&id)) else { return };
            let copy = store::settings().copy_after_capture;
            if copy {
                copy_files(main_hwnd(), &[item.path.clone()]);
            }
            if let Some(h) = host::find("library") {
                let _ = h;
                crate::gui::library::show_recordings();
            }
            let title = if item.kind == store::Kind::Video { t("Video guardado") } else { t("Audio guardado") };
            let mut text = crate::record::clock_text(std::time::Duration::from_secs_f64(item.duration.max(0.0)));
            if copy {
                text.push_str(&format!(" · {}", t("copiado como archivo")));
            }
            text.push_str(&format!(" · {}", t("clic para abrirlo")));
            notify(&title, &text, Some(id));
        }
        Some(Err(e)) => notify(&t("No se pudo guardar la grabación"), &e, None),
        None => {}
    }
    trim_memory();
}

/// `buzzprint --capture-screen`: captura toda la pantalla a la biblioteca, sin interfaz, y sale.
pub fn capture_screen_headless() -> i32 {
    crate::i18n::set_language(&store::settings().language);
    match run_capture(Mode::Full) {
        Ok(Some(item)) => {
            println!("{}", item.path.display());
            0
        }
        Ok(None) => 1,
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

/// `buzzprint --selftest DIR`: exporta las últimas capturas a los 4 formatos (dos plantillas) y renderiza
/// anotaciones de muestra. Para soporte y pruebas; no toca la biblioteca.
pub fn selftest(dir: &std::path::Path) -> i32 {
    use crate::export::{self, templates, Format, Opts, Page};
    use crate::marks::{Argb, Mark, Tool, P};
    crate::i18n::set_language(&store::settings().language);
    let _ = std::fs::create_dir_all(dir);
    let items: Vec<store::Item> = store::with(|s| s.all().iter().take(3).cloned().collect());
    if items.is_empty() {
        eprintln!("biblioteca vacía");
        return 1;
    }
    let mut fails = 0;
    // Anotaciones.
    if let Ok(base) = crate::image::load_png(&items[0].path) {
        let (w, h) = (base.w as f32, base.h as f32);
        let p = |x: f32, y: f32| P { x: w * x, y: h * y };
        let red = Argb::from_rgb(230, 30, 40);
        let mut marks = vec![
            Mark { kind: Tool::Highlight, color: Argb::from_rgb(255, 214, 0), width: 4.0, pts: vec![p(0.05, 0.30), p(0.45, 0.36)], text: String::new(), font_size: 18.0, number: 0 },
            Mark { kind: Tool::Rect, color: red, width: 4.0, pts: vec![p(0.55, 0.25), p(0.92, 0.55)], text: String::new(), font_size: 18.0, number: 0 },
            Mark { kind: Tool::Arrow, color: red, width: 5.0, pts: vec![p(0.40, 0.70), p(0.54, 0.50)], text: String::new(), font_size: 18.0, number: 0 },
            Mark { kind: Tool::Step, color: red, width: 4.0, pts: vec![p(0.55, 0.25)], text: String::new(), font_size: 26.0, number: 1 },
            Mark { kind: Tool::Text, color: red, width: 4.0, pts: vec![p(0.20, 0.72)], text: "Revisar este valor".into(), font_size: 24.0, number: 0 },
            Mark { kind: Tool::Blur, color: Argb::from_rgb(0, 0, 0), width: 4.0, pts: vec![p(0.05, 0.80), p(0.30, 0.92)], text: String::new(), font_size: 18.0, number: 0 },
        ];
        marks.push(Mark { kind: Tool::Ellipse, color: Argb::from_rgb(20, 110, 230), width: 3.0, pts: vec![p(0.1, 0.1), p(0.3, 0.25)], text: String::new(), font_size: 18.0, number: 0 });
        let out = crate::marks::render(&base, &marks);
        match crate::image::save_png(&out, &dir.join("marks.png")) {
            Ok(()) => println!("OK marks.png"),
            Err(e) => {
                fails += 1;
                println!("FAIL marks: {e}");
            }
        }
        // Ida y vuelta del JSON de anotaciones.
        let json = crate::marks::marks_to_meta(&marks);
        let mut m = serde_json::Map::new();
        m.insert("shapes".into(), json);
        let back = crate::marks::marks_from_meta(&m);
        println!("{} marks json ({} -> {})", if back.len() == marks.len() { "OK" } else { "FAIL" }, marks.len(), back.len());
    }
    let pages: Vec<Page> = items.iter().map(|i| Page { title: i.display(), note: "Nota de prueba: texto debajo de la imagen.".into(), item: i.clone() }).collect();
    for tpl in templates::builtins() {
        let tid = tpl.id.clone();
        let o = Opts {
            title: format!("Prueba {tid}"),
            intro: "Introducción de prueba.".into(),
            cover: true,
            numbers: true,
            captions: true,
            meta: true,
            one_per_page: true,
            landscape: false,
            accent: tpl.accent_hex(&export::app_accent()),
            t: tpl.clone(),
        };
        let formats: Vec<Format> = match tpl.format.as_str() {
            "pptx" => vec![Format::Pptx],
            "docx" => vec![Format::Docx],
            _ => vec![Format::Pdf, Format::Png],
        };
        let o = Opts { t: tpl, ..o };
        for f in formats {
            let path = dir.join(format!("{tid}.{}", f.ext()));
            let t0 = std::time::Instant::now();
            match export::write(&path, f, &pages, &o) {
                Ok(()) => println!("OK {} ({} ms, {} KB)", path.display(), t0.elapsed().as_millis(), std::fs::metadata(&path).map(|m| m.len() / 1024).unwrap_or(0)),
                Err(e) => {
                    fails += 1;
                    println!("FAIL {}: {e}", path.display());
                }
            }
        }
    }
    fails
}

/// Devuelve al sistema las páginas que ya no se usan: en reposo el proceso queda mínimo.
pub fn trim_memory() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

// ------------------------------------------------------------------ bandeja

unsafe fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n: NOTIFYICONDATAW = zeroed();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n
}

fn tip() -> String {
    let k = capture_key();
    if k.is_empty() { "BuzzPrint".into() } else { format!("BuzzPrint · {k} {}", t("para capturar")) }
}

unsafe fn tray_add(hwnd: HWND) {
    let mut n = tray_data(hwnd);
    n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    n.uCallbackMessage = WM_TRAY;
    n.hIcon = S.with(|s| s.icon.get());
    copy_into(&mut n.szTip, &tip());
    Shell_NotifyIconW(NIM_ADD, &n);
}

fn tray_icon_size() -> i32 {
    unsafe {
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForSystem().max(96) as i32;
        windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXSMICON, dpi as u32).max(16)
    }
}

/// Vuelve a dibujar el ícono de la bandeja y el de las ventanas abiertas (cambió el logo o el acento).
pub fn refresh_icons() {
    unsafe {
        let hwnd = main_hwnd();
        let icon = crate::gui::markicon::current_icon(tray_icon_size());
        if icon.is_null() {
            return;
        }
        let old = S.with(|s| s.icon.replace(icon));
        let mut n = tray_data(hwnd);
        n.uFlags = NIF_ICON;
        n.hIcon = icon;
        Shell_NotifyIconW(NIM_MODIFY, &n);
        if !old.is_null() {
            DestroyIcon(old);
        }
    }
    crate::gui::host::refresh_icons();
}

unsafe fn tray_update_tip(hwnd: HWND) {
    let mut n = tray_data(hwnd);
    n.uFlags = NIF_TIP;
    copy_into(&mut n.szTip, &tip());
    Shell_NotifyIconW(NIM_MODIFY, &n);
}

/// Aviso nativo de Windows, sin sonido. `item`: clic en el aviso = abrir esa captura en el editor.
pub fn notify(title: &str, text: &str, item: Option<String>) {
    S.with(|s| *s.toast_item.borrow_mut() = item);
    unsafe {
        let mut n = tray_data(main_hwnd());
        n.uFlags = NIF_INFO;
        n.dwInfoFlags = NIIF_USER | NIIF_NOSOUND;
        n.hBalloonIcon = S.with(|s| s.icon.get());
        copy_into(&mut n.szInfoTitle, title);
        copy_into(&mut n.szInfo, if text.is_empty() { " " } else { text });
        Shell_NotifyIconW(NIM_MODIFY, &n);
    }
}
