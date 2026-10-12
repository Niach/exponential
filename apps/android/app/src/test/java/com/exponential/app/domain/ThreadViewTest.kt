package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1284: the thread view model is fixture-locked ×4 — every case of
 * `packages/domain-contract/fixtures/thread-view.json` runs here (web
 * `thread-view.test.ts`, desktop `domain::thread_view`, iOS
 * `ThreadViewTests`). The output is re-encoded in the fixture's own JSON
 * shape and compared as a WHOLE, so a field the port forgets fails too.
 */
class ThreadViewTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("thread-view.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.list(key: String): List<JsonObject> =
        this.getValue(key).jsonArray.map { it.jsonObject }

    private fun thread(json: JsonObject) = ThreadView.Thread(
        id = json.str("id")!!,
        title = json.str("title"),
        results = json["results"]?.takeUnless { it is JsonNull }?.toString(),
        updatedAt = json.str("updatedAt")!!,
    )

    private fun item(json: JsonObject) = ThreadView.Item(
        id = json.str("id")!!,
        kind = json.str("kind")!!,
        issueId = json.str("issueId"),
        sessionId = json.str("sessionId"),
        source = json.str("source")!!,
        addedBySessionId = json.str("addedBySessionId"),
        sortOrder = json.getValue("sortOrder").jsonPrimitive.double,
        createdAt = json.str("createdAt")!!,
    )

    private fun issue(json: JsonObject) = ThreadView.Issue(
        id = json.str("id")!!,
        identifier = json.str("identifier")!!,
        title = json.str("title")!!,
        createdAt = json.str("createdAt")!!,
    )

    private fun run(json: JsonObject) = ThreadView.Run(
        id = json.str("id")!!,
        name = json.str("name")!!,
        userId = json.str("userId")!!,
        deviceLabel = json.str("deviceLabel"),
        status = json.str("status")!!,
        needsInput = json.getValue("needsInput").jsonPrimitive.boolean,
        agentBusy = json.getValue("agentBusy").jsonPrimitive.boolean,
        issueId = json.str("issueId"),
        parentSessionId = json.str("parentSessionId"),
        resumedFromId = json.str("resumedFromId"),
        startedAt = json.str("startedAt")!!,
        endedAt = json.str("endedAt"),
    )

    private fun strings(values: List<String>) = JsonArray(values.map(::JsonPrimitive))

    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    private fun links(rows: List<ThreadView.LinkRow>) = JsonArray(
        rows.map {
            buildJsonObject {
                put("itemId", it.itemId)
                put("issueId", it.issueId)
                put("identifier", it.identifier)
                put("title", it.title)
                put("label", it.label)
            }
        },
    )

    private fun encode(view: ThreadView.View): JsonObject = buildJsonObject {
        put("title", view.title)
        put("titleDerived", view.titleDerived)
        put(
            "strip",
            JsonArray(
                view.strip.map { child ->
                    when (child) {
                        ThreadView.Child.Conversation -> buildJsonObject { put("kind", "conversation") }
                        is ThreadView.Child.IssueChild -> buildJsonObject {
                            put("kind", "issue")
                            put("issueId", child.issueId)
                            put("identifier", child.identifier)
                            put("title", child.title)
                        }
                        ThreadView.Child.Add -> buildJsonObject { put("kind", "add") }
                        is ThreadView.Child.Guide -> buildJsonObject {
                            put("kind", "guide")
                            put("dot", child.dot)
                        }
                    }
                },
            ),
        )
        put("headerCaption", view.headerCaption)
        put("listCaption", view.listCaption)
        put("live", view.live)
        put("needsInput", view.needsInput)
        put("busy", view.busy)
        put(
            "conversation",
            JsonArray(
                view.conversation.map { block ->
                    when (block) {
                        is ThreadView.RunBlock -> buildJsonObject {
                            put("kind", "run")
                            put("runId", block.runId)
                            put("label", block.label)
                            put("caption", strings(block.caption))
                            put("state", block.state.wire)
                            put("folded", block.folded)
                            put("links", links(block.links))
                        }
                        is ThreadView.ParallelBlock -> buildJsonObject {
                            put("kind", "parallel")
                            put("label", block.label)
                            put("caption", strings(block.caption))
                            put("state", block.state.wire)
                            put(
                                "runs",
                                JsonArray(
                                    block.runs.map { row ->
                                        buildJsonObject {
                                            put("runId", row.runId)
                                            put("name", row.name)
                                            put("issueId", nullable(row.issueId))
                                            put("deviceLabel", nullable(row.deviceLabel))
                                            put("state", row.state.wire)
                                            put("startedAt", row.startedAt)
                                        }
                                    },
                                ),
                            )
                            put("links", links(block.links))
                        }
                    }
                },
            ),
        )
        put("runIds", strings(view.runIds))
        put("issueIds", strings(view.issueIds))
    }

    @Test
    fun `every buildThreadView case matches the fixture`() {
        val cases = fixture.list("views")
        assertTrue(cases.size >= 6)
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val view = ThreadView.build(
                thread = thread(input.getValue("thread").jsonObject),
                items = input.list("items").map(::item),
                issues = input.list("issues").map(::issue),
                runs = input.list("runs").map(::run),
            )
            assertEquals(case.str("name"), case.getValue("expected"), encode(view))
        }
    }

    @Test
    fun `every partitionThreadList case matches the fixture`() {
        val cases = fixture.list("partitions")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val entries = case.getValue("input").jsonArray.map { it.jsonObject }.map {
                ThreadView.ListEntry(
                    threadId = it.str("threadId")!!,
                    updatedAt = it.str("updatedAt")!!,
                    mineLive = it.getValue("mineLive").jsonPrimitive.boolean,
                    value = it,
                )
            }
            val partition = ThreadView.partition(entries)
            val actual = buildJsonObject {
                put("running", JsonArray(partition.running.map { it.value }))
                put("recent", JsonArray(partition.recent.map { it.value }))
            }
            assertEquals(case.str("name"), case.getValue("expected"), actual)
        }
    }

    @Test
    fun `a Guide dot needs a non-empty results array`() {
        assertEquals(false, ThreadView.hasGuide(null))
        assertEquals(false, ThreadView.hasGuide("[]"))
        assertEquals(false, ThreadView.hasGuide("null"))
        assertEquals(true, ThreadView.hasGuide("""[{"topic":"Summary"}]"""))
    }
}
