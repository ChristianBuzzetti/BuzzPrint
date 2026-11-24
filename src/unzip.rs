//! Lector ZIP mínimo: busca entradas en el directorio central y las extrae (guardadas o deflate). Alcanza para
//! sacar `ffmpeg.exe` del paquete oficial y para importar plantillas.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub struct Entry {
    pub name: String,
    method: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    offset: u64,
}

pub struct Zip {
    file: File,
    pub entries: Vec<Entry>,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

impl Zip {
    pub fn open(path: &Path) -> Result<Zip, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let len = file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        let tail_len = len.min(66_000 + 22);
        file.seek(SeekFrom::Start(len - tail_len)).map_err(|e| e.to_string())?;
        let mut tail = vec![0u8; tail_len as usize];
        file.read_exact(&mut tail).map_err(|e| e.to_string())?;
        let eocd = (0..tail.len().saturating_sub(21)).rev().find(|&i| u32_at(&tail, i) == 0x0605_4b50).ok_or("ZIP dañado")?;
        let mut count = u16_at(&tail, eocd + 10) as u64;
        let mut cd_size = u32_at(&tail, eocd + 12) as u64;
        let mut cd_off = u32_at(&tail, eocd + 16) as u64;
        // ZIP64: el localizador está justo antes del fin de directorio.
        if (cd_off == 0xFFFF_FFFF || count == 0xFFFF) && eocd >= 20 && u32_at(&tail, eocd - 20) == 0x0706_4b50 {
            let rec = u64_at(&tail, eocd - 20 + 8);
            let mut z = [0u8; 56];
            file.seek(SeekFrom::Start(rec)).map_err(|e| e.to_string())?;
            file.read_exact(&mut z).map_err(|e| e.to_string())?;
            count = u64_at(&z, 32);
            cd_size = u64_at(&z, 40);
            cd_off = u64_at(&z, 48);
        }
        let mut cd = vec![0u8; cd_size as usize];
        file.seek(SeekFrom::Start(cd_off)).map_err(|e| e.to_string())?;
        file.read_exact(&mut cd).map_err(|e| e.to_string())?;
        let mut entries = Vec::with_capacity(count as usize);
        let mut p = 0usize;
        while p + 46 <= cd.len() && u32_at(&cd, p) == 0x0201_4b50 {
            let method = u16_at(&cd, p + 10);
            let crc = u32_at(&cd, p + 16);
            let mut compressed = u32_at(&cd, p + 20) as u64;
            let mut size = u32_at(&cd, p + 24) as u64;
            let n = u16_at(&cd, p + 28) as usize;
            let x = u16_at(&cd, p + 30) as usize;
            let k = u16_at(&cd, p + 32) as usize;
            let mut offset = u32_at(&cd, p + 42) as u64;
            let name = String::from_utf8_lossy(&cd[p + 46..p + 46 + n]).into_owned();
            // Campo extra ZIP64 (0x0001) con los valores de 64 bits que no entraron.
            let mut e = p + 46 + n;
            let end = e + x;
            while e + 4 <= end {
                let (id, sz) = (u16_at(&cd, e), u16_at(&cd, e + 2) as usize);
                if id == 1 {
                    let mut q = e + 4;
                    if size == 0xFFFF_FFFF {
                        size = u64_at(&cd, q);
                        q += 8;
                    }
                    if compressed == 0xFFFF_FFFF {
                        compressed = u64_at(&cd, q);
                        q += 8;
                    }
                    if offset == 0xFFFF_FFFF {
                        offset = u64_at(&cd, q);
                    }
                }
                e += 4 + sz;
            }
            entries.push(Entry { name, method, crc, compressed, size, offset });
            p += 46 + n + x + k;
        }
        Ok(Zip { file, entries })
    }

    pub fn find(&self, pred: impl Fn(&str) -> bool) -> Option<usize> {
        self.entries.iter().position(|e| pred(&e.name))
    }

    pub fn read(&mut self, index: usize) -> Result<Vec<u8>, String> {
        let e = &self.entries[index];
        let mut head = [0u8; 30];
        self.file.seek(SeekFrom::Start(e.offset)).map_err(|e| e.to_string())?;
        self.file.read_exact(&mut head).map_err(|e| e.to_string())?;
        if u32_at(&head, 0) != 0x0403_4b50 {
            return Err("ZIP dañado".into());
        }
        let skip = u16_at(&head, 26) as i64 + u16_at(&head, 28) as i64;
        self.file.seek(SeekFrom::Current(skip)).map_err(|e| e.to_string())?;
        let mut data = vec![0u8; e.compressed as usize];
        self.file.read_exact(&mut data).map_err(|e| e.to_string())?;
        let out = match e.method {
            0 => data,
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&data, e.size as usize + 1).map_err(|_| "No se pudo descomprimir".to_string())?,
            m => return Err(format!("Compresión no soportada ({m})")),
        };
        if out.len() as u64 != e.size || crc32fast::hash(&out) != e.crc {
            return Err("El archivo extraído no coincide (CRC)".into());
        }
        Ok(out)
    }
}
