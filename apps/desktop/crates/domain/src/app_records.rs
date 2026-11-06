//! VAPP-122 — app RECORDS, the Rust twin of
//! `packages/domain-contract/src/app-records.ts` (iOS `AppRecords.swift`,
//! Android `AppRecords.kt`), byte-locked by `fixtures/app-records.json`
//! (`countLabelCases`, VAPP-187 `canChangeCases`, VAPP-188 `onBehalf` + the
//! Data tab's byline / author picker cases). VAPP-189: the `records:<kind>?…`
//! query and its value are eUI's ONE evaluator (`eui_core::host::query`,
//! packages/exponential-ui fixtures/records-query.json); the desktop host
//! calls it directly.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::apps::APPS;

const RECORDS_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/app-records.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsSpec {
    pub row_keys: Vec<String>,
    pub ownership: RecordsOwnership,
    #[serde(rename = "onBehalf")]
    pub on_behalf: RecordsOnBehalf,
}

/// The fixture's `onBehalf` block (VAPP-188) minus its cases: the server's
/// refusals of a write naming another member as its author.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsOnBehalf {
    /// FORBIDDEN: a non-owner names someone else.
    pub refusal: String,
    /// BAD_REQUEST: the named author is not a member of the team.
    pub not_member: String,
}

/// VAPP-187: who may patch / delete a record (`policy` per op).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnershipPolicy {
    Author,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct OwnershipPolicies {
    pub update: OwnershipPolicy,
    pub delete: OwnershipPolicy,
}

/// The fixture's `ownership` block (VAPP-187) minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RecordsOwnership {
    pub policy: OwnershipPolicies,
    pub refusal: String,
}

/// The write a viewer wants to make to an existing record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOp {
    Update,
    Delete,
}

/// VAPP-187: a record's AUTHOR (`authorId ?? createdBy`): whose record it
/// is, what `mine`/`user=` and the per-member views read.
pub fn record_author<'a>(author_id: Option<&'a str>, created_by: Option<&'a str>) -> Option<&'a str> {
    author_id.or(created_by)
}

/// May `viewer` (with team `role`; `None` = not a member) `op` the record
/// (`ownership.canChangeCases`)? Policy `author` = the author or a team owner.
pub fn can_change_record(
    author_id: Option<&str>,
    created_by: Option<&str>,
    viewer: Option<&str>,
    role: Option<&str>,
    op: RecordOp,
) -> bool {
    let (Some(viewer), Some(role)) = (viewer, role) else {
        return false;
    };
    if role == "owner" {
        return true;
    }
    let policy = match op {
        RecordOp::Update => APP_RECORDS.ownership.policy.update,
        RecordOp::Delete => APP_RECORDS.ownership.policy.delete,
    };
    if policy == OwnershipPolicy::Member {
        return true;
    }
    record_author(author_id, created_by) == Some(viewer)
}

/// VAPP-188: the verdict on a write naming `author` (`onBehalf.writeAsCases`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAsVerdict {
    Ok,
    /// FORBIDDEN `onBehalf.refusal`.
    Forbidden,
    /// BAD_REQUEST `onBehalf.notMember`.
    NotMember,
}

impl WriteAsVerdict {
    /// The fixture's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Forbidden => "forbidden",
            Self::NotMember => "notMember",
        }
    }
}

/// May `viewer` (team `role`; `None` = not a member) write a record as
/// `author` (`None` = as themself)? Another member = owners only, and the
/// author must be one of `members` (`writeAsCases`).
pub fn write_as_verdict(viewer: Option<&str>, role: Option<&str>, author: Option<&str>, members: &[&str]) -> WriteAsVerdict {
    let (Some(viewer), Some(role)) = (viewer, role) else {
        return WriteAsVerdict::Forbidden;
    };
    let Some(author) = author.filter(|author| *author != viewer) else {
        return WriteAsVerdict::Ok;
    };
    if role != "owner" {
        return WriteAsVerdict::Forbidden;
    }
    if members.contains(&author) {
        WriteAsVerdict::Ok
    } else {
        WriteAsVerdict::NotMember
    }
}

/// An imported record's author (`importCases`): absent, the importer or a
/// stranger to the team = the importer; another member = them when the
/// importer is an owner, else the importer too (VAPP-192: a member importing
/// a teammate's export keeps the rows as their own; never `None` now).
pub fn import_author<'a>(importer: &'a str, role: Option<&str>, author: Option<&'a str>, members: &[&str]) -> Option<&'a str> {
    match author {
        Some(author) if author != importer && members.contains(&author) => Some(if role == Some("owner") { author } else { importer }),
        _ => Some(importer),
    }
}

/// VAPP-188: the Data tab's `by` cell (`dataTab.bylineCases`): the author's
/// name (else `fallback`, apps.json `copy.byAgent`), `{writer} for {author}`
/// (`copy.byFor`) when someone else with a name wrote it. `names` = user id
/// → display name (`None` / "" = unknown).
pub fn record_byline(
    author_id: Option<&str>,
    created_by: Option<&str>,
    names: impl Fn(&str) -> Option<String>,
    fallback: &str,
) -> String {
    let name = |id: &str| names(id).filter(|name| !name.is_empty());
    let author_id = record_author(author_id, created_by);
    let author = author_id.and_then(name).unwrap_or_else(|| fallback.to_string());
    let writer = created_by.filter(|by| Some(*by) != author_id).and_then(name);
    match writer {
        Some(writer) => DATA_TAB.copy.by_for.replace("{writer}", &writer).replace("{author}", &author),
        None => author,
    }
}

/// VAPP-188: the owner's Author picker, `(id, name)` members by name
/// (case-insensitive, ties by id: `dataTab.authorOptionCases`).
pub fn author_options(mut users: Vec<(String, String)>) -> Vec<(String, String)> {
    users.sort_by(|(a_id, a), (b_id, b)| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a_id.cmp(b_id)));
    users
}

/// What [`form_author`] answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormAuthor<'a> {
    /// The picker's initial pick.
    pub default: Option<&'a str>,
    /// What Save sends as `author` (`None` = omit).
    pub send: Option<&'a str>,
}

/// VAPP-188: the picker's default (the record's author on edit, the viewer
/// on create) and what Save sends (the pick when it differs from the
/// record's author / the viewer, else `None`; `dataTab.formAuthorCases`).
/// `record` = the edited row's `(author_id, created_by)`, `None` = create.
pub fn form_author<'a>(
    viewer: Option<&'a str>,
    record: Option<(Option<&'a str>, Option<&'a str>)>,
    picked: Option<&'a str>,
) -> FormAuthor<'a> {
    let base = match record {
        Some((author_id, created_by)) => record_author(author_id, created_by),
        None => viewer,
    };
    FormAuthor { default: base.or(viewer), send: picked.filter(|picked| Some(*picked) != base) }
}

/// `fixtures/app-records.json` minus its cases.
pub static APP_RECORDS: LazyLock<RecordsSpec> =
    LazyLock::new(|| serde_json::from_str(RECORDS_JSON).expect("app-records.json parses"));

/// One synced `app_records` row as the Data tab lists it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordRowInput<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub data: &'a Value,
    pub created_by: Option<&'a str>,
    /// VAPP-187: whose record it is (`None` on a legacy row = `created_by`).
    pub author_id: Option<&'a str>,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

impl<'a> RecordRowInput<'a> {
    /// VAPP-187: `authorId ?? createdBy`.
    pub fn author(&self) -> Option<&'a str> {
        record_author(self.author_id, self.created_by)
    }
}

/// JS `Number(text)` when it parses to a finite number (`None` = NaN, or
/// blank).
fn num_text(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // Rust accepts `inf` / `nan` spellings JS does not; neither is finite.
    t.parse::<f64>().ok().filter(|f| f.is_finite())
}

/// A JS number as JSON: integral values as integers (so `0` equals `0`).
fn number_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

/// JS `String(n)` for the ordinary range.
fn js_number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// en-US grouping: `1,240`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Settings → Apps → the app: "1,240 records", "1,240 of 25,000 records"
/// (`max` = the team's quota, `None` = unlimited), "No records".
pub fn records_count_label(count: u64, max: Option<u64>) -> String {
    let copy = &APPS.copy;
    if count == 0 {
        return copy.records_none.clone();
    }
    let n = grouped(count);
    match max {
        None if count == 1 => copy.records_one.clone(),
        None => copy.records.replace("{n}", &n),
        Some(max) => copy.records_of.replace("{n}", &n).replace("{max}", &grouped(max)),
    }
}

