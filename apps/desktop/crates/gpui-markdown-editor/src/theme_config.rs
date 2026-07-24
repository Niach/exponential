//! Theme-pack parsing and resolution for the reusable editor component.
//!
//! The runtime consumes a complete [`Theme`]. Configuration files may provide
//! only a patch, which is filtered against the runtime token schema and merged
//! once before rendering.

use std::path::Path;

use anyhow::{Context as _, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::{Map, Value};

use crate::theme::Theme;

const VELOTYPE_THEME_ID: &str = "velotype";
const VELOTYPE_LIGHT_THEME_ID: &str = "velotype-light";

/// Built-in base themes understood by Velotype-compatible theme packs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkdownEditorBuiltinTheme {
    #[default]
    Velotype,
    VelotypeLight,
}

impl MarkdownEditorBuiltinTheme {
    /// Stable identifier used by Velotype theme packs.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Velotype => VELOTYPE_THEME_ID,
            Self::VelotypeLight => VELOTYPE_LIGHT_THEME_ID,
        }
    }

    /// Resolves the complete runtime theme.
    pub fn resolve(self) -> Theme {
        match self {
            Self::Velotype => Theme::default_theme(),
            Self::VelotypeLight => Theme::light_theme(),
        }
    }

    /// Parses a stable Velotype theme identifier.
    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            VELOTYPE_THEME_ID => Some(Self::Velotype),
            VELOTYPE_LIGHT_THEME_ID => Some(Self::VelotypeLight),
            _ => None,
        }
    }
}

/// A partial set of theme tokens.
///
/// Missing, `null`, and empty-string values inherit from the base theme.
/// Unknown fields are retained for serialization but ignored during resolution,
/// allowing this component to consume broader Velotype application themes.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownEditorThemePatch {
    value: Value,
}

impl Default for MarkdownEditorThemePatch {
    fn default() -> Self {
        Self {
            value: Value::Object(Map::new()),
        }
    }
}

impl MarkdownEditorThemePatch {
    /// Parses a strict JSON token patch such as `{ "colors": { ... } }`.
    pub fn from_json(source: &str) -> anyhow::Result<Self> {
        Self::from_value(serde_json::from_str(source)?)
    }

    /// Parses a JSONC token patch. Both line and block comments are accepted.
    pub fn from_jsonc(source: &str) -> anyhow::Result<Self> {
        Self::from_value(parse_jsonc_value(source)?)
    }

    /// Validates and normalizes a JSON value into a token patch.
    pub fn from_value(mut value: Value) -> anyhow::Result<Self> {
        if !value.is_object() {
            bail!("theme patch must be a JSON object");
        }
        prune_empty_json_values(&mut value);
        Ok(Self { value })
    }

    /// Returns the normalized patch as JSON data.
    pub fn to_value(&self) -> Value {
        self.value.clone()
    }

    /// Serializes this patch as strict, pretty-printed JSON.
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Applies the patch to a complete caller-provided base theme.
    ///
    /// This operation performs all optional-field handling up front. The
    /// returned runtime theme contains no patch lookups in render hot paths.
    pub fn apply_to(&self, base: &Theme) -> anyhow::Result<Theme> {
        let mut merged = serde_json::to_value(base)?;
        let mut patch = filter_json_by_schema(&self.value, &merged);
        if let Value::Object(object) = &mut patch {
            // A patch changes tokens, not the identity of its base theme.
            object.remove("name");
        }
        merge_non_empty_json_values(&mut merged, &patch);
        serde_json::from_value(merged).context("failed to construct resolved editor theme")
    }
}

impl Serialize for MarkdownEditorThemePatch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MarkdownEditorThemePatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(D::Error::custom)
    }
}

/// Metadata and partial tokens for a Velotype-compatible theme pack.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MarkdownEditorThemePack {
    pub name: String,
    pub creator: String,
    pub base_theme_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default)]
    pub theme: MarkdownEditorThemePatch,
}

#[derive(Deserialize)]
struct RawThemePack {
    name: String,
    creator: String,
    #[serde(default)]
    base_theme_id: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    theme: MarkdownEditorThemePatch,
}

impl MarkdownEditorThemePack {
    /// Parses a strict JSON theme pack using `velotype` as the fallback base.
    pub fn from_json(source: &str) -> anyhow::Result<Self> {
        Self::from_json_with_default_base(source, MarkdownEditorBuiltinTheme::Velotype)
    }

    /// Parses a strict JSON theme pack with a caller-selected fallback base.
    pub fn from_json_with_default_base(
        source: &str,
        default_base: MarkdownEditorBuiltinTheme,
    ) -> anyhow::Result<Self> {
        Self::from_value_with_default_base(serde_json::from_str(source)?, default_base)
    }

