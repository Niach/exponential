//! Round 1 (docs/round-1-contract.md §4): the locale decisions every renderer
//! shares (`catalog/locale.json`, embedded): the week start by region, the
//! likely region of a bare language, the right-to-left languages. Formatting
//! itself is the platform's ICU; these are the parts that must not differ.
//! Mirrors `src/locale.ts`.

use std::collections::HashMap;
use std::sync::LazyLock;

use indexmap::IndexMap;

use crate::generated::catalog as g;

pub const DEFAULT_LOCALE: &str = g::DEFAULT_LOCALE;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocaleFile {
    week_start: WeekStart,
    likely_region: IndexMap<String, String>,
    rtl: Vec<String>,
}

#[derive(serde::Deserialize)]
struct WeekStart {
    default: u8,
    regions: IndexMap<String, Vec<String>>,
}

struct LocaleData {
    default_week_start: u8,
    by_region: HashMap<String, u8>,
    likely: IndexMap<String, String>,
    rtl: Vec<String>,
}

static DATA: LazyLock<LocaleData> = LazyLock::new(|| {
    let file: LocaleFile = serde_json::from_str(g::LOCALE_JSON).expect("locale.json");
    let mut by_region = HashMap::new();
    for (day, regions) in &file.week_start.regions {
        let day: u8 = day.parse().unwrap_or(1);
        for r in regions {
            by_region.insert(r.clone(), day);
        }
    }
    LocaleData { default_week_start: file.week_start.default, by_region, likely: file.likely_region, rtl: file.rtl }
});

/// `pt-BR` → (`pt`, `BR`); `zh-Hant-TW` → (`zh`, `TW`); `de` → (`de`, None).
/// The region = the first 2-letter or 3-digit subtag after the language.
pub fn parse_locale(locale: &str) -> (String, Option<String>) {
    let normalized = locale.replace('_', "-");
    let tags: Vec<&str> = normalized.split('-').filter(|t| !t.is_empty()).collect();
    let language = tags.first().map(|t| t.to_lowercase()).unwrap_or_default();
    let region = tags.iter().skip(1).find(|t| (t.len() == 2 && t.chars().all(|c| c.is_ascii_alphabetic())) || (t.len() == 3 && t.chars().all(|c| c.is_ascii_digit())));
    (language, region.map(|r| r.to_uppercase()))
}

/// The first day of the week, 0 = Sunday … 6 = Saturday.
pub fn week_start(locale: &str) -> u8 {
    let (language, region) = parse_locale(locale);
    let region = region.or_else(|| DATA.likely.get(&language).cloned());
    region.and_then(|r| DATA.by_region.get(&r).copied()).unwrap_or(DATA.default_week_start)
}

/// `rtl` for the right-to-left languages, else `ltr`.
pub fn text_direction(locale: &str) -> &'static str {
    let (language, _) = parse_locale(locale);
    if DATA.rtl.contains(&language) {
        "rtl"
    } else {
        "ltr"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_start_by_region_likely_regions_cldr_exceptions() {
        for (locale, day) in [("en-US", 0), ("en-GB", 1), ("de-AT", 1), ("en", 0), ("de", 1), ("pt", 0), ("pt-PT", 0), ("ar-EG", 6), ("fa", 6), ("dv-MV", 5), ("zh-Hant-TW", 0), ("xx", 1)] {
            assert_eq!(week_start(locale), day, "{locale}");
        }
    }

    #[test]
    fn parsing_and_direction() {
        assert_eq!(parse_locale("zh_Hant_TW"), ("zh".to_string(), Some("TW".to_string())));
        assert_eq!(parse_locale("es-419"), ("es".to_string(), Some("419".to_string())));
        assert_eq!(text_direction("ar-EG"), "rtl");
        assert_eq!(text_direction("he"), "rtl");
        assert_eq!(text_direction("de-DE"), "ltr");
        assert_eq!(DEFAULT_LOCALE, "en-US");
    }
}
