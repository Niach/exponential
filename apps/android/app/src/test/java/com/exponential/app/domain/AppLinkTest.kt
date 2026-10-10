package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
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
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val href = case.getValue("href").jsonPrimitive.content
            assertEquals(name, expected(case.getValue("link").jsonObject), classifyAppLink(href, origin))
        }
    }

    private fun expected(link: JsonObject): AppLink {
        fun s(key: String) = link.getValue(key).jsonPrimitive.content
        return when (val kind = s("kind")) {
            "issue" -> AppLink.Issue(s("teamSlug"), s("boardSlug"), s("identifier"))
            "session" -> AppLink.Session(s("teamSlug"), s("sessionId"))
            "app" -> AppLink.App(s("path"))
            "external" -> AppLink.External(s("url"))
            "ignore" -> AppLink.Ignore
            else -> error("unknown kind $kind")
        }
    }
}