// VAPP-178 THE DATA TAB (fixture `dataTab`): the editor's records browser
// and form, pure so the four clients replay the same cases.

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataTabCopy {
    /// `{kind} · {n}`.
    pub kind_option: String,
    pub add_record: String,
    pub edit: String,
    pub delete: String,
    pub delete_title: String,
    pub delete_body: String,
    pub by: String,
    pub updated: String,
    pub empty: String,
    pub no_kinds: String,
    pub empty_title: String,
    /// `New {kind}`.
    pub new_title: String,
    /// `Edit {kind}`.
    pub edit_title: String,
    pub save: String,
    pub cancel: String,
    pub yes: String,
    pub no: String,
    pub none: String,
    pub optional: String,
    pub list_hint: String,
    /// VAPP-188: the owner's Author field.
    pub author: String,
    /// VAPP-188: `{writer} for {author}`.
    pub by_for: String,
    /// VAPP-221: one attachment ref.
    pub file: String,
    /// VAPP-221: `{n} files`.
    pub files: String,
}

/// The wide table's fixed columns (px).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataTabColumns {
    pub by_width: f32,
    pub updated_width: f32,
    pub menu_width: f32,
    /// A sharing column's floor.
    pub min_width: f32,
    /// The px between two cells.
    pub gap: f32,
    /// A typed column's fixed width (a string `format`, else the `type`).
    pub type_widths: BTreeMap<String, f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DataTabTestIds {
    pub kind: String,
    pub add: String,
    pub row: String,
    pub menu: String,
    pub form: String,
    pub save: String,
    pub error: String,
    /// VAPP-215: the record form's History section, its rows, Restore.
    pub history: String,
    #[serde(rename = "historyRow")]
    pub history_row: String,
    pub restore: String,
}

/// `dataTab` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DataTabSpec {
    pub copy: DataTabCopy,
    pub columns: DataTabColumns,
    /// A string `format` → the text field's placeholder.
    pub placeholders: BTreeMap<String, String>,
    #[serde(rename = "testIds")]
    pub test_ids: DataTabTestIds,
}

pub static DATA_TAB: LazyLock<DataTabSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(RECORDS_JSON).expect("app-records.json parses");
    serde_json::from_value(fixture["dataTab"].clone()).expect("dataTab parses")
});

/// A list's item type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct DataItems {
    #[serde(rename = "type", default)]
    pub ty: String,
    /// VAPP-221: `attachment` on a list of files.
    #[serde(default)]
    pub format: Option<String>,
}

/// A kind's schema field (the SDK's JSON Schema subset, host/records.ts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct DataField {
    #[serde(rename = "type", default)]
    pub ty: String,
    #[serde(rename = "enum", default)]
    pub enum_values: Option<Vec<String>>,
    #[serde(default)]
    pub items: Option<DataItems>,
    #[serde(default)]
    pub format: Option<String>,
}

/// A kind's schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct DataSchema {
    #[serde(default)]
    pub properties: BTreeMap<String, DataField>,
    #[serde(default)]
    pub required: Vec<String>,
}

impl DataSchema {
    /// A schema JSON (lenient: anything unreadable = no fields).
    pub fn from_value(value: &Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}

/// A head manifest's kind schema (`records.<kind>.schema`).
pub fn kind_schema(manifest: Option<&Value>, kind: &str) -> Option<DataSchema> {
    let schema = manifest?.get("records")?.get(kind)?.get("schema")?;
    Some(DataSchema::from_value(schema))
}

/// The head manifest's kinds, sorted by name, each with its live count
/// (`kindCases`); a row of an undeclared kind is ignored.
pub fn data_kinds<'a>(manifest: Option<&Value>, row_kinds: impl IntoIterator<Item = &'a str>) -> Vec<(String, usize)> {
    let Some(records) = manifest.and_then(|m| m.get("records")).and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut kinds: Vec<(String, usize)> = records.keys().map(|k| (k.clone(), 0)).collect();
    kinds.sort_by(|a, b| a.0.cmp(&b.0));
    for kind in row_kinds {
        if let Ok(i) = kinds.binary_search_by(|(k, _)| k.as_str().cmp(kind)) {
            kinds[i].1 += 1;
        }
    }
    kinds
}

/// A column's fixed width in px (`columns.typeWidths`), `None` = it
/// shares the rest of the row.
pub fn column_width(field: &DataField) -> Option<f32> {
    let widths = &DATA_TAB.columns.type_widths;
    if field.ty == "string" {
        return field.format.as_ref().and_then(|format| widths.get(format)).copied();
    }
    widths.get(&field.ty).copied()
}

/// The table's narrowest width (cells + gaps, no row padding): every
/// schema column at its fixed width or the floor, then `by`, `updated`
/// and the menu; a pane narrower than this scrolls sideways.
pub fn table_min_width(widths: &[Option<f32>]) -> f32 {
    let c = &DATA_TAB.columns;
    let cells: f32 = widths.iter().map(|w| w.unwrap_or(c.min_width)).sum();
    cells + c.by_width + c.updated_width + c.menu_width + c.gap * (widths.len() + 2) as f32
}

/// `{kind} · {n}`.
pub fn kind_option(kind: &str, count: usize) -> String {
    DATA_TAB.copy.kind_option.replace("{kind}", kind).replace("{n}", &count.to_string())
}

/// The columns (`columnCases`): the `required` names in their order, then
/// every other property sorted by name.
pub fn data_columns(schema: &DataSchema) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in &schema.required {
        if schema.properties.contains_key(name) && !out.contains(name) {
            out.push(name.clone());
        }
    }
    // BTreeMap keys iterate sorted (code points, as JS `.sort()` for ASCII).
    for name in schema.properties.keys() {
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    out
}

/// A kind's rows, `updated_at` desc, ties by id (`rowsCases`).
pub fn data_rows<'r, 'a>(rows: &'r [RecordRowInput<'a>], kind: &str) -> Vec<&'r RecordRowInput<'a>> {
    let mut out: Vec<&RecordRowInput> = rows.iter().filter(|r| r.kind == kind).collect();
    out.sort_by(|a, b| b.updated_at.cmp(a.updated_at).then_with(|| a.id.cmp(b.id)));
    out
}

/// JS `String(n)` of a JSON number.
fn number_text(n: &serde_json::Number) -> String {
    if n.is_i64() || n.is_u64() {
        n.to_string()
    } else {
        n.as_f64().map(js_number_text).unwrap_or_else(|| n.to_string())
    }
}

/// VAPP-221: an attachment ref (eUI `RECORD_ATTACHMENT_REF`):
/// `/api/attachments/<lowercase uuid>`.
fn is_attachment_ref(value: &Value) -> bool {
    let Some(id) = value.as_str().and_then(|s| s.strip_prefix("/api/attachments/")) else { return false };
    let b = id.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}

/// VAPP-221: a field holding files (`format: "attachment"` on a string or
/// a list's string items): no form field, the app's FileUpload writes it.
pub fn is_attachment_field(field: &DataField) -> bool {
    if field.ty == "array" {
        field.items.as_ref().is_some_and(|i| i.ty == "string" && i.format.as_deref() == Some("attachment"))
    } else {
        field.ty == "string" && field.format.as_deref() == Some("attachment")
    }
}

/// One cell's text (`cellCases`); `None` = missing. VAPP-221: files read
/// as a count (a ref, or a non-empty list of refs only).
pub fn cell_text(value: Option<&Value>) -> String {
    match value {
        Some(v) if is_attachment_ref(v) => DATA_TAB.copy.file.clone(),
        Some(Value::Array(items)) if !items.is_empty() && items.iter().all(is_attachment_ref) => {
            if items.len() == 1 {
                DATA_TAB.copy.file.clone()
            } else {
                DATA_TAB.copy.files.replace("{n}", &items.len().to_string())
            }
        }
        _ => plain_text(value),
    }
}

/// A value as written (a form input's text; a cell without files).
fn plain_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(b)) => if *b { DATA_TAB.copy.yes.clone() } else { DATA_TAB.copy.no.clone() },
        Some(Value::Number(n)) => number_text(n),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items.iter().map(|v| plain_text(Some(v))).collect::<Vec<_>>().join(", "),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataControl {
    Text,
    Number,
    Switch,
    Select,
    List,
}

