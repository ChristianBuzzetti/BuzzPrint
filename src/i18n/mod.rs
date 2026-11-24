//! Idioma de la interfaz. Igual que la versión .NET: el texto fuente está en español y el inglés sale
//! de una tabla. El idioma se elige una vez al arrancar.

mod en;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static SPANISH: AtomicBool = AtomicBool::new(false);
static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

pub fn set_language(lang: &str) {
    SPANISH.store(lang == "es", Ordering::Relaxed);
}

pub fn es() -> bool {
    SPANISH.load(Ordering::Relaxed)
}

/// Traduce un texto en español al idioma actual. Si no hay traducción, queda en español.
pub fn t(es_text: &str) -> String {
    if es() {
        return es_text.to_string();
    }
    let map = TABLE.get_or_init(|| en::MAP.iter().copied().collect());
    map.get(es_text).map(|s| s.to_string()).unwrap_or_else(|| es_text.to_string())
}

/// `t` con argumentos `{0}`, `{1}`… como `string.Format` de .NET.
pub fn f(es_text: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut s = t(es_text);
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}

/// "1 captura" / "3 capturas".
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", t(if n == 1 { one } else { many }))
}
