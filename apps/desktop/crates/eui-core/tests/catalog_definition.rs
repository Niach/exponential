//! Round 9 §11: the structural checker over the generated catalog files and
//! the vendored 1.0 basic catalog (`src/schema.test.ts`).

use eui_core::catalog::check_catalog_definition;
use serde_json::{json, Value};

const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui");

fn read(rel: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{PKG}/{rel}")).unwrap()).unwrap()
}

#[test]
fn the_generated_catalogs_conform() {
    for file in ["catalog/core.schema.json", "catalog/core-lite.schema.json"] {
        assert_eq!(check_catalog_definition(&read(file), true), Vec::<String>::new(), "{file}");
    }
}

#[test]
fn the_basic_catalog_conforms_in_upstreams_style_and_only_fails_flatness() {
    let basic = read("vendor/a2ui/v1_0/catalogs/basic/v1/catalog.json");
    assert_eq!(check_catalog_definition(&basic, false), Vec::<String>::new());
    let issues = check_catalog_definition(&basic, true);
    assert!(!issues.is_empty() && issues.iter().all(|i| i.contains("allOf") || i.contains("/properties")), "{issues:?}");
}

#[test]
fn a_broken_definition_names_every_rule() {
    let mut bad = read("catalog/core.schema.json");
    bad["liteCatalogId"] = json!("x");
    bad["protocolVersion"] = json!("0.9");
    bad["components"]["Button"]["oneOf"] = json!([]);
    bad["components"]["Text"]["properties"]["component"] = json!({"const": "Txt"});
    let percent = bad["functions"]["percent"].clone();
    bad["functions"]["app_toast"] = percent.clone();
    bad["functions"]["app_toast"]["properties"] = json!({"@call": {"const": "app_toast"}});
    bad["functions"]["a__b"] = percent.clone();
    bad["functions"]["a__b"]["properties"] = json!({"@call": {"const": "a__b"}});
    bad["functions"]["percent"]["unevaluatedProperties"] = json!(false);
    bad["$defs"]["Style"] = json!({"$ref": "https://example.com/x.json"});
    let issues = check_catalog_definition(&bad, true);
    for needle in ["root key liteCatalogId", "/protocolVersion", "no root oneOf/anyOf", "/components/Text/properties/component", "reserved host", "contains __", "unevaluatedProperties", "not an allowed target"] {
        assert!(issues.iter().any(|i| i.contains(needle)), "{needle}: {issues:?}");
    }
}