impl DataControl {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Switch => "switch",
            Self::Select => "select",
            Self::List => "list",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "text" => Some(Self::Text),
            "number" => Some(Self::Number),
            "switch" => Some(Self::Switch),
            "select" => Some(Self::Select),
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

/// One form field (`formCases`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFormField {
    pub name: String,
    pub control: DataControl,
    pub required: bool,
    pub placeholder: Option<String>,
    pub options: Option<Vec<String>>,
    pub item_type: Option<String>,
}

/// The form's fields, in column order (`formCases`).
pub fn form_fields(schema: &DataSchema) -> Vec<DataFormField> {
    data_columns(schema)
        .into_iter()
        .filter(|name| !is_attachment_field(&schema.properties[name]))
        .map(|name| {
            let field = &schema.properties[&name];
            let mut out = DataFormField {
                required: schema.required.contains(&name),
                name,
                control: DataControl::Text,
                placeholder: None,
                options: None,
                item_type: None,
            };
            match field.ty.as_str() {
                "boolean" => out.control = DataControl::Switch,
                "number" | "integer" => out.control = DataControl::Number,
                "array" => {
                    out.control = DataControl::List;
                    let ty = field.items.as_ref().map(|i| i.ty.clone()).filter(|t| !t.is_empty());
                    out.item_type = Some(ty.unwrap_or_else(|| "string".into()));
                }
                _ => {
                    if let Some(values) = &field.enum_values {
                        out.control = DataControl::Select;
                        out.options = Some(values.clone());
                    } else {
                        out.placeholder = field.format.as_ref().and_then(|f| DATA_TAB.placeholders.get(f)).cloned();
                    }
                }
            }
            out
        })
        .collect()
}

/// A control's input: a switch takes a boolean, every other control text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormInput {
    Text(String),
    Bool(bool),
}

/// A stored value → the control's initial input (`inputCases`).
pub fn form_input(control: DataControl, value: Option<&Value>) -> FormInput {
    if control == DataControl::Switch {
        return FormInput::Bool(matches!(value, Some(Value::Bool(true))));
    }
    FormInput::Text(match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::Array(items)) if control == DataControl::List => {
            items.iter().map(|v| plain_text(Some(v))).collect::<Vec<_>>().join(", ")
        }
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => number_text(n),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
    })
}

/// JS `Number(text)` when finite and non-blank, else the raw text.
fn parse_number(text: &str) -> Value {
    match num_text(text) {
        Some(n) => number_value(n),
        None => Value::String(text.to_string()),
    }
}

fn parse_item(text: &str, item_type: Option<&str>) -> Value {
    match item_type {
        Some("number" | "integer") => parse_number(text),
        Some("boolean") => match text {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(text.to_string()),
        },
        _ => Value::String(text.to_string()),
    }
}

/// The inputs → the record's data (`parseCases`): an empty field = null,
/// numbers and list items parse (unparsable stays the raw text, so the
/// server names it); CREATE omits the nulls, EDIT keeps them (null clears).
pub fn form_data(schema: &DataSchema, inputs: &HashMap<String, FormInput>, editing: bool) -> Map<String, Value> {
    let mut data = Map::new();
    for field in form_fields(schema) {
        let input = inputs.get(&field.name);
        let value = if field.control == DataControl::Switch {
            Value::Bool(matches!(input, Some(FormInput::Bool(true))))
        } else {
            let text = match input {
                Some(FormInput::Text(t)) => t.as_str(),
                _ => "",
            };
            if field.control == DataControl::List {
                let items: Vec<Value> = text
                    .split(',')
                    .map(str::trim)
                    .filter(|i| !i.is_empty())
                    .map(|i| parse_item(i, field.item_type.as_deref()))
                    .collect();
                if items.is_empty() { Value::Null } else { Value::Array(items) }
            } else if text.is_empty() {
                Value::Null
            } else if field.control == DataControl::Number {
                parse_number(text)
            } else {
                Value::String(text.to_string())
            }
        };
        if !value.is_null() || editing {
            data.insert(field.name, value);
        }
    }
    data
}

// VAPP-215 (R4) RECORD HISTORY + UNDO (fixture `history`): what one
// `app_record_events` row means, its undo and its History row, pure so the
// four clients replay the same cases (TS `app-records.ts`).

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCopy {
    pub history: String,
    pub restore: String,
    pub undo: String,
    pub created: String,
    pub deleted: String,
    pub restored: String,
    /// `Changed {fields}`.
    pub changed: String,
    /// `+{n}`.
    pub more: String,
    pub saved: String,
    pub empty: String,
    pub undo_create: String,
    pub undo_update: String,
    pub undo_delete: String,
    pub undo_batch: String,
    pub undo_verb: String,
    pub undo_thing: String,
}

/// `history` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySpec {
    pub undo_seconds: u64,
    pub retention_days: u64,
    pub batch: u64,
    pub history_limit: u64,
    pub max_fields: usize,
    pub ops: Vec<String>,
    pub vias: Vec<String>,
    pub copy: HistoryCopy,
}

pub static APP_RECORD_HISTORY: LazyLock<HistorySpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(RECORDS_JSON).expect("app-records.json parses");
    serde_json::from_value(fixture["history"].clone()).expect("history parses")
});

/// An `app_record_events.op`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordEventOp {
    Create,
    Update,
    Delete,
    Restore,
}

impl RecordEventOp {
    pub fn parse(op: &str) -> Option<Self> {
        match op {
            "create" => Some(Self::Create),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            "restore" => Some(Self::Restore),
            _ => None,
        }
    }
}

/// The part of an event the pure rules read (`before` / `patch` = a JSON
/// object, `None` = null).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordEventCore {
    pub op: RecordEventOp,
    pub before: Option<Map<String, Value>>,
    pub patch: Option<Map<String, Value>>,
}

/// One event as `apps.records.history` returns it (newest first).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordEvent {
    pub id: String,
    pub core: RecordEventCore,
    pub actor: Option<String>,
    /// VAPP-241: what caused it; `share:<name>` = a guest's submission
    /// (app-share.json `historyCases`).
    pub via: Option<String>,
    pub created_at: String,
}

impl RecordEvent {
    /// One `apps.records.history` event (`None` = unreadable).
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = |key: &str| value.get(key).and_then(Value::as_object).cloned();
        Some(Self {
            id: value.get("id")?.as_str()?.to_string(),
            core: RecordEventCore {
                op: RecordEventOp::parse(value.get("op")?.as_str()?)?,
                before: object("before"),
                patch: object("patch"),
            },
            actor: value.get("actor").and_then(Value::as_str).map(str::to_string),
            via: value.get("via").and_then(Value::as_str).map(str::to_string),
            created_at: value.get("createdAt").and_then(Value::as_str).unwrap_or_default().to_string(),
        })
    }
}

/// The write that undoes an event: a delete, or a restore of `data`.
#[derive(Debug, Clone, PartialEq)]
pub enum InverseWrite {
    Delete,
    Restore(Map<String, Value>),
}

/// The record an event left (`stateCases`): `None` = gone.
pub fn event_state(event: &RecordEventCore) -> Option<Map<String, Value>> {
    match event.op {
        RecordEventOp::Delete => None,
        RecordEventOp::Update => {
            let mut out = event.before.clone().unwrap_or_default();
            for (k, v) in event.patch.iter().flatten() {
                out.insert(k.clone(), v.clone());
            }
            Some(out)
        }
        RecordEventOp::Create | RecordEventOp::Restore => Some(event.patch.clone().unwrap_or_default()),
    }
}

/// Undo = a new write of the event's `before` (`inverseCases`).
pub fn inverse_write(event: &RecordEventCore) -> InverseWrite {
    match &event.before {
        None => InverseWrite::Delete,
        Some(before) => InverseWrite::Restore(before.clone()),
    }
}

/// Deep JSON equality (objects by key, numbers by value: `2` equals `2.0`).
pub fn json_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y || x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_equal(a, b)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_equal(v, w)))
        }
        _ => a == b,
    }
}

