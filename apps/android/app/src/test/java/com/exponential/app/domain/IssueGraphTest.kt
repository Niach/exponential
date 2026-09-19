package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-980: the blocks-graph rule, locked ×4 (web `issue-graph.test.ts`, iOS
 * `IssueGraphTests`, desktop `domain::issue_graph`) against the ONE contract
 * fixture — same cases, same test names (each case's `name` is the assertion
 * message here, the way `IssueSearchTest` consumes its fixture).
 */
class IssueGraphTest {

    private data class FixtureCase(
        val name: String,
        val issues: List<IssueEntity>,
        val relations: List<IssueRelationEntity>,
        val expectedCounts: Map<String, IssueGraph.Counts>?,
        val picked: List<String>,
        val expectedSetBlockers: List<String>?,
        val subjects: List<String>,
        val expectedGraph: IssueGraph.Graph?,
    )

    private fun issueRow(id: String, identifier: String, status: String) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = status,
        priority = "none",
        sortOrder = 1.0,
        createdAt = "2026-09-19T10:00:00Z",
        updatedAt = "2026-09-19T10:00:00Z",
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(contractFixtureJson("issue-graph.json")).jsonArray
            .mapIndexed { index, element ->
                val case = element.jsonObject
                FixtureCase(
                    name = case.getValue("name").jsonPrimitive.content,
                    issues = case.getValue("issues").jsonArray.map { issue ->
                        val obj = issue.jsonObject
                        issueRow(
                            id = obj.getValue("id").jsonPrimitive.content,
                            identifier = obj.getValue("identifier").jsonPrimitive.content,
                            status = obj.getValue("status").jsonPrimitive.content,
                        )
                    },
                    relations = case.getValue("relations").jsonArray.mapIndexed { row, relation ->
                        val obj = relation.jsonObject
                        relationRow(
                            id = "r$index-$row",
                            type = obj.getValue("type").jsonPrimitive.content,
                            issueId = obj.getValue("issueId").jsonPrimitive.content,
                            relatedIssueId = obj.getValue("relatedIssueId").jsonPrimitive.content,
                        )
                    },
                    expectedCounts = case["expectedCounts"]?.jsonObject?.mapValues { (_, value) ->
                        val obj = value.jsonObject
                        IssueGraph.Counts(
                            blockedBy = obj.getValue("blockedBy").jsonPrimitive.int,
                            blocking = obj.getValue("blocking").jsonPrimitive.int,
                        )
                    },
                    picked = case["picked"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty(),
                    expectedSetBlockers = case["expectedSetBlockers"]?.jsonArray
                        ?.map { it.jsonPrimitive.content },
                    subjects = case["subjects"]?.jsonArray?.map { it.jsonPrimitive.content }
                        .orEmpty(),
                    expectedGraph = case["expectedGraph"]?.jsonObject?.let { graph ->
                        IssueGraph.Graph(
                            nodes = graph.getValue("nodes").jsonArray.map { node ->
                                val obj = node.jsonObject
                                IssueGraph.Node(
                                    id = obj.getValue("id").jsonPrimitive.content,
                                    wave = obj.getValue("wave").jsonPrimitive.int,
                                    lane = obj.getValue("lane").jsonPrimitive.int,
                                    subject = obj.getValue("subject").jsonPrimitive.boolean,
                                )
                            },
                            edges = graph.getValue("edges").jsonArray.map { edge ->
                                val obj = edge.jsonObject
                                IssueGraph.Edge(
                                    from = obj.getValue("from").jsonPrimitive.content,
                                    to = obj.getValue("to").jsonPrimitive.content,
                                    cycle = obj.getValue("cycle").jsonPrimitive.boolean,
                                )
                            },
                            hasCycle = graph.getValue("hasCycle").jsonPrimitive.boolean,
                            truncated = graph.getValue("truncated").jsonPrimitive.boolean,
                        )
                    },
                )
            }

    @Test
    fun `every fixture case counts and lays out byte exact`() {
        val cases = cases()
        assertTrue(cases.size >= 8)
        for (case in cases) {
            case.expectedCounts?.let { expected ->
                assertEquals(case.name, expected, IssueGraph.blockCounts(case.relations, case.issues))
            }
            case.expectedSetBlockers?.let { expected ->
                assertEquals(
                    case.name,
                    expected,
                    IssueGraph.openBlockersOfSet(case.picked, case.relations, case.issues)
                        .map { it.id },
                )
            }
            case.expectedGraph?.let { expected ->
                assertEquals(
                    case.name,
                    expected,
                    IssueGraph.blockGraph(case.subjects, case.relations, case.issues),
                )
            }
        }
    }

    @Test
    fun `the fixture covers the counts, the picked set and the layout`() {
        val cases = cases()
        val names = cases.map { it.name }
        fun covers(needle: String) = names.any { it.contains(needle) }
        assertTrue(covers("counts open blockers"))
        assertTrue(covers("closed or unsynced end"))
        assertTrue(covers("blockers outside it"))
        assertTrue(covers("one wave per link"))
        assertTrue(covers("diamond"))
        assertTrue(covers("finished blocker"))
        assertTrue(covers("cycle edges"))
        assertTrue(covers("several subjects"))
        assertTrue(cases.count { it.expectedGraph != null } >= 5)
    }

    @Test
    fun `names the side that has a count`() {
        assertEquals(
            "Blocked by 2",
            IssueGraph.blocksBadgeLabel(IssueGraph.Counts(blockedBy = 2, blocking = 0)),
        )
        assertEquals(
            "Blocking 1",
            IssueGraph.blocksBadgeLabel(IssueGraph.Counts(blockedBy = 0, blocking = 1)),
        )
        assertEquals(
            "Blocked by 2, blocking 1",
            IssueGraph.blocksBadgeLabel(IssueGraph.Counts(blockedBy = 2, blocking = 1)),
        )
    }

    @Test
    fun `cuts the closure at the node cap, nearest blockers first`() {
        val total = IssueGraph.MAX_NODES + 5
        val issues = (0 until total).map { i ->
            issueRow(id = "n$i", identifier = "EXP-${1000 + i}", status = "backlog")
        }
        // A chain n(total-1) → … → n1 → n0; the subject is the most blocked end.
        val relations = issues.drop(1).mapIndexed { i, issue ->
            relationRow(
                id = "r$i",
                type = DomainContract.issueRelationTypeBlocks,
                issueId = issue.id,
                relatedIssueId = "n$i",
            )
        }
        val graph = IssueGraph.blockGraph(listOf("n0"), relations, issues)
        assertTrue(graph.truncated)
        assertEquals(IssueGraph.MAX_NODES, graph.nodes.size)
        assertEquals(
            IssueGraph.Node(
                id = "n0",
                wave = IssueGraph.MAX_NODES - 1,
                lane = 0,
                subject = true,
            ),
            graph.nodes.last(),
        )
    }

    @Test
    fun `the graph notes are the shared ones`() {
        assertEquals("Showing the nearest 60 issues.", IssueGraph.TRUNCATED_NOTE)
        assertEquals("Red issues block each other in a cycle.", IssueGraph.CYCLE_NOTE)
    }
}
