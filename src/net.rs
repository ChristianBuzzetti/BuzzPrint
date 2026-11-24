//! HTTP(S) con WinHTTP: usa los certificados de Windows y el proxy del sistema (con las credenciales del usuario
//! en redes corporativas), sin TLS propio ni dependencias. Lo usan la descarga de FFmpeg y las conexiones de IA.

use std::ffi::c_void;
use std::io::Write;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Networking::WinHttp::*;

use crate::win::wide;

const ACCESS_AUTOMATIC_PROXY: u32 = 4;
const FLAG_SECURE: u32 = 0x0080_0000;
const QUERY_STATUS_CODE: u32 = 19;
const QUERY_CONTENT_LENGTH: u32 = 5;
const QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
const OPTION_AUTOLOGON_POLICY: u32 = 77;
const OPTION_DECOMPRESSION: u32 = 118;
const AUTH_TARGET_PROXY: u32 = 1;
const AUTH_TARGET_SERVER: u32 = 0;

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn last_error(what: &str) -> String {
    let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    let text = match code {
        12002 => "se agotó el tiempo de espera".to_string(),
        12007 => "no se encontró el servidor".to_string(),
        12029 | 12030 => "no se pudo conectar".to_string(),
        12175 | 12045 | 12044 => "error de certificado".to_string(),
        12017 => "cancelado".to_string(),
        _ => format!("error {code}"),
    };
    format!("{what}: {text}")
}

pub struct Request<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: Option<&'a [u8]>,
    /// Tope sin recibir nada (conectar, enviar, esperar respuesta, leer), en milisegundos.
    pub timeout_ms: i32,
}

impl<'a> Request<'a> {
    pub fn get(url: &'a str) -> Request<'a> {
        Request { method: "GET", url, headers: Vec::new(), body: None, timeout_ms: 60_000 }
    }
    pub fn post_json(url: &'a str, body: &'a [u8]) -> Request<'a> {
        Request { method: "POST", url, headers: vec![("Content-Type".into(), "application/json".into())], body: Some(body), timeout_ms: 120_000 }
    }
    pub fn header(mut self, k: &str, v: impl Into<String>) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
}