/// A History row's title (`labelCases`).
pub fn event_label(event: &RecordEventCore) -> String {
    let c = &APP_RECORD_HISTORY.copy;
    match event.op {
        RecordEventOp::Create => return c.created.clone(),
        RecordEventOp::Delete => return c.deleted.clone(),
        RecordEventOp::Restore => return c.restored.clone(),
        RecordEventOp::Update => {}
    }
    let empty = Map::new();
    let before = event.before.as_ref().unwrap_or(&empty);
    let patch = event.patch.as_ref().unwrap_or(&empty);
    let mut changed: Vec<&str> = patch
        .iter()
        .filter(|(k, v)| !before.get(k.as_str()).is_some_and(|b| json_equal(b, v)))
        .map(|(k, _)| k.as_str())
        .collect();
    changed.sort_unstable();
    if changed.is_empty() {
        return c.saved.clone();
    }
    let max = APP_RECORD_HISTORY.max_fields;
    let shown = changed.iter().take(max).copied().collect::<Vec<_>>().join(", ");
    let fields = if changed.len() > max {
        format!("{shown} {}", c.more.replace("{n}", &(changed.len() - max).to_string()))
    } else {
        shown
    };
    c.changed.replace("{fields}", &fields)
}

/// One History row (`rowsCases`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub id: String,
    pub label: String,
    pub who: String,
    pub at: String,
    pub restorable: bool,
}

/// The History list (`rowsCases`): the events in the server's order (newest
/// first); Restore on every row but the newest, never on a delete. `names` =
/// user id → display name (`None` = unknown), `fallback` = apps.json byAgent.
pub fn history_rows(events: &[RecordEvent], names: impl Fn(&str) -> Option<String>, fallback: &str) -> Vec<HistoryRow> {
    events
        .iter()
        .enumerate()
        .map(|(i, event)| HistoryRow {
            id: event.id.clone(),
            label: event_label(&event.core),
            who: if crate::app_share::is_share_via(event.via.as_deref()) {
                crate::app_share::APP_SHARE.copy.guest.clone()
            } else {
                event.actor.as_deref().and_then(&names).unwrap_or_else(|| fallback.to_string())
            },
            at: event.created_at.clone(),
            restorable: i > 0 && event.core.op != RecordEventOp::Delete,
        })
        .collect()
}

/// A UI records write the Undo toast names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoableWrite {
    Create,
    Update,
    Delete,
    Batch,
}

/// The Undo toast's title for a UI write.
pub fn undo_title(write: UndoableWrite) -> &'static str {
    let c = &APP_RECORD_HISTORY.copy;
    match write {
        UndoableWrite::Create => &c.undo_create,
        UndoableWrite::Update => &c.undo_update,
        UndoableWrite::Delete => &c.undo_delete,
        UndoableWrite::Batch => &c.undo_batch,
    }
}

// VAPP-235 (O1) THE OFFLINE OUTBOX moved into the Engine (VAPP-261,
// `eengine::outbox`): the Engine replays `app-records.json` `outbox` as its
// own, and the desktop host keeps these names.
pub use eengine::outbox::*;

// VAPP-228 (X2, docs/spec/apps.md §10.1/§10.3) SPREADSHEET IMPORT (fixture
// `csvImport`): the CSV reader, the column auto-mapping and the cell
// coercion the Data tab's Import dialog runs, pure so the four clients
// replay the same cases. The host appends `validateRecord` (eUI) issues.

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvImportCopy {
    pub import_button: String,
    /// `Import {kind}`.
    pub title: String,
    pub placeholder: String,
    pub choose_file: String,
    pub skip: String,
    pub me: String,
    /// `Column {n}`.
    pub column: String,
    /// `row {row}: {field}: {message}`.
    pub issue: String,
    /// `row {row}: {message}`.
    pub row_issue: String,
    /// `+{n} more`.
    pub more_issues: String,
    /// `{n} of {total} rows ready`.
    pub ready: String,
    /// `Up to {max} rows per import`.
    pub too_many: String,
    pub no_rows: String,
    /// `Import {n}`.
    pub import_action: String,
    /// `{n} records imported`.
    pub imported: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvImportIssues {
    pub not_number: String,
    pub not_integer: String,
    pub not_boolean: String,
    pub not_date: String,
    pub slash_date: String,
    pub not_time: String,
    pub not_date_time: String,
    pub not_enum: String,
    pub not_member: String,
    pub extra_cells: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CsvImportTestIds {
    pub button: String,
    pub dialog: String,
    pub text: String,
    pub file: String,
    pub map: String,
    pub author: String,
    pub preview: String,
    pub error: String,
    pub confirm: String,
}

/// `csvImport` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvImportSpec {
    pub max_rows: usize,
    pub preview_rows: usize,
    pub issues_shown: usize,
    pub true_words: Vec<String>,
    pub false_words: Vec<String>,
    pub author_keys: Vec<String>,
    pub copy: CsvImportCopy,
    pub issues: CsvImportIssues,
    pub test_ids: CsvImportTestIds,
}

pub static APP_CSV_IMPORT: LazyLock<CsvImportSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(RECORDS_JSON).expect("app-records.json parses");
    serde_json::from_value(fixture["csvImport"].clone()).expect("csvImport parses")
});

/// One data row: `row` = its record number (header = 1, blank lines counted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvRow {
    pub row: usize,
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CsvTable {
    pub header: Vec<String>,
    pub rows: Vec<CsvRow>,
}

/// JS `String.prototype.trim` (Unicode whitespace + the BOM).
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// The delimiter: the most frequent of `,` `;` TAB in the first line
/// outside quotes (ties in that order, none = `,`).
fn csv_delimiter(text: &[char]) -> char {
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut quoted = false;
    for &ch in text {
        if ch == '"' {
            quoted = !quoted;
        } else if !quoted && (ch == '\n' || ch == '\r') {
            break;
        } else if !quoted {
            if let Some(c) = counts.iter_mut().find(|(d, _)| *d == ch) {
                c.1 += 1;
            }
        }
    }
    let mut best = counts[0];
    for c in counts {
        if c.1 > best.1 {
            best = c;
        }
    }
    best.0
}

/// RFC 4180 → the header + the data rows (`parseCases`).
pub fn csv_parse(input: &str) -> CsvTable {
    let text: Vec<char> = input.strip_prefix('\u{feff}').unwrap_or(input).chars().collect();
    let delimiter = csv_delimiter(&text);
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut i = 0;
    while i < text.len() {
        let ch = text[i];
        if quoted {
            if ch == '"' {
                if text.get(i + 1) == Some(&'"') {
                    field.push('"');
                    i += 2;
                    continue;
                }
                quoted = false;
            } else {
                field.push(ch);
            }
            i += 1;
            continue;
        }
        if ch == '"' {
            quoted = true;
        } else if ch == delimiter {
            record.push(std::mem::take(&mut field));
        } else if ch == '\r' || ch == '\n' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            if ch == '\r' && text.get(i + 1) == Some(&'\n') {
                i += 1;
            }
        } else {
            field.push(ch);
        }
        i += 1;
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    let mut numbered = records
        .into_iter()
        .enumerate()
        .map(|(index, cells)| CsvRow { row: index + 1, cells })
        .filter(|r| !(r.cells.len() == 1 && r.cells[0].is_empty()));
    let Some(first) = numbered.next() else {
        return CsvTable::default();
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    let header: Vec<String> = first
        .cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let trimmed = js_trim(cell);
            let name = if trimmed.is_empty() {
                APP_CSV_IMPORT.copy.column.replace("{n}", &(index + 1).to_string())
            } else {
                trimmed.to_string()
            };
            let n = seen.get(&name).copied().unwrap_or(0) + 1;
            seen.insert(name.clone(), n);
            if n == 1 { name } else { format!("{name} {n}") }
        })
        .collect();
    let rows = numbered
        .map(|mut r| {
            if r.cells.len() < header.len() {
                r.cells.resize(header.len(), String::new());
            }
            r
        })
        .collect();
    CsvTable { header, rows }
}