    /// Parses a JSONC theme pack using `velotype` as the fallback base.
    pub fn from_jsonc(source: &str) -> anyhow::Result<Self> {
        Self::from_jsonc_with_default_base(source, MarkdownEditorBuiltinTheme::Velotype)
    }

    /// Parses a JSONC theme pack with a caller-selected fallback base.
    pub fn from_jsonc_with_default_base(
        source: &str,
        default_base: MarkdownEditorBuiltinTheme,
    ) -> anyhow::Result<Self> {
        Self::from_value_with_default_base(parse_jsonc_value(source)?, default_base)
    }

    /// Loads `.json` and `.jsonc` theme packs from disk.
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::from_file_with_default_base(path, MarkdownEditorBuiltinTheme::Velotype)
    }

    /// Loads a theme pack with a caller-selected fallback base.
    pub fn from_file_with_default_base(
        path: impl AsRef<Path>,
        default_base: MarkdownEditorBuiltinTheme,
    ) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read theme pack '{}'", path.display()))?;
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("json") => Self::from_json_with_default_base(&source, default_base),
            Some("jsonc") => Self::from_jsonc_with_default_base(&source, default_base),
            _ => bail!("theme packs must use the .json or .jsonc extension"),
        }
    }

    /// Parses already-decoded JSON using `velotype` as the fallback base.
    pub fn from_value(value: Value) -> anyhow::Result<Self> {
        Self::from_value_with_default_base(value, MarkdownEditorBuiltinTheme::Velotype)
    }

    /// Parses JSON data with a caller-selected fallback base.
    pub fn from_value_with_default_base(
        value: Value,
        default_base: MarkdownEditorBuiltinTheme,
    ) -> anyhow::Result<Self> {
        let raw: RawThemePack = serde_json::from_value(value)?;
        let name = required_non_empty(raw.name, "name")?;
        let creator = required_non_empty(raw.creator, "creator")?;
        let base = raw
            .base_theme_id
            .as_deref()
            .and_then(MarkdownEditorBuiltinTheme::from_id)
            .unwrap_or(default_base);
        Ok(Self {
            name,
            creator,
            base_theme_id: base.id().to_string(),
            description: normalize_optional(raw.description),
            version: normalize_optional(raw.version),
            homepage: normalize_optional(raw.homepage),
            license: normalize_optional(raw.license),
            theme: raw.theme,
        })
    }

    /// Returns the normalized built-in base selected by this pack.
    pub fn base_theme(&self) -> MarkdownEditorBuiltinTheme {
        MarkdownEditorBuiltinTheme::from_id(&self.base_theme_id).unwrap_or_default()
    }

    /// Resolves this pack against its selected built-in base theme.
    pub fn resolve(&self) -> anyhow::Result<Theme> {
        self.resolve_against(&self.base_theme().resolve())
    }

    /// Resolves this pack against a complete caller-provided base theme.
    pub fn resolve_against(&self, base: &Theme) -> anyhow::Result<Theme> {
        let mut theme = self.theme.apply_to(base)?;
        theme.name.clone_from(&self.name);
        Ok(theme)
    }

    /// Serializes the normalized pack as strict, pretty-printed JSON.
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

impl<'de> Deserialize<'de> for MarkdownEditorThemePack {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(D::Error::custom)
    }
}

fn required_non_empty(value: String, field: &str) -> anyhow::Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("field '{field}' must be a non-empty string");
    }
    Ok(value.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn parse_jsonc_value(source: &str) -> anyhow::Result<Value> {
    Ok(serde_json::from_str(&strip_jsonc_comments(source)?)?)
}

fn strip_jsonc_comments(input: &str) -> anyhow::Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut closed = false;
                    let mut previous = '\0';
                    for next in chars.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                        }
                        if previous == '*' && next == '/' {
                            closed = true;
                            break;
                        }
                        previous = next;
                    }
                    if !closed {
                        bail!("unterminated block comment in JSONC theme pack");
                    }
                    continue;
                }
                _ => {}
            }
        }

        output.push(ch);
    }

    Ok(output)
}

fn prune_empty_json_values(value: &mut Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => {
            items.retain_mut(|item| !prune_empty_json_values(item));
            items.is_empty()
        }
        Value::Object(object) => {
            object.retain(|_, item| !prune_empty_json_values(item));
            object.is_empty()
        }
        Value::Bool(_) | Value::Number(_) => false,
    }
}

fn merge_non_empty_json_values(base: &mut Value, patch: &Value) {
    if is_empty_json_value(patch) {
        return;
    }
    match (base, patch) {
        (Value::Object(base_object), Value::Object(patch_object)) => {
            for (key, patch_value) in patch_object {
                if is_empty_json_value(patch_value) {
                    continue;
                }
                match base_object.get_mut(key) {
                    Some(base_value) => merge_non_empty_json_values(base_value, patch_value),
                    None => {
                        base_object.insert(key.clone(), patch_value.clone());
                    }
                }
            }
        }
        (base_value, patch_value) => *base_value = patch_value.clone(),
    }
}

