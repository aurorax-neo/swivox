use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::i18n_en::{EN_BACKEND_ENTRIES, EN_BACKEND_PATTERN_ENTRIES, EN_PLAIN_ENTRIES, EN_TEMPLATE_ENTRIES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Zh,
    En,
}

impl Language {
    pub fn toggled(self) -> Self {
        match self {
            Self::Zh => Self::En,
            Self::En => Self::Zh,
        }
    }
}

static ACTIVE_LANGUAGE: AtomicU8 = AtomicU8::new(0);

struct EnDictionaries {
    plain: HashMap<&'static str, &'static str>,
    backend: HashMap<&'static str, &'static str>,
    template: HashMap<&'static str, &'static str>,
    patterns: Vec<(Regex, &'static str)>,
}

fn dictionaries() -> &'static EnDictionaries {
    static DICTS: OnceLock<EnDictionaries> = OnceLock::new();
    DICTS.get_or_init(|| {
        let plain = EN_PLAIN_ENTRIES.iter().copied().collect();
        let backend = EN_BACKEND_ENTRIES.iter().copied().collect();
        let template = EN_TEMPLATE_ENTRIES.iter().copied().collect();
        let patterns = EN_BACKEND_PATTERN_ENTRIES
            .iter()
            .filter_map(|(pat, rep)| Regex::new(pat).ok().map(|re| (re, *rep)))
            .collect();
        EnDictionaries {
            plain,
            backend,
            template,
            patterns,
        }
    })
}

pub fn get_language() -> Language {
    match ACTIVE_LANGUAGE.load(Ordering::Relaxed) {
        1 => Language::En,
        _ => Language::Zh,
    }
}

pub fn set_language(lang: Language) {
    let code = match lang {
        Language::Zh => 0,
        Language::En => 1,
    };
    ACTIVE_LANGUAGE.store(code, Ordering::Relaxed);
}

/// 按指定语言翻译普通中文文本，未命中时回退原文。
pub fn translate_for(lang: Language, zh: &str) -> String {
    if lang != Language::En {
        return zh.to_string();
    }
    let dicts = dictionaries();
    if let Some(found) = dicts.plain.get(zh).or_else(|| dicts.backend.get(zh)) {
        return (*found).to_string();
    }
    for (re, replacement) in &dicts.patterns {
        if re.is_match(zh) {
            return re.replace(zh, *replacement).into_owned();
        }
    }
    zh.to_string()
}

/// 按当前全局激活语言翻译普通中文文本。
pub fn t(zh: &str) -> String {
    translate_for(get_language(), zh)
}

/// 按指定语言翻译带 `{0}`, `{1}` 占位符的模板字符串。
pub fn translate_template_for<S: Display>(lang: Language, key: &str, args: &[S]) -> String {
    let template = if lang == Language::En {
        let dicts = dictionaries();
        dicts
            .template
            .get(key)
            .or_else(|| dicts.plain.get(key))
            .copied()
            .unwrap_or(key)
    } else {
        key
    };
    static PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();
    let re = PLACEHOLDER_RE.get_or_init(|| Regex::new(r"\{(\d+)\}").expect("valid regex"));
    re.replace_all(template, |caps: &regex::Captures<'_>| {
        let index: usize = caps[1].parse().unwrap_or(usize::MAX);
        args.get(index)
            .map(ToString::to_string)
            .unwrap_or_else(|| caps[0].to_string())
    })
    .into_owned()
}

/// 按当前全局激活语言翻译带 `{0}`, `{1}` 占位符的模板字符串。
pub fn tf<S: Display>(key: &str, args: &[S]) -> String {
    translate_template_for(get_language(), key, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_pattern_and_template_lookup_work_in_both_languages() {
        assert_eq!(translate_for(Language::Zh, "本地服务器状态"), "本地服务器状态");
        assert_eq!(
            translate_for(Language::En, "本地服务器状态"),
            "Local server status"
        );
        assert_eq!(
            translate_for(Language::En, "缓存扫描失败：磁盘忙"),
            "Cache scan failed: 磁盘忙"
        );
        assert_eq!(
            translate_template_for(Language::Zh, "上次接管：{0}", &["10:00"]),
            "上次接管：10:00"
        );
        assert_eq!(
            translate_template_for(Language::En, "上次接管：{0}", &["10:00"]),
            "Last takeover: 10:00"
        );
    }

    fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    fn unescape_rust_str(raw: &str) -> String {
        raw.replace(r#"\""#, "\"")
            .replace(r"\n", "\n")
            .replace(r"\\", "\\")
    }

    #[test]
    fn all_gpui_and_backend_t_keys_have_english_translations() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let gpui_src = manifest_dir.join("../../apps/chatgptx-gpui/src");
        let backend_src = manifest_dir.join("src");

        let mut files = Vec::new();
        collect_rs_files(&gpui_src, &mut files);
        collect_rs_files(&backend_src, &mut files);

        let call_re = Regex::new(r#"(?s)\btf?\(\s*"((?:[^"\\]|\\.)*)""#).unwrap();
        let nav_re = Regex::new(r#"(?s)label_zh:\s*"((?:[^"\\]|\\.)*)""#).unwrap();
        let dicts = dictionaries();

        let mut missing = Vec::new();
        for file in files {
            let Some(name) = file.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name == "i18n.rs" || name == "i18n_en.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap();
            for caps in call_re.captures_iter(&text).chain(nav_re.captures_iter(&text)) {
                let key = unescape_rust_str(&caps[1]);
                let hit = dicts.plain.contains_key(key.as_str())
                    || dicts.backend.contains_key(key.as_str())
                    || dicts.template.contains_key(key.as_str())
                    || dicts.patterns.iter().any(|(re, _)| re.is_match(&key));
                if !hit && !missing.contains(&key) {
                    missing.push(key);
                }
            }
        }
        assert!(
            missing.is_empty(),
            "Missing English translations for {} key(s): {:?}",
            missing.len(),
            missing
        );
    }
}