/// Envía el pedido y entrega el cuerpo de a pedazos a `sink`. Devuelve el código HTTP.
pub fn send(req: &Request, cancel: Option<&AtomicBool>, sink: &mut dyn FnMut(&[u8]) -> Result<(), String>, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<u16, String> {
    let cancelled = || cancel.is_some_and(|c| c.load(Ordering::SeqCst));
    unsafe {
        let agent = wide(&format!("BuzzPrint/{}", env!("CARGO_PKG_VERSION")));
        let session = Handle(WinHttpOpen(agent.as_ptr(), ACCESS_AUTOMATIC_PROXY, null(), null(), 0));
        if session.0.is_null() {
            return Err(last_error("WinHTTP"));
        }
        let t = req.timeout_ms;
        WinHttpSetTimeouts(session.0, t, t, t, t);

        // Partes de la URL.
        let url_w = wide(req.url);
        let mut host = vec![0u16; 512];
        let mut path = vec![0u16; 4096];
        let mut uc: URL_COMPONENTS = std::mem::zeroed();
        uc.dwStructSize = std::mem::size_of::<URL_COMPONENTS>() as u32;
        uc.lpszHostName = host.as_mut_ptr();
        uc.dwHostNameLength = host.len() as u32;
        uc.lpszUrlPath = path.as_mut_ptr();
        uc.dwUrlPathLength = path.len() as u32;
        let mut extra = vec![0u16; 8192];
        uc.lpszExtraInfo = extra.as_mut_ptr();
        uc.dwExtraInfoLength = extra.len() as u32;
        if WinHttpCrackUrl(url_w.as_ptr(), 0, 0, &mut uc) == 0 {
            return Err(format!("URL no válida: {}", req.url));
        }
        let host_s = String::from_utf16_lossy(&host[..uc.dwHostNameLength as usize]);
        let mut object = String::from_utf16_lossy(&path[..uc.dwUrlPathLength as usize]);
        object.push_str(&String::from_utf16_lossy(&extra[..uc.dwExtraInfoLength as usize]));
        if object.is_empty() {
            object.push('/');
        }
        let secure = uc.nScheme == 2; // INTERNET_SCHEME_HTTPS

        let connect = Handle(WinHttpConnect(session.0, wide(&host_s).as_ptr(), uc.nPort, 0));
        if connect.0.is_null() {
            return Err(last_error(&host_s));
        }
        let verb = wide(req.method);
        let obj = wide(&object);
        let request = Handle(WinHttpOpenRequest(connect.0, verb.as_ptr(), obj.as_ptr(), null(), null(), null(), if secure { FLAG_SECURE } else { 0 }));
        if request.0.is_null() {
            return Err(last_error(&host_s));
        }
        // Credenciales de Windows para el proxy de la empresa, y respuestas comprimidas.
        let low: u32 = 0;
        WinHttpSetOption(request.0, OPTION_AUTOLOGON_POLICY, &low as *const u32 as *const c_void, 4);
        let decompress: u32 = 3;
        WinHttpSetOption(request.0, OPTION_DECOMPRESSION, &decompress as *const u32 as *const c_void, 4);

        let mut headers = String::new();
        for (k, v) in &req.headers {
            headers.push_str(&format!("{k}: {v}\r\n"));
        }
        let headers_w: Vec<u16> = headers.encode_utf16().collect();
        let body = req.body.unwrap_or(&[]);

        let mut status: u32 = 0;
        for attempt in 0..3 {
            if cancelled() {
                return Err("cancelado".into());
            }
            let ok = WinHttpSendRequest(
                request.0,
                if headers_w.is_empty() { null() } else { headers_w.as_ptr() },
                headers_w.len() as u32,
                if body.is_empty() { null() } else { body.as_ptr() as *const c_void },
                body.len() as u32,
                body.len() as u32,
                0,
            );
            if ok == 0 || WinHttpReceiveResponse(request.0, null_mut()) == 0 {
                return Err(last_error(&host_s));
            }
            let mut size = 4u32;
            status = 0;
            WinHttpQueryHeaders(request.0, QUERY_STATUS_CODE | QUERY_FLAG_NUMBER, null(), &mut status as *mut u32 as *mut c_void, &mut size, null_mut());
            // Proxy o servidor que piden autenticación integrada: se responde con las credenciales del usuario.
            if (status == 407 || status == 401) && attempt < 2 {
                let (mut supported, mut first, mut target) = (0u32, 0u32, 0u32);
                if WinHttpQueryAuthSchemes(request.0, &mut supported, &mut first, &mut target) != 0 {
                    let scheme = [0x10u32, 0x2, 0x8, 0x1].into_iter().find(|s| supported & s != 0).unwrap_or(first);
                    let tgt = if status == 407 { AUTH_TARGET_PROXY } else { AUTH_TARGET_SERVER };
                    if WinHttpSetCredentials(request.0, tgt, scheme, null(), null(), null_mut()) != 0 {
                        continue;
                    }
                }
            }
            break;
        }

        let mut total: u32 = 0;
        let mut size = 4u32;
        let has_len = WinHttpQueryHeaders(request.0, QUERY_CONTENT_LENGTH | QUERY_FLAG_NUMBER, null(), &mut total as *mut u32 as *mut c_void, &mut size, null_mut()) != 0;
        let total = has_len.then_some(total as u64);
        let mut buf = vec![0u8; 64 * 1024];
        let mut done: u64 = 0;
        loop {
            if cancelled() {
                return Err("cancelado".into());
            }
            let mut read = 0u32;
            if WinHttpReadData(request.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read) == 0 {
                return Err(last_error(&host_s));
            }
            if read == 0 {
                break;
            }
            sink(&buf[..read as usize])?;
            done += read as u64;
            progress(done, total);
        }
        Ok(status as u16)
    }
}

/// Pedido completo en memoria. Devuelve (código, cuerpo).
pub fn fetch(req: &Request, cancel: Option<&AtomicBool>) -> Result<(u16, Vec<u8>), String> {
    let mut body = Vec::new();
    let status = send(req, cancel, &mut |b| {
        body.extend_from_slice(b);
        Ok(())
    }, &mut |_, _| {})?;
    Ok((status, body))
}

pub fn get_text(url: &str, cancel: Option<&AtomicBool>) -> Result<String, String> {
    let (status, body) = fetch(&Request::get(url), cancel)?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Descarga a un archivo, informando (hecho, total).
pub fn download(url: &str, path: &Path, cancel: Option<&AtomicBool>, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::with_capacity(1 << 20, file);
    let mut req = Request::get(url);
    req.timeout_ms = 60_000;
    let status = send(&req, cancel, &mut |b| out.write_all(b).map_err(|e| e.to_string()), progress)?;
    out.flush().map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    Ok(())
}
