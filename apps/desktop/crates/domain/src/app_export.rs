//! VAPP-121 — an app as ONE JSON file, the Rust twin of
//! `packages/domain-contract/src/app-export.ts` (iOS `AppExport.swift`,
//! Android `AppExport.kt`), locked by `fixtures/app-export.json`
//! (`exportCases`): the file's name, the file built from the synced app row
//! and its records, the copy and the caps. Parsing an import is the
//! SERVER's (`apps.validateImport`, the one authority ×4).

use std::cmp::Ordering;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{Map, Value};

const EXPORT_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/app-export.json");

/// The fixture's `copy`: the Export dialog and the New app import step.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppExportCopy {
    pub export: String,
    /// `Export {name}`.
    pub export_title: String,
    pub include_records: String,
    pub import: String,
    pub import_title: String,
    pub import_placeholder: String,
    pub choose_file: String,
    pub import_action: String,
    pub import_note: String,
    /// `and {n} more`.
    pub more_issues: String,
}

/// The fixture's `limits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppExportLimits {
    pub import_max_bytes: usize,
    pub records_max: usize,
    pub issues_shown: usize,
}

/// The fixture's `capture`: the import step's capture text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppExportCapture {
    pub import_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportSpec {
    format: String,
    file_suffix: String,
    mime: String,
    limits: AppExportLimits,
    copy: AppExportCopy,
    capture: AppExportCapture,
}

static SPEC: LazyLock<ExportSpec> =
    LazyLock::new(|| serde_json::from_str(EXPORT_JSON).expect("app-export.json parses"));

/// `exponential-app/1`.
pub static APP_EXPORT_FORMAT: LazyLock<&'static str> = LazyLock::new(|| SPEC.format.as_str());
/// `.app.json`.
pub static APP_EXPORT_SUFFIX: LazyLock<&'static str> = LazyLock::new(|| SPEC.file_suffix.as_str());
/// `application/json`.
pub static APP_EXPORT_MIME: LazyLock<&'static str> = LazyLock::new(|| SPEC.mime.as_str());
pub static APP_EXPORT_LIMITS: LazyLock<AppExportLimits> = LazyLock::new(|| SPEC.limits);
pub static APP_EXPORT_COPY: LazyLock<&'static AppExportCopy> = LazyLock::new(|| &SPEC.copy);
pub static APP_EXPORT_CAPTURE: LazyLock<&'static AppExportCapture> = LazyLock::new(|| &SPEC.capture);

/// `working-hours` → `working-hours.app.json`.
pub fn app_export_file_name(slug: &str) -> String {
    format!("{slug}{}", *APP_EXPORT_SUFFIX)
}

/// `Export {name}`.
pub fn app_export_title(name: &str) -> String {
    APP_EXPORT_COPY.export_title.replace("{name}", name)
}

/// `and {n} more`, None when every issue shows.
pub fn more_issues_label(total: usize) -> Option<String> {
    let shown = APP_EXPORT_LIMITS.issues_shown;
    (total > shown).then(|| APP_EXPORT_COPY.more_issues.replace("{n}", &(total - shown).to_string()))
}

/// The identity an export carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportApp {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// One synced record (only `kind` and `data` reach the file).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRecord {
    pub id: String,
    pub kind: String,
    pub data: Value,
    /// ISO timestamp (`2026-10-05T18:00:00Z`, or Postgres' `+00` form).
    pub created_at: String,
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Milliseconds since the epoch of an ISO / Postgres timestamp
/// (`2026-10-05T18:00:00Z`, `2026-10-05 18:00:00.123+00`); None when it does
/// not parse (sorted by the raw text then).
fn instant(at: &str) -> Option<i64> {
    let b = at.trim().as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = b.get(from..from + len)?;
        part.iter().all(u8::is_ascii_digit).then(|| std::str::from_utf8(part).ok()?.parse().ok())?
    };
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return None;
    }
    let (h, mi, s) = if matches!(b.get(10), Some(b'T' | b' ')) {
        (num(11, 2)?, num(14, 2)?, num(17, 2)?)
    } else {
        (0, 0, 0)
    };
    let mut i = if b.len() > 10 { 19 } else { 10 };
    let mut millis = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let mut scale = 100;
        while let Some(c) = b.get(i).filter(|c| c.is_ascii_digit()) {
            millis += i64::from(c - b'0') * scale;
            scale /= 10;
            i += 1;
        }
    }
    let offset_min = match b.get(i) {
        None | Some(b'Z' | b'z') => 0,
        Some(sign @ (b'+' | b'-')) => {
            let oh = num(i + 1, 2)?;
            let om = match b.get(i + 3) {
                Some(b':') => num(i + 4, 2)?,
                Some(_) => num(i + 3, 2)?,
                None => 0,
            };
            if *sign == b'-' { -(oh * 60 + om) } else { oh * 60 + om }
        }
        Some(_) => return None,
    };
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s - offset_min * 60;
    Some(secs * 1000 + millis)
}

