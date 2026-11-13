package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import com.exponential.app.domain.CiRun
import com.exponential.app.domain.RunnerSettings
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1617 / #EXP-1804: v125 adds `devices.runner` and the `ci_runs` +
 * `ci_jobs` tables (index names = the ones Room derives from the entities'
 * `@Index`, or the open fails validation); only the devices shape refetches.
 * Plus the wire decode of the two new shapes' rows and the runner column.
 */
class CiMigrationTest {
    private fun recording(statements: MutableList<String>): SupportSQLiteDatabase =
        Proxy.newProxyInstance(
            SupportSQLiteDatabase::class.java.classLoader,
            arrayOf(SupportSQLiteDatabase::class.java),
        ) { _, method, args ->
            if (method.name == "execSQL") {
                statements += args!![0] as String
                null
            } else {
                error("unexpected ${method.name}")
            }
        } as SupportSQLiteDatabase

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `v125 adds devices runner and the ci tables and refetches only devices`() {
        assertEquals(124, MIGRATION_124_125.startVersion)
        assertEquals(125, MIGRATION_124_125.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_124_125.migrate(recording(statements))
        assertEquals("ALTER TABLE `devices` ADD COLUMN `runner` TEXT", statements.first())
        assertTrue(statements.any { it.startsWith("CREATE TABLE IF NOT EXISTS `ci_runs`") })
        assertTrue(statements.any { it.startsWith("CREATE TABLE IF NOT EXISTS `ci_jobs`") })
        for (index in listOf("index_ci_runs_team_id", "index_ci_jobs_run_id", "index_ci_jobs_team_id", "index_ci_jobs_runner_device_id")) {
            assertTrue(index, statements.any { it.contains("`$index`") })
        }
        val refetches = statements.filter { it.contains("electric_offsets") }
        assertEquals(1, refetches.size)
        assertTrue(refetches.single().endsWith("WHERE `shape` = 'devices'"))
    }

    @Test
    fun `a ci_jobs row decodes with its jsonb steps and text integers`() {
        val row = json.decodeFromString(
            CiJobEntity.serializer(),
            """
            {"id": "j2", "team_id": "t1", "run_id": "r12", "key": "test", "check": "test (linux)",
             "position": "1", "runs_on": ["linux"], "needs": ["j1"], "matrix": {}, "timeout_min": 60,
             "status": "completed", "conclusion": "failure", "reason": null, "runner_device_id": "d1",
             "steps": [{"name": "Checkout", "status": "completed", "conclusion": "success", "startedAt": 1794564255000, "completedAt": 1794564259000}],
             "log_key": "ci-logs/j2", "log_bytes": "2048", "queued_at": "2026-11-13 10:00:00+00",
             "started_at": "2026-11-13 10:04:15+00", "completed_at": "2026-11-13 10:12:40+00",
             "created_at": "2026-11-13 10:00:00+00", "updated_at": "2026-11-13 10:12:40+00"}
            """.trimIndent(),
        )
        assertEquals(1, row.position)
        assertEquals(2048, row.logBytes)
        assertEquals(
            listOf(CiRun.Step("Checkout", "completed", "success", 1794564255000, 1794564259000)),
            CiRun.parseSteps(row.steps),
        )
    }

    @Test
    fun `a ci_runs row decodes with only its required columns`() {
        val row = json.decodeFromString(CiRunEntity.serializer(), """{"id": "r12", "number": 12, "state": "pending"}""")
        assertEquals(12, row.number)
        assertNull(row.error)
    }

    @Test
    fun `the devices runner column decodes and parses`() {
        val row = json.decodeFromString(
            DeviceEntity.serializer(),
            """{"id": "d1", "user_id": "u1", "device_id": "dev-1", "runner": {"enabled": true, "labels": ["gpu"], "slots": 2, "timeoutMin": 60}}""",
        )
        assertEquals(RunnerSettings.Settings(true, listOf("gpu"), 2, 60), RunnerSettings.parse(row.runner))
        val bare = json.decodeFromString(DeviceEntity.serializer(), """{"id": "d1", "user_id": "u1", "device_id": "dev-1"}""")
        assertNull(RunnerSettings.parse(bare.runner))
    }
}
