package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-2171: v136 adds the `repositories` table (the 34th shape; the index
 * name = the one Room derives from the entity's `@Index`), nothing refetches.
 * Plus the wire decode of a shape row (jsonb `sync`, pg boolean, text real).
 */
class RepositoryShapeTest {
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
    fun `v136 adds the repositories table and refetches nothing`() {
        assertEquals(135, MIGRATION_135_136.startVersion)
        assertEquals(136, MIGRATION_135_136.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_135_136.migrate(recording(statements))
        assertTrue(statements.first().startsWith("CREATE TABLE IF NOT EXISTS `repositories`"))
        for (column in listOf("team_id", "provider", "full_name", "slug", "description", "sync", "default_branch",
            "default_branch_override", "private", "inaccessible_at", "shared_by_user_id", "sort_order", "created_at", "updated_at")) {
            assertTrue(column, statements.first().contains("`$column`"))
        }
        assertTrue(statements.any { it.contains("`index_repositories_team_id`") })
        assertTrue(statements.none { it.contains("electric_offsets") })
    }

    @Test
    fun `a repositories row decodes`() {
        val row = json.decodeFromString(
            RepositoryEntity.serializer(),
            """
            {"id": "r1", "team_id": "t1", "provider": "ecode", "full_name": "acme/engine", "slug": "engine",
             "description": null, "sync": {"mode": "mirror", "url": "https://github.com/acme/engine.git"},
             "default_branch": "main", "default_branch_override": null, "private": "t", "inaccessible_at": null,
             "shared_by_user_id": "u1", "sort_order": "1.5", "created_at": "2026-11-26 10:00:00+00",
             "updated_at": "2026-11-26 10:00:00+00"}
            """.trimIndent(),
        )
        assertEquals("engine", row.name)
        assertEquals("engine", row.slug)
        assertTrue(row.private)
        assertEquals(1.5, row.sortOrder, 0.0)
        assertTrue(row.sync!!.contains("\"mirror\""))
        val bare = json.decodeFromString(RepositoryEntity.serializer(), """{"id": "r2"}""")
        assertNull(bare.sync)
        assertEquals("github", bare.provider)
    }
}
