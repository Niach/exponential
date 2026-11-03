import Foundation
import ExponentialUICore

/// eApps R8 (VAPP-233): declarative record migrations, the core's one rule
/// (`@exponential_app/eui/host` record-migrations, eui-core
/// `host::record_migrations`, `fixtures/record-migrations.json`).
public enum RecordMigrations {
    /// A record written under `version` as `kind` through `package`'s
    /// migrations: `{kind, data, applied}`, or `{error}` (the record left as it was).
    public static func migrate(package: JSONValue, version: String, kind: String, data: JSONValue, now: Double = 0) -> JSONValue {
        guard let out = try? migrateRecordJson(packageJson: package.json, version: version, kind: kind, dataJson: data.json, now: now) else {
            return .object(["error": .string("migration failed")])
        }
        return JSONValue.parse(out)
    }

    /// Why `after` may not replace the published `before` (an uncovered
    /// incompatible major bump), nil = it may.
    public static func refusal(before: JSONValue, after: JSONValue) -> String? {
        guard let out = try? migrationRefusalJson(beforeJson: before.json, afterJson: after.json) else { return nil }
        return out
    }
}