/// A name's matching key: lowercased, only ASCII a–z and 0–9.
pub fn csv_key(name: &str) -> String {
    name.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

/// The first pick per field (`mapCases`): equal keys first, then the ONE
/// free column containing the field's key; a column maps at most once.
pub fn csv_auto_map(fields: &[String], header: &[String]) -> BTreeMap<String, Option<usize>> {
    let keys: Vec<String> = header.iter().map(|h| csv_key(h)).collect();
    let mut taken = vec![false; keys.len()];
    let mut mapping: BTreeMap<String, Option<usize>> = BTreeMap::new();
    for field in fields {
        let key = csv_key(field);
        let index = if key.is_empty() { None } else { keys.iter().enumerate().position(|(i, k)| !taken[i] && *k == key) };
        if let Some(i) = index {
            taken[i] = true;
        }
        mapping.insert(field.clone(), index);
    }
    for field in fields {
        let key = csv_key(field);
        if mapping.get(field).copied().flatten().is_some() || key.is_empty() {
            continue;
        }
        let hits: Vec<usize> = keys.iter().enumerate().filter(|(i, k)| !taken[*i] && k.contains(&key)).map(|(i, _)| i).collect();
        if let [hit] = hits[..] {
            mapping.insert(field.clone(), Some(hit));
            taken[hit] = true;
        }
    }
    mapping
}

/// The owner's Author pick: the first column whose key is an `authorKeys`
/// entry, else `None` (= Me).
pub fn csv_author_column(header: &[String]) -> Option<usize> {
    header.iter().position(|name| APP_CSV_IMPORT.author_keys.contains(&csv_key(name)))
}

fn csv_issue(template: &str, value: &str, values: &str) -> String {
    template.replace("{value}", value).replace("{values}", values)
}

fn pad2(s: &str) -> String {
    if s.len() == 1 { format!("0{s}") } else { s.to_string() }
}

/// `[0-9]{min,max}` (ASCII digits only).
fn digits(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// `^[0-9]{4}-[0-9]{2}-[0-9]{2}`, the 10-byte prefix of `s`.
fn iso_date_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
}

fn csv_number(text: &str, integer: bool) -> Result<Value, String> {
    let issues = &APP_CSV_IMPORT.issues;
    let compact = text.replace(' ', "");
    let body = compact.strip_prefix(['+', '-']).unwrap_or(&compact);
    let shape = body.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') && body.chars().any(|c| c.is_ascii_digit());
    if !shape {
        return Err(csv_issue(&issues.not_number, text, ""));
    }
    let normal = match (compact.rfind('.'), compact.rfind(',')) {
        (Some(dot), Some(comma)) => {
            let (decimal, group) = if dot > comma { ('.', ',') } else { (',', '.') };
            compact.replace(group, "").replacen(decimal, ".", 1)
        }
        (Some(_), None) | (None, Some(_)) => {
            let sep = if compact.contains('.') { '.' } else { ',' };
            let parts: Vec<&str> = compact.split(sep).collect();
            if parts.len() == 2 { parts.join(".") } else { parts.join("") }
        }
        (None, None) => compact.clone(),
    };
    let unsigned = normal.strip_prefix(['+', '-']).unwrap_or(&normal);
    let valid = match unsigned.split_once('.') {
        Some((int, frac)) => digits(int, 1, usize::MAX) && digits(frac, 1, usize::MAX),
        None => digits(unsigned, 1, usize::MAX),
    };
    let Some(n) = valid.then(|| normal.parse::<f64>().ok()).flatten().filter(|n| n.is_finite()) else {
        return Err(csv_issue(&issues.not_number, text, ""));
    };
    if integer && n.fract() != 0.0 {
        return Err(csv_issue(&issues.not_integer, text, ""));
    }
    Ok(number_value(n))
}

fn csv_scalar(ty: &str, enum_values: Option<&[String]>, format: Option<&str>, text: &str) -> Result<Value, String> {
    let spec = &*APP_CSV_IMPORT;
    let issues = &spec.issues;
    if ty == "number" || ty == "integer" {
        return csv_number(text, ty == "integer");
    }
    if ty == "boolean" {
        let lower = text.to_lowercase();
        if spec.true_words.contains(&lower) {
            return Ok(Value::Bool(true));
        }
        if spec.false_words.contains(&lower) {
            return Ok(Value::Bool(false));
        }
        return Err(csv_issue(&issues.not_boolean, text, ""));
    }
    if let Some(values) = enum_values {
        let lower = text.to_lowercase();
        return match values.iter().find(|v| v.to_lowercase() == lower) {
            Some(hit) => Ok(Value::String(hit.clone())),
            None => Err(csv_issue(&issues.not_enum, text, &values.join(", "))),
        };
    }
    match format {
        Some("date") => {
            if text.len() == 10 && iso_date_prefix(text) {
                return Ok(Value::String(text.to_string()));
            }
            let parts: Vec<&str> = text.split('.').collect();
            if let [d, m, y] = parts[..] {
                if digits(d, 1, 2) && digits(m, 1, 2) && digits(y, 4, 4) {
                    return Ok(Value::String(format!("{y}-{}-{}", pad2(m), pad2(d))));
                }
            }
            let template = if text.contains('/') { &issues.slash_date } else { &issues.not_date };
            Err(csv_issue(template, text, ""))
        }
        Some("time") => {
            let parts: Vec<&str> = text.split(':').collect();
            let ok = match parts[..] {
                [h, m] => digits(h, 1, 2) && digits(m, 2, 2),
                [h, m, s] => digits(h, 1, 2) && digits(m, 2, 2) && digits(s, 2, 2),
                _ => false,
            };
            if !ok {
                return Err(csv_issue(&issues.not_time, text, ""));
            }
            let seconds = parts.get(2).map(|s| format!(":{s}")).unwrap_or_default();
            Ok(Value::String(format!("{}:{}{seconds}", pad2(parts[0]), parts[1])))
        }
        Some("date-time") => {
            if iso_date_prefix(text) && text.as_bytes().get(10) == Some(&b'T') {
                Ok(Value::String(text.to_string()))
            } else {
                Err(csv_issue(&issues.not_date_time, text, ""))
            }
        }
        _ => Ok(Value::String(text.to_string())),
    }
}

/// One cell → the field's value (`cellCases`): `Ok(Null)` = empty (no
/// value), `Err` = the issue's message.
pub fn csv_cell(field: &DataField, cell: &str) -> Result<Value, String> {
    let text = js_trim(cell);
    if text.is_empty() {
        return Ok(Value::Null);
    }
    if field.ty != "array" {
        return csv_scalar(&field.ty, field.enum_values.as_deref(), field.format.as_deref(), text);
    }
    let sep = if text.contains(';') { ';' } else { ',' };
    let items: Vec<&str> = text.split(sep).map(js_trim).filter(|item| !item.is_empty()).collect();
    if items.is_empty() {
        return Ok(Value::Null);
    }
    let item_type = field.items.as_ref().map(|i| i.ty.as_str()).filter(|t| !t.is_empty()).unwrap_or("string");
    let values = items.into_iter().map(|item| csv_scalar(item_type, None, None, item)).collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(values))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvMember {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvAuthor {
    /// The Author column (`None` = every row is the importer's).
    pub column: Option<usize>,
    pub importer: String,
    pub members: Vec<CsvMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsvRecord {
    pub row: usize,
    pub data: Map<String, Value>,
    /// A member's id when the row is someone else's (an on-behalf write).
    pub author: Option<String>,
    pub issues: Vec<String>,
}

/// The rows → records + their issues (`rowsCases`), before the host's
/// schema check. A field missing from `mapping` = skipped.
pub fn csv_records(
    schema: &DataSchema,
    header: &[String],
    rows: &[CsvRow],
    mapping: &BTreeMap<String, Option<usize>>,
    author: Option<&CsvAuthor>,
) -> Vec<CsvRecord> {
    let spec = &*APP_CSV_IMPORT;
    let columns = data_columns(schema);
    rows.iter()
        .map(|CsvRow { row, cells }| {
            let row_text = row.to_string();
            let at = |field: &str, message: &str| {
                spec.copy.issue.replace("{row}", &row_text).replace("{field}", field).replace("{message}", message)
            };
            let mut issues: Vec<String> = Vec::new();
            if cells.len() > header.len() {
                let message = spec.issues.extra_cells.replace("{n}", &cells.len().to_string()).replace("{m}", &header.len().to_string());
                issues.push(spec.copy.row_issue.replace("{row}", &row_text).replace("{message}", &message));
            }
            let mut data = Map::new();
            for name in &columns {
                let Some(column) = mapping.get(name).copied().flatten() else { continue };
                let field = &schema.properties[name];
                match csv_cell(field, cells.get(column).map(String::as_str).unwrap_or("")) {
                    Err(message) => issues.push(at(name, &message)),
                    Ok(Value::Null) => {}
                    Ok(value) => {
                        data.insert(name.clone(), value);
                    }
                }
            }
            let mut record_author = None;
            if let Some(author) = author {
                if let Some(column) = author.column {
                    let text = js_trim(cells.get(column).map(String::as_str).unwrap_or(""));
                    let id: Option<&str> = if text.is_empty() {
                        Some(&author.importer)
                    } else {
                        let lower = text.to_lowercase();
                        let by_email = author.members.iter().find(|m| m.email.to_lowercase() == lower);
                        let by_name: Vec<&CsvMember> = author.members.iter().filter(|m| m.name.to_lowercase() == lower).collect();
                        by_email.map(|m| m.id.as_str()).or(if by_name.len() == 1 { Some(by_name[0].id.as_str()) } else { None })
                    };
                    match id {
                        None => issues.push(at(&DATA_TAB.copy.author, &spec.issues.not_member.replace("{value}", text))),
                        Some(id) if id != author.importer => record_author = Some(id.to_string()),
                        Some(_) => {}
                    }
                }
            }
            CsvRecord { row: *row, data, author: record_author, issues }
        })
        .collect()
}

/// The preview's count line (`copy.ready`).
pub fn csv_summary(records: &[CsvRecord]) -> String {
    let ready = records.iter().filter(|r| r.issues.is_empty()).count();
    APP_CSV_IMPORT.copy.ready.replace("{n}", &ready.to_string()).replace("{total}", &records.len().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outbox_cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let all = fixture["outbox"][key].as_array().unwrap().clone();
        assert!(!all.is_empty(), "{key}");
        all
    }

    fn outbox_entry(v: &Value) -> OutboxEntry {
        serde_json::from_value(v.clone()).unwrap()
    }

    fn outbox_entries(v: &Value) -> Vec<OutboxEntry> {
        v.as_array().unwrap().iter().map(outbox_entry).collect()
    }

    fn outbox_rows_of(v: &Value) -> Vec<OutboxRow> {
        serde_json::from_value(v.clone()).unwrap()
    }

    /// VAPP-235: `outbox` spec + `writesCases`.
    #[test]
    fn outbox_writes_cases_replay_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let o = &fixture["outbox"];
        assert_eq!(o["ackGraceSeconds"].as_u64().unwrap(), APP_RECORDS_OUTBOX.ack_grace_seconds);
        assert_eq!(o["copy"]["pendingOne"], APP_RECORDS_OUTBOX.copy.pending_one.as_str());
        for case in outbox_cases("writesCases") {
            let call = OutboxCall::parse(case["call"].as_str().unwrap()).unwrap();
            let new_ids: Vec<String> = serde_json::from_value(case["newIds"].clone()).unwrap();
            let got = outbox_writes(call, &case["args"], &new_ids);
            assert_eq!(serde_json::to_value(&got).unwrap(), case["writes"], "{}", case["name"]);
            assert!(outbox_new_id_count(call, &case["args"]) >= new_ids.len(), "{}", case["name"]);
        }
    }

    #[test]
    fn outbox_request_cases_replay_the_fixture() {
        for case in outbox_cases("requestCases") {
            let (call, input) = outbox_request(&outbox_entry(&case["entry"]));
            assert_eq!(json!({"call": call.as_str(), "input": input}), case["request"], "{}", case["name"]);
        }
    }

    #[test]
    fn outbox_rows_cases_replay_the_fixture() {
        for case in outbox_cases("rowsCases") {
            let got = outbox_rows(&outbox_rows_of(&case["rows"]), &outbox_entries(&case["entries"]), case["me"].as_str());
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn outbox_outcome_cases_replay_the_fixture() {
        for case in outbox_cases("outcomeCases") {
            let got = outbox_outcome(&outbox_entry(&case["entry"]), &OutboxResult::from_value(&case["result"]));
            assert_eq!(got.as_str(), case["outcome"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn outbox_flush_cases_replay_the_fixture() {
        for case in outbox_cases("flushCases") {
            let results: HashMap<String, OutboxResult> = case["results"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), OutboxResult::from_value(v)))
                .collect();
            let got = outbox_flush(&outbox_entries(&case["entries"]), &results, case["now"].as_str().unwrap());
            let name = case["name"].as_str().unwrap();
            assert_eq!(json!(got.sent), case["sent"], "{name} sent");
            let refused: Vec<Value> = got.refused.iter().map(|(w, verb)| json!({"writeId": w, "verb": verb})).collect();
            assert_eq!(Value::Array(refused), case["refused"], "{name} refused");
            assert_eq!(serde_json::to_value(&got.after).unwrap(), case["after"], "{name} after");
        }
    }

    #[test]
    fn outbox_prune_cases_replay_the_fixture() {
        for case in outbox_cases("pruneCases") {
            let kept = outbox_prune(&outbox_entries(&case["entries"]), &outbox_rows_of(&case["rows"]), case["now"].as_str().unwrap());
            let ids: Vec<&str> = kept.iter().map(|e| e.write_id.as_str()).collect();
            assert_eq!(json!(ids), case["kept"], "{}", case["name"]);
        }
    }

    #[test]
    fn outbox_pending_cases_replay_the_fixture() {
        for case in outbox_cases("pendingCases") {
            let got = outbox_pending_label(&outbox_entries(&case["entries"]), case["appId"].as_str().unwrap());
            assert_eq!(got.as_deref(), case["label"].as_str(), "{}", case["name"]);
        }
    }

    fn cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        fixture[key].as_array().unwrap().clone()
    }

    /// VAPP-187: `ownership.canChangeCases`.
    #[test]
    fn can_change_cases_replay_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let ownership = &fixture["ownership"];
        assert_eq!(ownership["refusal"], APP_RECORDS.ownership.refusal.as_str());
        let all = ownership["canChangeCases"].as_array().unwrap();
        assert!(!all.is_empty());
        for case in all {
            let name = case["name"].as_str().unwrap();
            for (op, key) in [(RecordOp::Update, "update"), (RecordOp::Delete, "delete")] {
                let got = can_change_record(
                    case["author"].as_str(),
                    case["createdBy"].as_str(),
                    case["viewer"].as_str(),
                    case["role"].as_str(),
                    op,
                );
                assert_eq!(got, case["expected"][key].as_bool().unwrap(), "{name} {key}");
            }
        }
    }

    #[test]
    fn count_label_cases_replay_the_fixture() {
        for case in cases("countLabelCases") {
            let label = records_count_label(case["count"].as_u64().unwrap(), case["max"].as_u64());
            assert_eq!(label, case["label"].as_str().unwrap());
        }
    }

    #[test]
    fn data_tab_spec_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let tab = &fixture["dataTab"];
        let copy: Value = tab["copy"].clone();
        assert_eq!(copy["addRecord"], DATA_TAB.copy.add_record.as_str());
        assert_eq!(copy["listHint"], DATA_TAB.copy.list_hint.as_str());
        assert_eq!(copy["file"], DATA_TAB.copy.file.as_str());
        assert_eq!(copy["files"], DATA_TAB.copy.files.as_str());
        let columns = &tab["columns"];
        assert_eq!(columns["byWidth"].as_f64().unwrap() as f32, DATA_TAB.columns.by_width);
        assert_eq!(columns["updatedWidth"].as_f64().unwrap() as f32, DATA_TAB.columns.updated_width);
        assert_eq!(columns["menuWidth"].as_f64().unwrap() as f32, DATA_TAB.columns.menu_width);
        assert_eq!(columns["minWidth"].as_f64().unwrap() as f32, DATA_TAB.columns.min_width);
        assert_eq!(columns["gap"].as_f64().unwrap() as f32, DATA_TAB.columns.gap);
        let c = &DATA_TAB.columns;
        assert_eq!(table_min_width(&[]), c.by_width + c.updated_width + c.menu_width + 2. * c.gap);
        assert_eq!(table_min_width(&[Some(10.), None]) - table_min_width(&[]), 10. + c.min_width + 2. * c.gap);
        let widths = columns["typeWidths"].as_object().unwrap();
        assert_eq!(widths.len(), DATA_TAB.columns.type_widths.len());
        for (key, px) in widths {
            assert_eq!(DATA_TAB.columns.type_widths.get(key).copied(), Some(px.as_f64().unwrap() as f32), "{key}");
        }
        // `columnWidth`: by format for a string, by type otherwise.
        let field = |v: Value| serde_json::from_value::<DataField>(v).unwrap();
        let w = |key: &str| DATA_TAB.columns.type_widths.get(key).copied();
        assert_eq!(column_width(&field(json!({"type": "string", "format": "date"}))), w("date"));
        assert_eq!(column_width(&field(json!({"type": "string", "format": "date-time"}))), w("date-time"));
        assert_eq!(column_width(&field(json!({"type": "string", "format": "time"}))), w("time"));
        assert_eq!(column_width(&field(json!({"type": "number"}))), w("number"));
        assert_eq!(column_width(&field(json!({"type": "integer"}))), w("integer"));
        assert_eq!(column_width(&field(json!({"type": "boolean"}))), w("boolean"));
        assert_eq!(column_width(&field(json!({"type": "string"}))), None);
        assert_eq!(column_width(&field(json!({"type": "string", "enum": ["a"]}))), None);
        assert_eq!(column_width(&field(json!({"type": "array", "items": {"type": "number"}}))), None);
        assert_eq!(tab["testIds"]["row"], DATA_TAB.test_ids.row.as_str());
        assert_eq!(DATA_TAB.placeholders.get("date").map(String::as_str), Some("YYYY-MM-DD"));
        assert_eq!(kind_option("entry", 2), "entry · 2");
    }

    fn data_cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let all = fixture["dataTab"][key].as_array().unwrap().clone();
        assert!(!all.is_empty(), "{key}");
        all
    }

    #[test]
    fn kind_cases_replay_the_fixture() {
        for case in data_cases("kindCases") {
            let manifest = (!case["manifest"].is_null()).then(|| &case["manifest"]);
            let rows = case["rows"].as_array().unwrap();
            let got = data_kinds(manifest, rows.iter().map(|r| r["kind"].as_str().unwrap()));
            let got: Vec<Value> = got.into_iter().map(|(kind, count)| json!({"kind": kind, "count": count})).collect();
            assert_eq!(Value::Array(got), case["kinds"], "{case}");
        }
    }

    #[test]
    fn column_cases_replay_the_fixture() {
        for case in data_cases("columnCases") {
            let got = data_columns(&DataSchema::from_value(&case["schema"]));
            assert_eq!(json!(got), case["columns"], "{case}");
        }
    }

    #[test]
    fn rows_cases_replay_the_fixture() {
        let null = Value::Null;
        for case in data_cases("rowsCases") {
            let rows: Vec<RecordRowInput> = case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| RecordRowInput {
                    id: r["id"].as_str().unwrap(),
                    kind: r["kind"].as_str().unwrap(),
                    data: &null,
                    created_by: None,
                    author_id: None,
                    created_at: "",
                    updated_at: r["updatedAt"].as_str().unwrap(),
                })
                .collect();
            let got: Vec<&str> = data_rows(&rows, case["kind"].as_str().unwrap()).iter().map(|r| r.id).collect();
            assert_eq!(json!(got), case["order"], "{case}");
        }
    }

    #[test]
    fn cell_cases_replay_the_fixture() {
        for case in data_cases("cellCases") {
            assert_eq!(cell_text(case.get("value")), case["text"].as_str().unwrap(), "{case}");
        }
        // VAPP-221: the new file cases are in the fixture.
        assert!(data_cases("cellCases").iter().any(|c| c["text"] == "2 files"));
        // A list INPUT never counts files: the plain list.
        let ref_ = json!(["/api/attachments/0f6e2c1a-8b3d-4c5e-9a7f-1b2c3d4e5f60"]);
        assert_eq!(form_input(DataControl::List, Some(&ref_)), FormInput::Text("/api/attachments/0f6e2c1a-8b3d-4c5e-9a7f-1b2c3d4e5f60".into()));
        // A whole float (a host's JSON layer) still reads `3`.
        assert_eq!(cell_text(Some(&json!(3.0))), "3");
    }

    #[test]
    fn form_cases_replay_the_fixture() {
        for case in data_cases("formCases") {
            let got: Vec<Value> = form_fields(&DataSchema::from_value(&case["schema"]))
                .into_iter()
                .map(|f| {
                    let mut o = json!({"name": f.name, "control": f.control.as_str(), "required": f.required});
                    if let Some(p) = f.placeholder {
                        o["placeholder"] = json!(p);
                    }
                    if let Some(opts) = f.options {
                        o["options"] = json!(opts);
                    }
                    if let Some(t) = f.item_type {
                        o["itemType"] = json!(t);
                    }
                    o
                })
                .collect();
            assert_eq!(Value::Array(got), case["fields"], "{case}");
        }
    }

    #[test]
    fn input_cases_replay_the_fixture() {
        for case in data_cases("inputCases") {
            let control = DataControl::parse(case["control"].as_str().unwrap()).unwrap();
            let want = match &case["input"] {
                Value::Bool(b) => FormInput::Bool(*b),
                other => FormInput::Text(other.as_str().unwrap().to_string()),
            };
            assert_eq!(form_input(control, case.get("value")), want, "{case}");
        }
    }

    /// VAPP-188: `onBehalf.writeAsCases` + `importCases`.
    #[test]
    fn on_behalf_cases_replay_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let on_behalf = &fixture["onBehalf"];
        assert_eq!(on_behalf["refusal"], APP_RECORDS.on_behalf.refusal.as_str());
        assert_eq!(on_behalf["notMember"], APP_RECORDS.on_behalf.not_member.as_str());
        let members = |case: &Value| -> Vec<String> {
            case["members"].as_array().unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect()
        };
        let write_as = on_behalf["writeAsCases"].as_array().unwrap();
        assert!(!write_as.is_empty());
        for case in write_as {
            let members = members(case);
            let members: Vec<&str> = members.iter().map(String::as_str).collect();
            let got = write_as_verdict(case["viewer"].as_str(), case["role"].as_str(), case["author"].as_str(), &members);
            assert_eq!(got.as_str(), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
        let imports = on_behalf["importCases"].as_array().unwrap();
        assert!(!imports.is_empty());
        for case in imports {
            let members = members(case);
            let members: Vec<&str> = members.iter().map(String::as_str).collect();
            let got = import_author(case["importer"].as_str().unwrap(), case["role"].as_str(), case["author"].as_str(), &members);
            assert_eq!(got, case["expected"].as_str(), "{}", case["name"]);
        }
    }

    /// VAPP-188: `dataTab.bylineCases`.
    #[test]
    fn byline_cases_replay_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        assert_eq!(fixture["dataTab"]["copy"]["author"], DATA_TAB.copy.author.as_str());
        assert_eq!(fixture["dataTab"]["copy"]["byFor"], DATA_TAB.copy.by_for.as_str());
        for case in data_cases("bylineCases") {
            let names = case["names"].as_object().unwrap();
            let got = record_byline(
                case["authorId"].as_str(),
                case["createdBy"].as_str(),
                |id| names.get(id).and_then(Value::as_str).map(str::to_string),
                case["fallback"].as_str().unwrap(),
            );
            assert_eq!(got, case["text"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    /// VAPP-188: `dataTab.authorOptionCases`.
    #[test]
    fn author_option_cases_replay_the_fixture() {
        for case in data_cases("authorOptionCases") {
            let users = case["users"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| (u["id"].as_str().unwrap().to_string(), u["name"].as_str().unwrap().to_string()))
                .collect();
            let got: Vec<Value> = author_options(users).into_iter().map(|(id, _)| json!(id)).collect();
            assert_eq!(Value::Array(got), case["order"], "{case}");
        }
    }

    /// VAPP-188: `dataTab.formAuthorCases`.
    #[test]
    fn form_author_cases_replay_the_fixture() {
        for case in data_cases("formAuthorCases") {
            let record = &case["record"];
            let record = (!record.is_null()).then(|| (record["authorId"].as_str(), record["createdBy"].as_str()));
            let got = form_author(case["viewer"].as_str(), record, case["picked"].as_str());
            assert_eq!(got.default, case["default"].as_str(), "{} default", case["name"]);
            assert_eq!(got.send, case["send"].as_str(), "{} send", case["name"]);
        }
    }

    fn history_cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let all = fixture["history"][key].as_array().unwrap().clone();
        assert!(!all.is_empty(), "{key}");
        all
    }

    fn event_core(v: &Value) -> RecordEventCore {
        RecordEventCore {
            op: RecordEventOp::parse(v["op"].as_str().unwrap()).unwrap(),
            before: v["before"].as_object().cloned(),
            patch: v["patch"].as_object().cloned(),
        }
    }

    /// VAPP-215: `history` spec + `stateCases`.
    #[test]
    fn history_state_cases_replay_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let h = &fixture["history"];
        assert_eq!(h["undoSeconds"].as_u64().unwrap(), APP_RECORD_HISTORY.undo_seconds);
        assert_eq!(h["maxFields"].as_u64().unwrap() as usize, APP_RECORD_HISTORY.max_fields);
        assert_eq!(h["copy"]["undo"], APP_RECORD_HISTORY.copy.undo.as_str());
        assert_eq!(fixture["dataTab"]["testIds"]["historyRow"], DATA_TAB.test_ids.history_row.as_str());
        assert_eq!(undo_title(UndoableWrite::Create), h["copy"]["undoCreate"].as_str().unwrap());
        assert_eq!(undo_title(UndoableWrite::Update), h["copy"]["undoUpdate"].as_str().unwrap());
        assert_eq!(undo_title(UndoableWrite::Delete), h["copy"]["undoDelete"].as_str().unwrap());
        assert_eq!(undo_title(UndoableWrite::Batch), h["copy"]["undoBatch"].as_str().unwrap());
        for case in history_cases("stateCases") {
            let got = event_state(&event_core(&case["event"])).map(Value::Object).unwrap_or(Value::Null);
            assert!(json_equal(&got, &case["state"]), "{}", case["name"]);
        }
    }

    #[test]
    fn history_inverse_cases_replay_the_fixture() {
        for case in history_cases("inverseCases") {
            let got = match inverse_write(&event_core(&case["event"])) {
                InverseWrite::Delete => json!({"op": "delete"}),
                InverseWrite::Restore(data) => json!({"op": "restore", "data": data}),
            };
            assert!(json_equal(&got, &case["inverse"]), "{}", case["name"]);
        }
    }

    #[test]
    fn history_label_cases_replay_the_fixture() {
        for case in history_cases("labelCases") {
            assert_eq!(event_label(&event_core(&case["event"])), case["label"].as_str().unwrap(), "{}", case["name"]);
        }
        assert!(json_equal(&json!(2), &json!(2.0)));
        assert!(!json_equal(&json!([1, 2]), &json!([2, 1])));
    }

    #[test]
    fn history_rows_cases_replay_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        // `history_cases` refuses an empty list; `rowsCases` must have cases.
        let all = fixture["history"]["rowsCases"].as_array().unwrap();
        assert!(!all.is_empty());
        for case in all {
            let names = case["names"].as_object().unwrap();
            let events: Vec<RecordEvent> =
                case["events"].as_array().unwrap().iter().map(|e| RecordEvent::from_value(e).unwrap()).collect();
            let got: Vec<Value> = history_rows(
                &events,
                |id| names.get(id).and_then(Value::as_str).map(str::to_string),
                case["fallback"].as_str().unwrap(),
            )
            .into_iter()
            .map(|r| json!({"id": r.id, "label": r.label, "who": r.who, "at": r.at, "restorable": r.restorable}))
            .collect();
            assert_eq!(Value::Array(got), case["rows"], "{}", case["name"]);
        }
    }

    #[test]
    fn parse_cases_replay_the_fixture() {
        for case in data_cases("parseCases") {
            let schema = DataSchema::from_value(&case["schema"]);
            let inputs: HashMap<String, FormInput> = case["inputs"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| {
                    let input = match v {
                        Value::Bool(b) => FormInput::Bool(*b),
                        other => FormInput::Text(other.as_str().unwrap().to_string()),
                    };
                    (k.clone(), input)
                })
                .collect();
            let got = form_data(&schema, &inputs, case["editing"].as_bool().unwrap());
            assert_eq!(Value::Object(got), case["data"], "{case}");
        }
    }

    fn csv_cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let all = fixture["csvImport"][key].as_array().unwrap().clone();
        assert!(!all.is_empty(), "{key}");
        all
    }

    fn strings(v: &Value) -> Vec<String> {
        v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
    }

    fn mapping_of(v: &Value) -> BTreeMap<String, Option<usize>> {
        v.as_object().unwrap().iter().map(|(k, i)| (k.clone(), i.as_u64().map(|i| i as usize))).collect()
    }

    fn rows_of(v: &Value) -> Vec<CsvRow> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|r| CsvRow { row: r["row"].as_u64().unwrap() as usize, cells: strings(&r["cells"]) })
            .collect()
    }

    /// VAPP-228: `csvImport` minus its cases.
    #[test]
    fn csv_import_spec_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        let c = &fixture["csvImport"];
        assert_eq!(c["maxRows"].as_u64().unwrap() as usize, APP_CSV_IMPORT.max_rows);
        assert_eq!(c["copy"]["importButton"], APP_CSV_IMPORT.copy.import_button.as_str());
        assert_eq!(c["issues"]["extraCells"], APP_CSV_IMPORT.issues.extra_cells.as_str());
        assert_eq!(c["testIds"]["confirm"], APP_CSV_IMPORT.test_ids.confirm.as_str());
    }

    #[test]
    fn csv_parse_cases_replay_the_fixture() {
        for case in csv_cases("parseCases") {
            let got = csv_parse(case["text"].as_str().unwrap());
            let rows: Vec<Value> = got.rows.iter().map(|r| json!({"row": r.row, "cells": r.cells})).collect();
            assert_eq!(json!(got.header), case["header"], "{}", case["name"]);
            assert_eq!(Value::Array(rows), case["rows"], "{}", case["name"]);
        }
    }

    #[test]
    fn csv_map_cases_replay_the_fixture() {
        for case in csv_cases("mapCases") {
            let got = csv_auto_map(&strings(&case["fields"]), &strings(&case["header"]));
            assert_eq!(json!(got), case["mapping"], "{}", case["name"]);
        }
    }

    #[test]
    fn csv_author_column_cases_replay_the_fixture() {
        for case in csv_cases("authorColumnCases") {
            let got = csv_author_column(&strings(&case["header"]));
            assert_eq!(json!(got), case["column"], "{case}");
        }
    }

    #[test]
    fn csv_cell_cases_replay_the_fixture() {
        for case in csv_cases("cellCases") {
            let field: DataField = serde_json::from_value(case["field"].clone()).unwrap();
            let got = csv_cell(&field, case["cell"].as_str().unwrap());
            let want = match case.get("issue") {
                Some(issue) => Err(issue.as_str().unwrap().to_string()),
                None => Ok(case["value"].clone()),
            };
            assert_eq!(got, want, "{}", case["name"]);
        }
    }

    #[test]
    fn csv_rows_cases_replay_the_fixture() {
        for case in csv_cases("rowsCases") {
            let author = case.get("author").map(|a| CsvAuthor {
                column: a["column"].as_u64().map(|c| c as usize),
                importer: a["importer"].as_str().unwrap().to_string(),
                members: a["members"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|m| CsvMember {
                        id: m["id"].as_str().unwrap().to_string(),
                        name: m["name"].as_str().unwrap().to_string(),
                        email: m["email"].as_str().unwrap().to_string(),
                    })
                    .collect(),
            });
            let got = csv_records(
                &DataSchema::from_value(&case["schema"]),
                &strings(&case["header"]),
                &rows_of(&case["rows"]),
                &mapping_of(&case["mapping"]),
                author.as_ref(),
            );
            let got: Vec<Value> = got
                .into_iter()
                .map(|r| {
                    let mut v = json!({"row": r.row, "data": r.data, "issues": r.issues});
                    if let Some(a) = r.author {
                        v["author"] = json!(a);
                    }
                    v
                })
                .collect();
            assert_eq!(Value::Array(got), case["records"], "{}", case["name"]);
        }
    }

    #[test]
    fn csv_summary_counts_the_rows_without_issues() {
        let record = |issues: Vec<String>| CsvRecord { row: 2, data: Map::new(), author: None, issues };
        assert_eq!(csv_summary(&[record(vec![]), record(vec!["x".into()]), record(vec![])]), "2 of 3 rows ready");
    }
}
