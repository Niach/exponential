package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** [RepoPage] replays `fixtures/repo-page.json` (`treeCases`, `branchCases`,
 *  `rollupCases`, `tabCases`), comparing the WHOLE result; the copy and the
 *  tabs equal the fixture's. */
class RepoPageTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("repo-page.json")).jsonObject
    }

    private fun JsonElement.strOrNull(): String? = (this as? JsonPrimitive)?.takeUnless { this is JsonNull }?.content

    private fun pr(o: JsonObject) = RepoPage.Pr(
        id = o.getValue("id").jsonPrimitive.content,
        number = o.getValue("number").jsonPrimitive.int,
        title = o["title"]?.strOrNull(),
        head = o.getValue("head").jsonPrimitive.content,
        base = o.getValue("base").jsonPrimitive.content,
        state = o.getValue("state").jsonPrimitive.content,
        checksState = o.getValue("checksState").jsonPrimitive.content,
    )

    private fun prs(c: JsonObject) = c.getValue("prs").jsonArray.map { pr(it.jsonObject) }

    private fun nullable(value: String?): JsonElement = value?.let { JsonPrimitive(it) } ?: JsonNull

    private fun cases(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    @Test
    fun `tree cases`() {
        val all = cases("treeCases")
        assertTrue(all.isNotEmpty())
        for (c in all) {
            val actual = JsonArray(
                RepoPage.repoPrTree(prs(c)).map { row ->
                    buildJsonObject {
                        put("id", row.id)
                        put("number", row.number)
                        put("identifier", row.identifier)
                        put("title", row.title)
                        put("depth", row.depth)
                        put("subtitle", row.subtitle)
                        put("draft", row.draft)
                        put("checks", nullable(row.checks))
                    }
                },
            )
            assertEquals(c.getValue("name").jsonPrimitive.content, c.getValue("expected"), actual)
        }
    }

    @Test
    fun `branch cases`() {
        val all = cases("branchCases")
        assertTrue(all.isNotEmpty())
        for (c in all) {
            val branches = c.getValue("branches").jsonArray.map {
                it.jsonObject.getValue("name").jsonPrimitive.content to it.jsonObject.getValue("sha").jsonPrimitive.content
            }
            val actual = JsonArray(
                RepoPage.repoBranches(branches, c.getValue("defaultBranch").jsonPrimitive.content, prs(c)).map { row ->
                    buildJsonObject {
                        put("name", row.name)
                        put("sha", row.sha)
                        put("isDefault", row.isDefault)
                        put("prNumber", row.prNumber?.let { JsonPrimitive(it) } ?: JsonNull)
                    }
                },
            )
            assertEquals(c.getValue("name").jsonPrimitive.content, c.getValue("expected"), actual)
        }
    }

    @Test
    fun `rollup cases`() {
        val all = cases("rollupCases")
        assertTrue(all.isNotEmpty())
        for (c in all) {
            assertEquals(
                c.getValue("name").jsonPrimitive.content,
                c.getValue("expected").strOrNull(),
                RepoPage.repoChecksRollup(prs(c)),
            )
        }
    }

    @Test
    fun `tab cases`() {
        val all = cases("tabCases")
        assertTrue(all.isNotEmpty())
        for (c in all) {
            val param = c.getValue("param").strOrNull()
            assertEquals("tab $param", c.getValue("expected").jsonPrimitive.content, RepoPage.repoTab(param))
        }
    }

    @Test
    fun `tabs and copy match the fixture`() {
        assertEquals(
            fixture.getValue("tabs").jsonArray.map {
                it.jsonObject.getValue("id").jsonPrimitive.content to it.jsonObject.getValue("label").jsonPrimitive.content
            },
            RepoPage.TABS.map { it.id to it.label },
        )
        val copy = fixture.getValue("copy").jsonObject
        fun group(key: String) = copy.getValue(key).jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(mapOf("github" to RepoPage.Copy.Provider.GITHUB, "ecode" to RepoPage.Copy.Provider.ECODE), group("provider"))
        assertEquals(
            mapOf(
                "title" to RepoPage.Copy.Add.TITLE, "github" to RepoPage.Copy.Add.GITHUB, "ecode" to RepoPage.Copy.Add.ECODE, "name" to RepoPage.Copy.Add.NAME,
                "defaultBranch" to RepoPage.Copy.Add.DEFAULT_BRANCH, "mirror" to RepoPage.Copy.Add.MIRROR, "mirrorUrl" to RepoPage.Copy.Add.MIRROR_URL,
                "cancel" to RepoPage.Copy.Add.CANCEL, "create" to RepoPage.Copy.Add.CREATE,
            ),
            group("add"),
        )
        assertEquals(mapOf("empty" to RepoPage.Copy.Pulls.EMPTY), group("pulls"))
        assertEquals(mapOf("empty" to RepoPage.Copy.Runs.EMPTY), group("runs"))
        assertEquals(mapOf("empty" to RepoPage.Copy.Branches.EMPTY, "default" to RepoPage.Copy.Branches.DEFAULT), group("branches"))
        assertEquals(
            mapOf(
                "provider" to RepoPage.Copy.Settings.PROVIDER, "defaultBranch" to RepoPage.Copy.Settings.DEFAULT_BRANCH,
                "cloneUrl" to RepoPage.Copy.Settings.CLONE_URL, "backups" to RepoPage.Copy.Settings.BACKUPS,
                "noBackups" to RepoPage.Copy.Settings.NO_BACKUPS, "restore" to RepoPage.Copy.Settings.RESTORE,
                "restoreTitle" to RepoPage.Copy.Settings.RESTORE_TITLE, "restoreConfirm" to RepoPage.Copy.Settings.RESTORE_CONFIRM,
                "mirror" to RepoPage.Copy.Settings.MIRROR, "mirrorUrl" to RepoPage.Copy.Settings.MIRROR_URL,
            ),
            group("settings"),
        )
        assertEquals(
            copy.getValue("checks").jsonObject.mapValues { (_, v) ->
                RepoPage.Copy.Check(
                    v.jsonObject.getValue("label").jsonPrimitive.content,
                    v.jsonObject.getValue("tone").jsonPrimitive.content,
                )
            },
            RepoPage.Copy.CHECKS,
        )
    }
}