fn is_empty_json_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.iter().all(is_empty_json_value),
        Value::Object(object) => object.values().all(is_empty_json_value),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

fn filter_json_by_schema(value: &Value, schema: &Value) -> Value {
    match (value, schema) {
        (Value::Object(value_object), Value::Object(schema_object)) => {
            let mut filtered = Map::new();
            for (key, value) in value_object {
                if let Some(schema_value) = schema_object.get(key) {
                    filtered.insert(key.clone(), filter_json_by_schema(value, schema_value));
                }
            }
            Value::Object(filtered)
        }
        (value, _) => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{MarkdownEditorBuiltinTheme, MarkdownEditorThemePack, MarkdownEditorThemePatch};
    use crate::theme::Theme;

    #[test]
    fn jsonc_pack_preserves_comment_markers_inside_strings() {
        let pack = MarkdownEditorThemePack::from_jsonc(
            r#"{
                // Theme metadata
                "name": "Ocean",
                "creator": "Example",
                "homepage": "https://example.com/theme//docs",
                "base_theme_id": "velotype-light",
                /* Only changed tokens are required. */
                "theme": { "dimensions": { "editor_padding": 42.0 } }
            }"#,
        )
        .expect("JSONC theme pack should parse");

        assert_eq!(pack.name, "Ocean");
        assert_eq!(
            pack.homepage.as_deref(),
            Some("https://example.com/theme//docs")
        );
        assert_eq!(pack.base_theme(), MarkdownEditorBuiltinTheme::VelotypeLight);
        assert_eq!(pack.resolve().unwrap().dimensions.editor_padding, 42.0);
    }

    #[test]
    fn partial_pack_inherits_selected_base_and_ignores_unknown_tokens() {
        let light = Theme::light_theme();
        let pack = MarkdownEditorThemePack::from_value(json!({
            "name": "Readable Light",
            "creator": "Example",
            "base_theme_id": "velotype-light",
            "theme": {
                "dimensions": {
                    "editor_padding": 33.0,
                    "not_a_real_token": 999
                },
                "placeholders": { "empty_editing": null },
                "unknown_group": { "value": true }
            }
        }))
        .unwrap();

        let resolved = pack.resolve().unwrap();
        assert_eq!(resolved.name, "Readable Light");
        assert_eq!(resolved.dimensions.editor_padding, 33.0);
        assert_eq!(
            resolved.colors.editor_background,
            light.colors.editor_background
        );
        assert_eq!(
            resolved.placeholders.empty_editing,
            light.placeholders.empty_editing
        );
    }

    #[test]
    fn missing_or_invalid_base_uses_caller_selected_fallback() {
        let source = r#"{
            "name": "Fallback",
            "creator": "Example",
            "base_theme_id": "unknown",
            "theme": {}
        }"#;
        let pack = MarkdownEditorThemePack::from_json_with_default_base(
            source,
            MarkdownEditorBuiltinTheme::VelotypeLight,
        )
        .unwrap();

        assert_eq!(pack.base_theme_id, "velotype-light");
        assert_eq!(
            pack.resolve().unwrap().colors.editor_background,
            Theme::light_theme().colors.editor_background
        );
    }

    #[test]
    fn direct_patch_applies_to_caller_theme_without_changing_its_name() {
        let mut base = Theme::default_theme();
        base.name = "Host Theme".into();
        let patch = MarkdownEditorThemePatch::from_json(
            r#"{
                "name": "Ignored Patch Name",
                "typography": { "text_size": 19.0 }
            }"#,
        )
        .unwrap();

        let resolved = patch.apply_to(&base).unwrap();
        assert_eq!(resolved.name, "Host Theme");
        assert_eq!(resolved.typography.text_size, 19.0);
    }

    #[test]
    fn normalized_pack_round_trips_as_strict_json() {
        let pack = MarkdownEditorThemePack::from_json(
            r#"{
                "name": "Minimal",
                "creator": "Example",
                "description": "",
                "theme": { "dimensions": { "editor_padding": 17.0 } }
            }"#,
        )
        .unwrap();
        let reparsed = MarkdownEditorThemePack::from_json(&pack.to_json().unwrap()).unwrap();
        assert_eq!(reparsed, pack);
        assert_eq!(reparsed.description, None);
    }

    #[test]
    fn unterminated_jsonc_block_comment_is_rejected() {
        assert!(MarkdownEditorThemePack::from_jsonc("{/* never closed").is_err());
    }
}
