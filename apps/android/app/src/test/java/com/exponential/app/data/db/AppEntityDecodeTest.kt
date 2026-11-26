package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Release train 2026-10-23: migration 0186 makes `apps.created_by` nullable
 * (ON DELETE SET NULL), and the apps shape sends every column, so a row can
 * arrive with `"created_by": null`. A decode failure drops the row forever.
 */
class AppEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private fun decode(row: String): AppEntity =
        json.decodeFromJsonElement(AppEntity.serializer(), json.parseToJsonElement(row) as JsonObject)

    @Test
    fun `a null created_by decodes to the empty string`() {
        val app = decode(
            """
            {
              "id": "app-1",
              "team_id": "team-1",
              "slug": "triage",
              "name": "Triage",
              "icon": null,
              "color": null,
              "description": "",
              "package": null,
              "head_version": "0",
              "created_by": null,
              "deleted_at": null,
              "created_at": "2026-10-23 10:00:00+00",
              "updated_at": "2026-10-23 10:00:00+00"
            }
            """.trimIndent(),
        )
        assertEquals("app-1", app.id)
        assertEquals("", app.createdBy)
    }

    @Test
    fun `a set created_by still decodes`() {
        val app = decode("""{ "id": "app-2", "created_by": "user-1" }""")
        assertEquals("user-1", app.createdBy)
    }

    @Test
    fun `granted_tools decodes to its JSON text, absent = none`() {
        // VAPP-120: the jsonb string[] arrives as a JSON value.
        val app = decode("""{ "id": "app-3", "granted_tools": ["exponential_issues_update"] }""")
        assertEquals("""["exponential_issues_update"]""", app.grantedTools)
        assertEquals("[]", decode("""{ "id": "app-4" }""").grantedTools)
    }

    @Test
    fun `published_version and draft_package decode, absent = never published`() {
        // eApps V1 (#VAPP-210): `package` = the published one, `draft_package`
        // = the head's while it is ahead (both jsonb, kept as JSON text).
        val app = decode(
            """{ "id": "app-5", "head_version": "5", "published_version": "3",
                 "package": {"id": "hours"}, "draft_package": {"id": "hours", "functions": []} }""",
        )
        assertEquals(3, app.publishedVersion)
        assertEquals("""{"id":"hours"}""", app.packageJson)
        assertEquals("""{"id":"hours","functions":[]}""", app.draftPackageJson)
        val bare = decode("""{ "id": "app-6", "draft_package": null }""")
        assertEquals(0, bare.publishedVersion)
        assertEquals(null, bare.draftPackageJson)
    }
}
