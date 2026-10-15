//! Interface language: Russian (the source strings in the code), Ukrainian, English.
//! `t()` takes the Russian text and returns the text in the chosen language; unknown strings fall back to Russian.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum Lang { #[default] Ru, Uk, En }

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set(l: Lang) { LANG.store(l as u8, Ordering::Relaxed); }
pub fn get() -> Lang { match LANG.load(Ordering::Relaxed) { 1 => Lang::Uk, 2 => Lang::En, _ => Lang::Ru } }

fn table() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    static T: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    T.get_or_init(|| TABLE.iter().map(|(ru, uk, en)| (*ru, (*uk, *en))).collect())
}

pub fn t(ru: &'static str) -> &'static str {
    match get() {
        Lang::Ru => ru,
        l => table().get(ru).map(|(uk, en)| if l == Lang::Uk { *uk } else { *en }).unwrap_or(ru),
    }
}

/// Labels built at run time ("Генерация · шаг 3/8", "Синхронизация губ · часть 2 из 5"): translated piece by piece.
pub fn t_dyn(s: &str) -> String {
    if get() == Lang::Ru {
        return s.to_string();
    }
    s.split(" · ").map(|part| {
        if let Some((_, (uk, en))) = table().iter().find(|(ru, _)| **ru == part) {
            return (if get() == Lang::Uk { *uk } else { *en }).to_string();
        }
        let mut out = part.to_string();
        for (ru, uk, en) in WORDS {
            out = out.replace(ru, if get() == Lang::Uk { uk } else { en });
        }
        out
    }).collect::<Vec<_>>().join(" · ")
}

/// Words inside run-time labels and errors.
const WORDS: &[(&str, &str, &str)] = &[
    ("шаг", "крок", "step"), ("часть", "частина", "part"), (" из ", " з ", " of "), ("Синхронизация губ", "Синхронізація губ", "Lip sync"),
];

include!("i18n_table.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_translated_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (ru, uk, en) in TABLE {
            assert!(seen.insert(*ru), "duplicate: {ru}");
            assert!(!uk.is_empty() && !en.is_empty(), "missing translation: {ru}");
        }
    }

    #[test]
    fn source_strings_are_all_in_the_table() {
        // every translated literal in the UI must have a table entry
        let re = regex::Regex::new(r#"\bt\("((?:[^"\\]|\\.)*)"\)"#).unwrap();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui");
        let mut missing = Vec::new();
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            let src = std::fs::read_to_string(f.path()).unwrap();
            for c in re.captures_iter(&src) {
                let s = c[1].replace("\\\"", "\"").replace("\\n", "\n");
                if !table().contains_key(s.as_str()) && !missing.contains(&s) { missing.push(s); }
            }
        }
        assert!(missing.is_empty(), "no translation for: {missing:#?}");
    }

    #[test]
    fn dynamic_labels() {
        set(Lang::En);
        assert_eq!(t_dyn("Генерация · шаг 3/8"), "Generating · step 3/8");
        set(Lang::Ru);
    }
}

/// `t()` for templates with `{}` placeholders, filled in order.
pub fn tf(template: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = t(template).to_string();
    for a in args {
        out = out.replacen("{}", &a.to_string(), 1);
    }
    out
}
