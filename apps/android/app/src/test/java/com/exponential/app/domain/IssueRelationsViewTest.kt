package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1097: the relations view model + its copy, fixture-locked ×4 (web
 * `issue-relations-view.test.ts`, iOS `IssueRelationsViewTests`, desktop
 * `domain::relations_view`) against the ONE contract fixture — same copy
 * table, same named cases (each case's `name` is the assertion message here).
 * Change one, change all four.
 */
class IssueRelationsViewTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("issue-relations-view.json")).jsonObject

    private fun JsonElement?.stringOrNull(): String? =
        if (this == null || this is JsonNull) null else jsonPrimitive.content

    private fun keys(element: JsonElement?): Set<IssueRelationsView.BandKey> =
        element?.takeUnless { it is JsonNull }?.jsonArray.orEmpty()
            .map { key -> IssueRelationsView.BandKey.fromWire(key.jsonPrimitive.content)!! }
            .toSet()

    private fun input(obj: JsonObject) = IssueRelationsView.Input(
        subjectId = obj.getValue("subjectId").jsonPrimitive.content,
        relations = obj.getValue("relations").jsonArray.map { relation ->
            val r = relation.jsonObject
            IssueRelationsView.Relation(
                type = r.getValue("type").jsonPrimitive.content,
                issueId = r.getValue("issueId").jsonPrimitive.content,
                relatedIssueId = r.getValue("relatedIssueId").jsonPrimitive.content,
            )
        },
        issues = obj.getValue("issues").jsonArray.map { issue ->
            val i = issue.jsonObject
            IssueRelationsView.Issue(
                id = i.getValue("id").jsonPrimitive.content,
                identifier = i.getValue("identifier").jsonPrimitive.content,
                title = i.getValue("title").jsonPrimitive.content,
                status = i.getValue("status").jsonPrimitive.content,
            )
        },
        toggled = keys(obj["toggled"]),
        showAll = keys(obj["showAll"]),
    )

    private fun row(element: JsonElement): IssueRelationsView.Row {
        val r = element.jsonObject
        return IssueRelationsView.Row(
            id = r.getValue("id").jsonPrimitive.content,
            identifier = r.getValue("identifier").jsonPrimitive.content,
            title = r.getValue("title").jsonPrimitive.content,
            status = r.getValue("status").jsonPrimitive.content,
            open = r.getValue("open").jsonPrimitive.boolean,
        )
    }

    private fun expected(obj: JsonObject): IssueRelationsView.View {
        val sub = obj.getValue("subIssues").jsonObject
        return IssueRelationsView.View(
            parent = obj["parent"]?.takeUnless { it is JsonNull }?.let(::row),
            subIssues = IssueRelationsView.SubIssues(
                rows = sub.getValue("rows").jsonArray.map(::row),
                done = sub.getValue("done").jsonPrimitive.int,
                total = sub.getValue("total").jsonPrimitive.int,
                progress = sub["progress"].stringOrNull(),
            ),
            bands = obj.getValue("bands").jsonArray.map { band ->
                val b = band.jsonObject
                IssueRelationsView.Band(
                    key = IssueRelationsView.BandKey.fromWire(b.getValue("key").jsonPrimitive.content)!!,
                    title = b.getValue("title").jsonPrimitive.content,
                    count = b.getValue("count").jsonPrimitive.int,
                    openCount = b.getValue("openCount").jsonPrimitive.int,
                    expanded = b.getValue("expanded").jsonPrimitive.boolean,
                    rows = b.getValue("rows").jsonArray.map(::row),
                    more = b["more"].stringOrNull(),
                    less = b["less"].stringOrNull(),
                )
            },
            relationCount = obj.getValue("relationCount").jsonPrimitive.int,
        )
    }

    @Test
    fun locksTheCopyTable() {
        val copy = fixture.getValue("copy").jsonObject.mapValues { it.value.jsonPrimitive.content }
        val ours = mapOf(
            "subIssues" to IssueRelationsView.Copy.SUB_ISSUES,
            "subIssueOf" to IssueRelationsView.Copy.SUB_ISSUE_OF,
            "addSubIssues" to IssueRelationsView.Copy.ADD_SUB_ISSUES,
            "relations" to IssueRelationsView.Copy.RELATIONS,
            "add" to IssueRelationsView.Copy.ADD,
            "blockedBy" to IssueRelationsView.Copy.BLOCKED_BY,
            "blocking" to IssueRelationsView.Copy.BLOCKING,
            "duplicateOf" to IssueRelationsView.Copy.DUPLICATE_OF,
            "duplicatedBy" to IssueRelationsView.Copy.DUPLICATED_BY,
            "related" to IssueRelationsView.Copy.RELATED,
            "showLess" to IssueRelationsView.Copy.SHOW_LESS,
            "showMore(4)" to IssueRelationsView.showMore(4),
            "progress(2,5)" to IssueRelationsView.progress(2, 5),
            "cap" to IssueRelationsView.BAND_CAP.toString(),
        )
        assertEquals(copy, ours)
    }

    @Test
    fun matchesEveryFixtureCase() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue("the fixture has cases", cases.isNotEmpty())
        cases.forEach { case ->
            val obj = case.jsonObject
            val name = obj.getValue("name").jsonPrimitive.content
            assertEquals(
                name,
                expected(obj.getValue("expected").jsonObject),
                IssueRelationsView.build(input(obj.getValue("input").jsonObject)),
            )
        }
    }
}
