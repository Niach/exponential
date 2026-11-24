package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #NEW-133: the phrase grammar, locked ×4 (web `issue-search-phrase.test.ts`,
 * iOS `IssueSearchPhraseTests`, desktop `domain::issue_search_phrase`) against
 * the ONE contract fixture: same cases, same test names.
 */
class IssueSearchPhraseTest {

    private fun str(obj: JsonObject, key: String): String? =
        obj[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `issue search phrase (contract fixture)`() {
        val cases = Json.parseToJsonElement(phraseFixtureJson()).jsonObject.getValue("cases").jsonArray
        assertTrue(cases.size >= 32)
        for (element in cases) {
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val now = IssueSearch.epochMs(case.getValue("now").jsonPrimitive.content)
            assertTrue(name, now != Long.MIN_VALUE)
            val members = case.getValue("members").jsonArray.map {
                IssueSearchPhrase.Member(name = str(it.jsonObject, "name"), email = str(it.jsonObject, "email"))
            }
            assertEquals(
                name,
                str(case, "expected"),
                IssueSearchPhrase.translate(case.getValue("phrase").jsonPrimitive.content, members, now),
            )
        }
    }
}

private fun phraseFixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/issue-search-phrase.json",
        "../../packages/domain-contract/fixtures/issue-search-phrase.json",
        "packages/domain-contract/fixtures/issue-search-phrase.json",
    )
    val file = candidates.map(::File).firstOrNull { it.isFile }
        ?: error("issue-search-phrase.json not found from ${File(".").absolutePath}")
    return file.readText()
}
