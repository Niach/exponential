package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-2109: replays `sidebar-tree.json` `teams`, `teamItems`, `teamsMenu`,
 *  `phone` and the copy the phone shows (TS `sidebar-tree.test.ts`). */
class SidebarTreeTest {
    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("sidebar-tree.json")).jsonObject }

    private fun JsonObject.str(key: String): String? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.list(key: String): List<JsonObject> = getValue(key).jsonArray.map { it.jsonObject }

    private fun strings(element: JsonElement): List<String> = element.jsonArray.map { it.jsonPrimitive.content }

    private fun kind(wire: String) = SidebarTree.Kind.entries.first { it.wire == wire }

    private fun items(json: List<JsonObject>) = json.map { SidebarTree.Item(it.str("id")!!, kind(it.str("kind")!!)) }

    private fun entries(json: List<JsonObject>) = json.map {
        SidebarFolders.Entry(
            id = it.str("id")!!,
            kind = it.str("kind")!!,
            targetId = it.str("targetId"),
            parentId = it.str("parentId"),
            name = it.str("name"),
            sortOrder = it.getValue("sortOrder").jsonPrimitive.double,
        )
    }

    @Test
    fun `teams by name, never the personal one`() {
        for (case in fixture.list("teams")) {
            val actual = SidebarTree.teams(case.list("teams").map { SidebarTree.Team(it.str("id")!!, it.str("name")!!, it.str("kind")!!) })
            assertEquals(case.str("name"), strings(case.getValue("expected")), actual)
        }
        assertEquals(strings(fixture.getValue("teamsMenu")), SidebarTree.TEAMS_MENU)
    }

    @Test
    fun `team items replay`() {
        val cases = fixture.list("teamItems")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            fun sortables(key: String) = case.list(key).map {
                SidebarTree.Sortable(it.str("id")!!, it.getValue("sortOrder").jsonPrimitive.double, it.str("role"))
            }
            val actual = SidebarTree.teamItems(
                teamId = case.str("teamId")!!,
                boards = sortables("boards"),
                agents = sortables("agents"),
                apps = case.list("apps").map { SidebarTree.Named(it.str("id")!!, it.str("name")!!) },
                sessions = case.list("sessions").map { SidebarTree.SessionLike(it.str("id")!!, it.str("teamId")!!, it.str("parentId")) },
                entries = entries(case.list("entries")),
            )
            assertEquals(case.str("name"), items(case.list("expected")), actual)
        }
    }

    @Test
    fun `phone rows replay`() {
        val cases = fixture.getValue("phone").jsonObject.list("rows")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val teams = case.list("teams").map {
                SidebarTree.PhoneTreeTeam(
                    id = it.str("id")!!,
                    items = items(it.list("items")),
                    entries = entries(it.list("entries")),
                    open = it.getValue("open").jsonPrimitive.boolean,
                    closedFolders = strings(it.getValue("closedFolders")).toSet(),
                )
            }
            val expected = case.list("expected").map {
                SidebarTree.PhoneTreeRow(
                    kind = it.str("kind")!!,
                    id = it.str("id")!!,
                    teamId = it.str("teamId")!!,
                    depth = it.getValue("depth").jsonPrimitive.int,
                    name = it.str("name"),
                    open = it.getValue("open").jsonPrimitive.boolean,
                    count = it.getValue("count").jsonPrimitive.int,
                )
            }
            assertEquals(case.str("name"), expected, SidebarTree.phoneTreeRows(teams))
        }
    }

    @Test
    fun `phone empty replay`() {
        val cases = fixture.getValue("phone").jsonObject.list("empty")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val teams = case.list("teams").map { it.str("id")!! to it.getValue("count").jsonPrimitive.int }
            assertEquals(case.str("name"), case.str("expected"), SidebarTree.phoneTeamsEmpty(teams))
        }
    }

    @Test
    fun `the team open key`() {
        val template = fixture.getValue("openKeys").jsonObject.str("team")!!
        assertEquals(template.replace("{teamId}", "t-1"), SidebarTree.teamOpenKey("t-1"))
    }

    @Test
    fun `copy equals the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("teamActions"), SidebarTree.Copy.TEAM_ACTIONS)
        assertEquals(copy.str("settings"), SidebarTree.Copy.SETTINGS)
        assertEquals(copy.str("newTeam"), SidebarTree.Copy.NEW_TEAM)
        assertEquals(copy.str("joinTeam"), SidebarTree.Copy.JOIN_TEAM)
    }
}
