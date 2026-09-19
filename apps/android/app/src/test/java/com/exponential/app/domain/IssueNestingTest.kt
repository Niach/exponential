package com.exponential.app.domain

import com.exponential.app.data.db.IssueRelationEntity
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-980: the list-nesting rule, locked ×4 (web `issue-nesting.test.ts`, iOS
 * `IssueNestingTests`, desktop `domain::issue_nesting`) against the ONE
 * contract fixture — same cases, same test names (each case's `name` is the
 * assertion message here, the way `IssueSearchTest` consumes its fixture).
 */
class IssueNestingTest {

    private data class FixtureCase(
        val name: String,
        val groups: List<List<String>>,
        val identifiers: Map<String, String>,
        val relations: List<IssueRelationEntity>,
        val expected: List<List<IssueNesting.Row>>,
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(nestingFixtureJson()).jsonArray.mapIndexed { index, element ->
            val case = element.jsonObject
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                groups = case.getValue("groups").jsonArray.map { group ->
                    group.jsonArray.map { it.jsonPrimitive.content }
                },
                identifiers = case.getValue("identifiers").jsonObject
                    .mapValues { (_, value) -> value.jsonPrimitive.content },
                relations = case.getValue("relations").jsonArray.mapIndexed { row, relation ->
                    val obj = relation.jsonObject
                    relationRow(
                        id = "r$index-$row",
                        type = obj.getValue("type").jsonPrimitive.content,
                        issueId = obj.getValue("issueId").jsonPrimitive.content,
                        relatedIssueId = obj.getValue("relatedIssueId").jsonPrimitive.content,
                    )
                },
                expected = case.getValue("expected").jsonArray.map { group ->
                    group.jsonArray.map { row ->
                        val obj = row.jsonObject
                        IssueNesting.Row(
                            id = obj.getValue("id").jsonPrimitive.content,
                            depth = obj.getValue("depth").jsonPrimitive.int,
                        )
                    }
                },
            )
        }

    @Test
    fun `every fixture case nests byte exact`() {
        val cases = cases()
        assertTrue(cases.size >= 10)
        for (case in cases) {
            assertEquals(
                case.name,
                case.expected,
                IssueNesting.nestIssueRows(case.groups, case.relations) {
                    case.identifiers[it] ?: it
                },
            )
        }
    }

    @Test
    fun `the fixture covers every nesting rule`() {
        val names = cases().map { it.name }
        fun covers(needle: String) = names.any { it.contains(needle) }
        assertTrue(covers("stays flat"))
        assertTrue(covers("leaves its own position"))
        assertTrue(covers("the root decides the group"))
        assertTrue(covers("comes back empty"))
        assertTrue(covers("siblings keep the list order"))
        assertTrue(covers("nest recursively"))
        assertTrue(covers("a parent outside the list"))
        assertTrue(covers("lowest identifier wins"))
        assertTrue(covers("parent cycle"))
        assertTrue(covers("self relation"))
    }
}

/** A relation row as the fixture describes it — only the three fields count. */
internal fun relationRow(
    id: String,
    type: String,
    issueId: String,
    relatedIssueId: String,
) = IssueRelationEntity(
    id = id,
    issueId = issueId,
    relatedIssueId = relatedIssueId,
    type = type,
    source = DomainContract.issueRelationSourceUser,
    teamId = "team-1",
    createdAt = "2026-09-19T10:00:00Z",
    updatedAt = "2026-09-19T10:00:00Z",
)

/**
 * The contract fixture, located relative to the Gradle test working directory
 * (the `app` module dir) with fallbacks so the suite also runs from the
 * `apps/android` dir or the repo root.
 */
private fun nestingFixtureJson(): String = contractFixtureJson("issue-nesting.json")

internal fun contractFixtureJson(name: String): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/$name",
        "../../packages/domain-contract/fixtures/$name",
        "packages/domain-contract/fixtures/$name",
    )
    val file = candidates.map(::File).firstOrNull { it.isFile }
        ?: error("$name not found from ${File(".").absolutePath}")
    return file.readText()
}
