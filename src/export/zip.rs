//! Escritor ZIP mínimo (PPTX y DOCX son ZIP): deflate para XML, sin comprimir para PNG.

use crate::clock::Stamp;

pub struct Zip {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
    time: u16,
    date: u16,
}

impl Zip {
    pub fn new() -> Zip {
        let s = Stamp::now();
        let time = ((s.hour << 11) | (s.minute << 5) | (s.second / 2)) as u16;
        let date = ((((s.year - 1980).max(0) as u32) << 9) | (s.month << 5) | s.day) as u16;
        Zip { out: Vec::new(), central: Vec::new(), count: 0, time, date }
    }

    /// XML con su encabezado, comprimido.
    pub fn xml(&mut self, name: &str, body: &str) {
        let mut s = String::with_capacity(body.len() + 60);
        s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        s.push_str(body);
        self.add(name, s.as_bytes(), true);
    }

    pub fn add(&mut self, name: &str, data: &[u8], compress: bool) {
        let crc = crc32fast::hash(data);
        let (method, body) = if compress {
            (8u16, miniz_oxide::deflate::compress_to_vec(data, 6))
        } else {
            (0u16, data.to_vec())
        };
        let offset = self.out.len() as u32;
        let n = name.as_bytes();
        let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
        let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());

        // Encabezado local.
        le32(&mut self.out, 0x04034b50);
        le16(&mut self.out, 20);
        le16(&mut self.out, 0x0800); // nombres UTF-8
        le16(&mut self.out, method);
        le16(&mut self.out, self.time);
        le16(&mut self.out, self.date);
        le32(&mut self.out, crc);
        le32(&mut self.out, body.len() as u32);
        le32(&mut self.out, data.len() as u32);
        le16(&mut self.out, n.len() as u16);
        le16(&mut self.out, 0);
        self.out.extend_from_slice(n);
        self.out.extend_from_slice(&body);

        // Directorio central.
        let c = &mut self.central;
        le32(c, 0x02014b50);
        le16(c, 20);
        le16(c, 20);
        le16(c, 0x0800);
        le16(c, method);
        le16(c, self.time);
        le16(c, self.date);
        le32(c, crc);
        le32(c, body.len() as u32);
        le32(c, data.len() as u32);
        le16(c, n.len() as u16);
        le16(c, 0);
        le16(c, 0);
        le16(c, 0);
        le16(c, 0);
        le32(c, 0);
        le32(c, offset);
        c.extend_from_slice(n);
        self.count += 1;
    }

    pub fn finish(mut self) -> Vec<u8> {
        let start = self.out.len() as u32;
        let size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        let o = &mut self.out;
        o.extend_from_slice(&0x06054b50u32.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        o.extend_from_slice(&self.count.to_le_bytes());
        o.extend_from_slice(&self.count.to_le_bytes());
        o.extend_from_slice(&size.to_le_bytes());
        o.extend_from_slice(&start.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}