fn by_created_then_id(a: &ExportRecord, b: &ExportRecord) -> Ordering {
    let at = match (instant(&a.created_at), instant(&b.created_at)) {
        (Some(x), Some(y)) => x.cmp(&y),
        _ => a.created_at.cmp(&b.created_at),
    };
    at.then_with(|| a.id.cmp(&b.id))
}

/// The file for an app: its identity, the head package and, when switched
/// on, every record as `{kind, data}` oldest first (createdAt, then id).
/// Never ids, authors, times or grants.
pub fn build_app_export(app: &ExportApp, package: &Value, records: &[ExportRecord], include_records: bool) -> Value {
    let mut identity = Map::new();
    identity.insert("name".into(), Value::String(app.name.clone()));
    identity.insert("icon".into(), app.icon.clone().map(Value::String).unwrap_or(Value::Null));
    identity.insert("color".into(), app.color.clone().map(Value::String).unwrap_or(Value::Null));
    identity.insert("description".into(), Value::String(app.description.clone().unwrap_or_default()));
    let mut file = Map::new();
    file.insert("format".into(), Value::String(APP_EXPORT_FORMAT.to_string()));
    file.insert("app".into(), Value::Object(identity));
    file.insert("package".into(), package.clone());
    if include_records && !records.is_empty() {
        let mut sorted: Vec<&ExportRecord> = records.iter().collect();
        sorted.sort_by(|a, b| by_created_then_id(a, b));
        let rows = sorted
            .into_iter()
            .map(|r| serde_json::json!({ "kind": r.kind, "data": r.data }))
            .collect();
        file.insert("records".into(), Value::Array(rows));
    }
    Value::Object(file)
}

/// The file's text: two-space JSON and a trailing newline.
pub fn app_export_text(file: &Value) -> String {
    let mut text = serde_json::to_string_pretty(file).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(EXPORT_JSON).unwrap()
    }

    fn opt(v: &Value) -> Option<String> {
        v.as_str().map(str::to_string)
    }

    #[test]
    fn the_fixture_parses() {
        assert_eq!(*APP_EXPORT_FORMAT, "exponential-app/1");
        assert_eq!(*APP_EXPORT_SUFFIX, ".app.json");
        assert_eq!(*APP_EXPORT_MIME, "application/json");
        assert!(APP_EXPORT_LIMITS.issues_shown > 0 && APP_EXPORT_LIMITS.records_max > 0);
        assert_eq!(app_export_title("Working hours"), "Export Working hours");
        let shown = APP_EXPORT_LIMITS.issues_shown;
        assert_eq!(more_issues_label(shown), None);
        assert_eq!(more_issues_label(shown + 3), Some("and 3 more".into()));
        assert!(serde_json::from_str::<Value>(&APP_EXPORT_CAPTURE.import_text).is_ok());
    }

    #[test]
    fn export_cases() {
        let fixture = fixture();
        let cases = fixture["exportCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let app = ExportApp {
                name: input["app"]["name"].as_str().unwrap().to_string(),
                icon: opt(&input["app"]["icon"]),
                color: opt(&input["app"]["color"]),
                description: opt(&input["app"]["description"]),
            };
            let records: Vec<ExportRecord> = input["records"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| ExportRecord {
                    id: r["id"].as_str().unwrap().to_string(),
                    kind: r["kind"].as_str().unwrap().to_string(),
                    data: r["data"].clone(),
                    created_at: r["createdAt"].as_str().unwrap().to_string(),
                })
                .collect();
            let slug = input["app"]["slug"].as_str().unwrap();
            assert_eq!(app_export_file_name(slug), case["expected"]["fileName"].as_str().unwrap(), "{name}");
            let file = build_app_export(&app, &input["package"], &records, input["includeRecords"].as_bool().unwrap());
            assert_eq!(file, case["expected"]["file"], "{name}");
            // The text round-trips to the same file.
            let text = app_export_text(&file);
            assert!(text.ends_with('\n'));
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), file, "{name}");
        }
    }

    #[test]
    fn postgres_timestamps_sort_like_iso() {
        let rec = |id: &str, at: &str| ExportRecord { id: id.into(), kind: "k".into(), data: Value::Null, created_at: at.into() };
        let a = rec("b", "2026-10-05 18:00:00.5+00");
        let b = rec("a", "2026-10-05T19:00:00Z");
        assert_eq!(by_created_then_id(&a, &b), Ordering::Less);
        assert_eq!(instant("2026-10-05 18:00:00+00"), instant("2026-10-05T18:00:00Z"));
        assert_eq!(instant("1970-01-01T00:00:01.250Z"), Some(1250));
        assert_eq!(instant("1970-01-01T02:00:00+02:00"), Some(0));
        assert_eq!(instant("nope"), None);
    }
}
