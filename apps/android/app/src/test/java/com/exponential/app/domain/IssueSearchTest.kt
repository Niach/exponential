package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-892: the issue-search engine, locked ×4 (web `issue-search.test.ts`, iOS
 * `IssueSearchTests`, desktop `domain::issue_search`) against the ONE contract
 * fixture — same cases, same test names.
 */
class IssueSearchTest {

    private data class TestRow(
        override val id: String,
        override val identifier: String,
        override val title: String,
        override val description: String? = null,
        override val createdAt: String? = null,
        override val updatedAt: String? = null,
        override val status: String? = null,
        override val creatorName: String? = null,
        override val creatorEmail: String? = null,
        override val creatorAgentName: String? = null,
        override val statusName: String? = null,
    ) : IssueSearch.Row

    private data class TestHit(
        override val id: String,
        val identifier: String,
        val title: String,
    ) : IssueSearch.Hit

    private data class FixtureCase(
        val name: String,
        val rows: List<TestRow>,
        val query: String,
        val exclude: Set<String>,
        val limit: Int,
        val serverHits: List<TestHit>?,
        val allowUnsynced: Boolean,
        /** #NEW-132: the clock relative ranges count back from (null = the real one). */
        val now: Long?,
        val expected: List<String>,
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(fixtureJson()).jsonArray.map { element ->
            val case = element.jsonObject
            fun str(obj: kotlinx.serialization.json.JsonObject, key: String): String? =
                obj[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                rows = case.getValue("rows").jsonArray.map { row ->
                    val obj = row.jsonObject
                    TestRow(
                        id = obj.getValue("id").jsonPrimitive.content,
                        identifier = obj.getValue("identifier").jsonPrimitive.content,
                        title = obj.getValue("title").jsonPrimitive.content,
                        description = str(obj, "description"),
                        createdAt = str(obj, "createdAt"),
                        updatedAt = str(obj, "updatedAt"),
                        status = str(obj, "status"),
                        creatorName = str(obj, "creatorName"),
                        creatorEmail = str(obj, "creatorEmail"),
                        creatorAgentName = str(obj, "creatorAgentName"),
                        statusName = str(obj, "statusName"),
                    )
                },
                query = case.getValue("query").jsonPrimitive.content,
                exclude = case["exclude"]?.jsonArray
                    ?.map { it.jsonPrimitive.content }
                    ?.toSet()
                    .orEmpty(),
                limit = case["limit"]?.jsonPrimitive?.int ?: IssueSearch.DEFAULT_LIMIT,
                serverHits = case["serverHits"]?.jsonArray?.map { hit ->
                    val obj = hit.jsonObject
                    TestHit(
                        id = obj.getValue("id").jsonPrimitive.content,
                        identifier = obj.getValue("identifier").jsonPrimitive.content,
                        title = obj.getValue("title").jsonPrimitive.content,
                    )
                },
                allowUnsynced = case["allowUnsynced"]?.jsonPrimitive?.boolean ?: false,
                now = str(case, "now")?.let { requireNotNull(IssueSearch.epochMs(it).takeIf { ms -> ms != Long.MIN_VALUE }) },
                expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
            )
        }

    @Test
    fun `every fixture case ranks and merges byte exact`() {
        val cases = cases()
        assertTrue(cases.size >= 12)
        for (case in cases) {
            val local = IssueSearch.rank(
                case.rows,
                case.query,
                limit = case.limit,
                exclude = case.exclude,
                now = case.now ?: System.currentTimeMillis(),
            )
            val byId = case.rows.associateBy { it.id }
            val hits = case.serverHits
            val merged = if (hits == null) {
                local
            } else {
                IssueSearch.mergeServerHits(
                    local,
                    hits,
                    limit = case.limit,
                    exclude = case.exclude,
                ) { hit ->
                    byId[hit.id] ?: if (case.allowUnsynced) {
                        TestRow(id = hit.id, identifier = hit.identifier, title = hit.title)
                    } else {
                        null
                    }
                }
            }
            assertEquals(case.name, case.expected, merged.map { it.id })
        }
    }

    @Test
    fun `the fixture covers every scored field and both merge modes`() {
        val names = cases().map { it.name }
        fun covers(needle: String) = names.any { it.contains(needle) }
        assertTrue(covers("empty query"))
        assertTrue(covers("leading hash"))
        assertTrue(covers("identifier prefix"))
        assertTrue(covers("title word prefix"))
        assertTrue(covers("every token must match"))
        assertTrue(covers("descriptions are searched"))
        assertTrue(covers("most recently updated"))
        assertTrue(covers("no match"))
        assertTrue(covers("limit caps the list"))
        assertTrue(covers("hash inside the query"))
        assertTrue(covers("by: token"))
        assertTrue(covers("is:closed"))
        assertTrue(covers("status: token"))
        assertTrue(covers("created:<n>d"))
        assertTrue(covers("D..D range"))
        assertTrue(cases().count { it.serverHits != null } >= 2)
        assertTrue(cases().any { it.allowUnsynced })
    }

    @Test
    fun `normalizes a query`() {
        assertEquals("exp-87", IssueSearch.normalizeQuery("  #EXP-87  "))
        assertEquals("", IssueSearch.normalizeQuery("#"))
    }

    @Test
    fun `tokenizes on whitespace and drops per token hashes`() {
        assertEquals(listOf("fix", "87", "login"), IssueSearch.tokens("Fix  #87 login"))
        assertEquals(emptyList<String>(), IssueSearch.tokens("#"))
    }

    @Test
    fun `parses by tokens into creator needles`() {
        assertEquals(
            IssueSearch.ParsedQuery(listOf("editor"), listOf("lukas", "triage")),
            IssueSearch.parseQuery("editor BY:lukas by:@triage by:"),
        )
    }

    @Test
    fun `parses created and updated ranges`() {
        val now = IssueSearch.epochMs("2026-11-14T12:00:00Z")
        val day = 86_400_000L
        assertEquals(IssueSearch.Range(now - 7 * day, Long.MAX_VALUE), IssueSearch.parseRange("1w", now))
        val nov3 = IssueSearch.epochMs("2026-11-03T00:00:00Z")
        assertEquals(IssueSearch.Range(nov3, nov3 + day), IssueSearch.parseRange("2026-11-03", now))
        assertEquals(IssueSearch.Range(nov3 + day, Long.MAX_VALUE), IssueSearch.parseRange(">2026-11-03", now))
        assertEquals(null, IssueSearch.parseRange("2026-02-30", now))
        assertEquals(null, IssueSearch.parseRange("0d", now))
    }

    @Test
    fun `is stable for equal keys`() {
        val rows = listOf(
            TestRow(id = "a", identifier = "X-1", title = "same"),
            TestRow(id = "b", identifier = "X-1", title = "same"),
        )
        assertEquals(listOf("a", "b"), IssueSearch.rank(rows, "same").map { it.id })
    }
}

/**
 * The contract fixture, located relative to the Gradle test working directory
 * (the `app` module dir) with fallbacks so the suite also runs from the
 * `apps/android` dir or the repo root.
 */
private fun fixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/issue-search.json",
        "../../packages/domain-contract/fixtures/issue-search.json",
        "packages/domain-contract/fixtures/issue-search.json",
    )
    val file = candidates.map(::File).firstOrNull { it.isFile }
        ?: error("issue-search.json not found from ${File(".").absolutePath}")
    return file.readText()
}
