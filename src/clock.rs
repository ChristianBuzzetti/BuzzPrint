//! Fechas locales sin dependencias: lo justo para nombrar capturas, agrupar por día y filtrar.

use std::mem::zeroed;

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub milli: u32,
}

impl Stamp {
    pub fn now() -> Stamp {
        unsafe {
            let mut t: SYSTEMTIME = zeroed();
            GetLocalTime(&mut t);
            from_systemtime(&t)
        }
    }

    /// Hora local de un `SystemTime` de archivo.
    pub fn from_system(t: std::time::SystemTime) -> Stamp {
        // 100 ns desde 1601-01-01 (FILETIME).
        let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let ticks = d.as_nanos() / 100 + 116_444_736_000_000_000u128;
        let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
        unsafe {
            let mut utc: SYSTEMTIME = zeroed();
            let mut local: SYSTEMTIME = zeroed();
            if FileTimeToSystemTime(&ft, &mut utc) == 0 {
                return Stamp::default();
            }
            if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0 {
                local = utc;
            }
            from_systemtime(&local)
        }
    }

    /// `yyyyMMdd_HHmmss_fff`: el id de una captura en la biblioteca.
    pub fn id(&self) -> String {
        format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}_{:03}",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.milli
        )
    }

    pub fn parse_id(id: &str) -> Option<Stamp> {
        let b = id.as_bytes();
        if b.len() < 19 || b[8] != b'_' || b[15] != b'_' {
            return None;
        }
        let n = |r: std::ops::Range<usize>| id.get(r)?.parse::<u32>().ok();
        let s = Stamp {
            year: n(0..4)? as i32,
            month: n(4..6)?,
            day: n(6..8)?,
            hour: n(9..11)?,
            minute: n(11..13)?,
            second: n(13..15)?,
            milli: n(16..19)?,
        };
        (s.month >= 1 && s.month <= 12 && s.day >= 1 && s.day <= 31).then_some(s)
    }

    /// `dd_MM_yyyy_HH_mm_ss` del nombre por defecto.
    pub fn name_part(&self) -> String {
        format!(
            "{:02}_{:02}_{:04}_{:02}_{:02}_{:02}",
            self.day, self.month, self.year, self.hour, self.minute, self.second
        )
    }

    pub fn short(&self) -> String {
        format!("{:02}/{:02} {:02}:{:02}", self.day, self.month, self.hour, self.minute)
    }

    pub fn long(&self) -> String {
        format!(
            "{:02}/{:02}/{:04} {:02}:{:02}:{:02}",
            self.day, self.month, self.year, self.hour, self.minute, self.second
        )
    }

    pub fn date_text(&self) -> String {
        format!("{:02}/{:02}/{:04}", self.day, self.month, self.year)
    }

    /// Días desde 1970-01-01 (para comparar fechas).
    pub fn day_number(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    pub fn weekday(&self) -> u32 {
        // 1970-01-01 fue jueves (4). 0 = domingo.
        ((self.day_number() + 4).rem_euclid(7)) as u32
    }
}

fn from_systemtime(t: &SYSTEMTIME) -> Stamp {
    Stamp {
        year: t.wYear as i32,
        month: t.wMonth as u32,
        day: t.wDay as u32,
        hour: t.wHour as u32,
        minute: t.wMinute as u32,
        second: t.wSecond as u32,
        milli: t.wMilliseconds as u32,
    }
}

/// Algoritmo de Howard Hinnant.
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

pub fn month_name(m: u32) -> String {
    const ES: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
    const EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let i = (m.clamp(1, 12) - 1) as usize;
    if crate::i18n::es() { ES[i].into() } else { EN[i].into() }
}

pub fn weekday_name(w: u32) -> String {
    const ES: [&str; 7] = ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"];
    const EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let i = (w % 7) as usize;
    if crate::i18n::es() { ES[i].into() } else { EN[i].into() }
}

/// "hoy", "ayer", "lunes 17 de noviembre"…
pub fn day_label(day: i64, today: i64) -> String {
    use crate::i18n::t;
    if day == today {
        return t("Hoy");
    }
    if day == today - 1 {
        return t("Ayer");
    }
    let (y, m, d) = civil_from_days(day);
    let w = ((day + 4).rem_euclid(7)) as u32;
    let (ty, _, _) = civil_from_days(today);
    let year = if y != ty { format!(" {y}") } else { String::new() };
    if crate::i18n::es() {
        format!("{} {d} de {}{year}", weekday_name(w), month_name(m))
    } else {
        format!("{} {} {d}{year}", weekday_name(w), month_name(m))
    }
}
