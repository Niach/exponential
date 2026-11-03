package at.exponential.ui.host

import at.exponential.ui.ffi.migrateRecordJson
import at.exponential.ui.ffi.migrationRefusalJson
import at.exponential.ui.json.JsonValue

/**
 * eApps R8 (VAPP-233): declarative record migrations, the core's one rule
 * (`@exponential_app/eui/host` record-migrations, eui-core
 * `host::record_migrations`, `fixtures/record-migrations.json`).
 */
object RecordMigrations {
    /** A record written under [version] as [kind] through [pkg]'s migrations: `{kind, data, applied}` or `{error}`. */
    fun migrate(pkg: JsonValue, version: String, kind: String, data: JsonValue, now: Double = 0.0): JsonValue {
        val out = runCatching { migrateRecordJson(pkg.json, version, kind, data.json, now) }.getOrNull()
            ?: return JsonValue.Obj(mapOf("error" to JsonValue.Str("migration failed")))
        return JsonValue.parse(out)
    }

    /** Why [after] may not replace the published [before] (an uncovered incompatible major bump), null = it may. */
    fun refusal(before: JsonValue, after: JsonValue): String? = runCatching { migrationRefusalJson(before.json, after.json) }.getOrNull()
}
