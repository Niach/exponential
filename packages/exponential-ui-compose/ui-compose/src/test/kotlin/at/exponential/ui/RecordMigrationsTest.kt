package at.exponential.ui

import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.host.RecordMigrations
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * eApps R8 (VAPP-233): `fixtures/record-migrations.json` through
 * [RecordMigrations] (the core over the FFI). The TS reference, the Rust
 * core and Swift's `RecordMigrationsTests` replay the same cases.
 */
class RecordMigrationsTest {
    @Test
    fun fixture() {
        Fixtures.require()
        val f = Fixtures.json("record-migrations.json")
        val migrate = f["migrateCases"]?.array.orEmpty()
        assertFalse(migrate.isEmpty())
        for (c in migrate) {
            val got = RecordMigrations.migrate(c["package"] ?: JsonValue.Null, c["version"]?.string ?: "", c["kind"]?.string ?: "", c["data"] ?: JsonValue.Null)
            val want = c["expected"] ?: JsonValue.Null
            assertTrue("${c["name"]?.string}: differs at ${jsonDiff(got.json, want.json)}", jsonEqual(got.json, want.json))
        }
        for (c in f["refusalCases"]?.array.orEmpty()) {
            assertEquals(c["name"]?.string, c["expected"]?.string, RecordMigrations.refusal(c["before"] ?: JsonValue.Null, c["after"] ?: JsonValue.Null))
        }
    }
}
