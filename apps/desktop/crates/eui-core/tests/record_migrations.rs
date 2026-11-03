//! eApps R8 (VAPP-233): `fixtures/record-migrations.json` replayed through
//! the Rust core (the TS reference replays it in `src/host/record-migrations.test.ts`).

mod support;

use eui_core::host::{is_version_range, kind_changes_json, migrate_record, migration_refusal, package_migrations, range_holds, validate_migrations, RECORD_MIGRATIONS_MAX, RECORD_MIGRATION_OPS_MAX};
use serde_json::json;
use support::fixture;

#[test]
fn record_migrations_fixture() {
    let f = fixture("record-migrations.json");
    assert_eq!(f["limits"], json!({"migrations": RECORD_MIGRATIONS_MAX, "ops": RECORD_MIGRATION_OPS_MAX}));
    for c in f["rangeCases"].as_array().unwrap() {
        let (v, r) = (c["version"].as_str().unwrap(), c["range"].as_str().unwrap());
        assert_eq!(json!({"holds": range_holds(v, r), "range": is_version_range(r)}), c["expected"], "range: {v} {r}");
    }
    for c in f["migrateCases"].as_array().unwrap() {
        let got = migrate_record(&package_migrations(&c["package"]), c["version"].as_str().unwrap(), c["kind"].as_str().unwrap(), &c["data"], 0.0);
        assert!(eui_core::json::equal(&serde_json::to_value(got).unwrap(), &c["expected"]), "migrate: {}", c["name"]);
    }
    for c in f["validateCases"].as_array().unwrap() {
        assert_eq!(serde_json::to_value(validate_migrations(&c["package"])).unwrap(), c["expected"], "validate: {}", c["name"]);
    }
    for c in f["changeCases"].as_array().unwrap() {
        assert_eq!(kind_changes_json(&c["before"], &c["after"]), c["expected"], "changes: {}", c["name"]);
    }
    for c in f["refusalCases"].as_array().unwrap() {
        assert_eq!(json!(migration_refusal(&c["before"], &c["after"])), c["expected"], "refusal: {}", c["name"]);
    }
}
