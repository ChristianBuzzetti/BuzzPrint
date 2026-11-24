//! Miniaturas: se decodifican en dos hilos de fondo y se suben a la GPU de la ventana que las pide.
//! Con tope LRU de texturas, caché de mtime invalidado solo cuando cambia `store.revision` (cero `stat()`
//! por cuadro en reposo) y `clear()` para soltar las texturas al minimizar.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use egui::{ColorImage, TextureHandle, TextureId, TextureOptions, Vec2};
use windows_sys::Win32::Foundation::HWND;

use crate::image::{load_png, Rgba};

struct Job {
    key: String,
    path: PathBuf,
    max: u32,
}

struct Done {
    key: String,
    img: Option<Rgba>,
}

pub struct Thumbs {
    tx: Option<Sender<Job>>,
    rx: Receiver<Done>,
    ready: HashMap<String, (TextureHandle, Vec2, u64)>,
    failed: HashSet<String>,
    pending: HashSet<String>,
    mtimes: HashMap<PathBuf, u128>,
    store_rev: u64,
    tick: u64,
    max_entries: usize,
    /// `tick` al empezar el cuadro actual: lo usado desde ahí está a la vista y no se expulsa.
    frame_tick: u64,
    frame_nr: u64,
}

impl Thumbs {
    pub fn new(hwnd: HWND) -> Thumbs {
        Self::with_cap(hwnd, 64)
    }

    pub fn with_cap(hwnd: HWND, max_entries: usize) -> Thumbs {
        let (tx, jobs) = channel::<Job>();
        let (done_tx, rx) = channel::<Done>();
        let jobs = Arc::new(Mutex::new(jobs));
        let hwnd = hwnd as usize;
        for _ in 0..2 {
            let jobs = jobs.clone();
            let done = done_tx.clone();
            std::thread::Builder::new()
                .name("thumbs".into())
                .stack_size(256 * 1024)
                .spawn(move || loop {
                    let job = match jobs.lock().map(|r| r.recv()) {
                        Ok(Ok(j)) => j,
                        _ => return, // la ventana se cerró
                    };
                    let img = load_png(&job.path).ok().map(|i| i.fit(job.max, job.max));
                    if done.send(Done { key: job.key, img }).is_err() {
                        return;
                    }
                    crate::gui::host::wake(hwnd as HWND);
                })
                .ok();
        }
        Thumbs {
            tx: Some(tx),
            rx,
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashSet::new(),
            mtimes: HashMap::new(),
            store_rev: u64::MAX,
            tick: 0,
            max_entries: max_entries.max(4),
            frame_tick: 0,
            frame_nr: u64::MAX,
        }
    }

    fn evict_if_needed(&mut self) {
        // Si todo lo que hay está a la vista (más miniaturas que el tope, p. ej. el editor maximizado), el tope
        // se estira: expulsar algo visible hacía que se recargara en el cuadro siguiente y parpadeara.
        while self.ready.len() > self.max_entries {
            let frame_tick = self.frame_tick;
            let Some(oldest) = self.ready.iter().filter(|(_, (_, _, used))| *used < frame_tick).min_by_key(|(_, (_, _, used))| *used).map(|(k, _)| k.clone()) else {
                break;
            };
            self.ready.remove(&oldest);
        }
    }

    fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(d) = self.rx.try_recv() {
            self.pending.remove(&d.key);
            match d.img {
                Some(img) => {
                    self.tick = self.tick.wrapping_add(1);
                    let size = Vec2::new(img.w as f32, img.h as f32);
                    let ci = ColorImage::from_rgba_unmultiplied([img.w as usize, img.h as usize], &img.px);
                    let tex = ctx.load_texture(&d.key, ci, TextureOptions::LINEAR);
                    self.ready.insert(d.key, (tex, size, self.tick));
                    self.evict_if_needed();
                }
                None => {
                    self.failed.insert(d.key);
                }
            }
        }
    }

    /// Invalida la caché de fechas solo cuando cambió la biblioteca (evita `fs::metadata` por cuadro).
    pub fn sync_revision(&mut self, rev: u64) {
        if rev != self.store_rev {
            self.store_rev = rev;
            self.mtimes.clear();
            self.failed.clear();
        }
    }

    fn mtime_of(&mut self, path: &Path) -> u128 {
        if let Some(&m) = self.mtimes.get(path) {
            return m;
        }
        let m = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        self.mtimes.insert(path.to_path_buf(), m);
        m
    }

    /// Miniatura de `path` (con la fecha de modificación en la clave, así se refresca tras editar).
    pub fn get(&mut self, ctx: &egui::Context, path: &Path, max: u32) -> Option<(TextureId, Vec2)> {
        let rev = crate::store::with(|s| s.revision);
        self.sync_revision(rev);
        let nr = ctx.cumulative_pass_nr();
        if nr != self.frame_nr {
            self.frame_nr = nr;
            self.frame_tick = self.tick.wrapping_add(1);
            // Lo que quedó de más del cuadro anterior ya no está a la vista: ahora sí se puede soltar.
            self.evict_if_needed();
        }
        self.pump(ctx);
        let mtime = self.mtime_of(path);
        let key = format!("{}@{max}@{mtime}", path.display());
        if let Some((t, s, used)) = self.ready.get_mut(&key) {
            self.tick = self.tick.wrapping_add(1);
            *used = self.tick;
            return Some((t.id(), *s));
        }
        if self.failed.contains(&key) || self.pending.contains(&key) {
            return None;
        }
        if let Some(tx) = &self.tx {
            self.pending.insert(key.clone());
            let _ = tx.send(Job { key, path: path.to_path_buf(), max });
        }
        None
    }

    /// Suelta todas las texturas GPU y cachés (al minimizar o recargar).
    pub fn clear(&mut self) {
        self.ready.clear();
        self.failed.clear();
        self.pending.clear();
        self.mtimes.clear();
        while self.rx.try_recv().is_ok() {}
    }

    /// Olvida las miniaturas (por ejemplo tras borrar capturas).
    pub fn forget_all(&mut self) {
        self.clear();
    }
}

impl Drop for Thumbs {
    fn drop(&mut self) {
        self.tx = None; // los hilos terminan solos
    }
}
