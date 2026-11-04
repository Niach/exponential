package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1188: the in-app link rule, locked ×4 (web `app-link.test.ts`, iOS
 * `AppLinkTests`, desktop `domain::app_link`) against the ONE contract fixture
 * `app-link.json` — same cases, same names.
 */
class AppLinkTest {

    private val fixture = Json.parseToJsonElement(contractFixtureJson("app-link.json")).jsonObject

    @Test
    fun everyCaseClassifiesTheSame() {
        val origin = fixture.getValue("origin").jsonPrimitive.content
        val defaultTeam = fixture.getValue("team").jsonPrimitive.content
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val href = case.getValue("href").jsonPrimitive.content
            // #EXP-1674: absent = the fixture's team; null = no team.
            val team = case["team"].let { if (it == null) defaultTeam else it.jsonPrimitive.contentOrNull }
            assertEquals(name, expected(case.getValue("link").jsonObject), classifyAppLink(href, origin, team))
        }
    }

    @Test
    fun everyOpenCaseOutcomeIsTheSame() {
        val cases = fixture.getValue("openCases").jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val input = case.getValue("input").jsonObject
            fun flag(key: String) = input.getValue(key).jsonPrimitive.boolean
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("outcome").jsonPrimitive.content,
                linkOpenOutcome(flag("member"), flag("activeTeam"), flag("found"), flag("webOnly")).wire,
            )
        }
    }

    @Test
    fun theNoAccessCopyIsTheContracts() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.getValue("noAccess").jsonPrimitive.content, LINK_NO_ACCESS)
    }

    // `encodeURIComponent`, and it round-trips through the segment decoder.
    @Test
    fun theTeamSlugEncodesLikeEncodeUriComponent() {
        assertEquals("/t/a%20b%2Fc%C3%A9-_.!~*'()/inbox", expandTeamLink("~/inbox", "a b/cé-_.!~*'()"))
        assertEquals(
            AppLink.Inbox("a b/cé-_.!~*'()"),
            classifyAppLink("~/inbox", "https://app.exponential.at", "a b/cé-_.!~*'()"),
        )
    }

    @Test
    fun aLinksTeamIsReadOffItsRouteOrPath() {
        assertEquals("acme", appLinkTeam(AppLink.Inbox("acme")))
        assertEquals("my team", appLinkTeam(AppLink.App("/t/my%20team/reviews?tab=mine")))
        assertEquals(null, appLinkTeam(AppLink.App("/settings/account")))
        assertEquals(null, appLinkTeam(AppLink.External("https://example.com")))
        assertTrue(isAppsListPath("/t/acme/apps?x=1"))
        assertTrue(!isAppsListPath("/t/acme/apps/crm/deals/more"))
    }

    private fun expected(link: JsonObject): AppLink {
        fun s(key: String) = link.getValue(key).jsonPrimitive.content
        return when (val kind = s("kind")) {
            "issue" -> AppLink.Issue(s("teamSlug"), s("boardSlug"), s("identifier"))
            "issueRef" -> AppLink.IssueRef(s("teamSlug"), s("identifier"))
            "board" -> AppLink.Board(s("teamSlug"), s("boardSlug"))
            "session" -> AppLink.Session(s("teamSlug"), s("sessionId"))
            "plan" -> AppLink.Plan(s("teamSlug"), s("sessionId"), s("messageId"))
            "thread" -> AppLink.Thread(s("teamSlug"), s("threadId"))
            "threads" -> AppLink.Threads(s("teamSlug"))
            "agent" -> AppLink.Agent(s("teamSlug"), s("agentId"))
            "agentHome" -> AppLink.AgentHome(s("teamSlug"))
            "inbox" -> AppLink.Inbox(s("teamSlug"))
            "devices" -> AppLink.Devices(s("teamSlug"))
            "settings" -> AppLink.Settings(s("teamSlug"), s("section"))
            "appHome" -> AppLink.AppHome(s("teamSlug"), s("appSlug"))
            "appScreen" -> AppLink.AppScreen(
                s("teamSlug"),
                s("appSlug"),
                s("screen"),
                link.getValue("params").jsonObject.mapValues { it.value.jsonPrimitive.content },
            )
            "app" -> AppLink.App(s("path"))
            "external" -> AppLink.External(s("url"))
            "ignore" -> AppLink.Ignore
            else -> error("unknown kind $kind")
        }
    }
}
