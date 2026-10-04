package com.exponential.app.data.api

import com.exponential.app.data.api.HttpClientModule.provideJson
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-1154: the `issues.prDescription` answer the Results face's PR-body
 * fallback reads — the open PR's GitHub title + body. Server shape
 * (`lib/trpc/issues.ts`): `{repo, prNumber, url, title, body, state}`, every
 * field null when no PR is linked.
 */
class PrDescriptionWireFormatTest {
    private val json = provideJson()

    @Test
    fun `decodes a linked pull request`() {
        val decoded = json.decodeFromString(
            PrDescription.serializer(),
            """{"repo":"acme/web","prNumber":42,"url":"https://github.com/acme/web/pull/42",""" +
                """"title":"Group board issues","body":"## Summary\nDid it","state":"open","extra":1}""",
        )
        assertEquals(
            PrDescription(
                repo = "acme/web",
                prNumber = 42,
                url = "https://github.com/acme/web/pull/42",
                title = "Group board issues",
                body = "## Summary\nDid it",
                state = "open",
            ),
            decoded,
        )
    }

    @Test
    fun `decodes the all-null answer of an issue with no pull request`() {
        val decoded = json.decodeFromString(
            PrDescription.serializer(),
            """{"repo":null,"prNumber":null,"url":null,"title":null,"body":null,"state":null}""",
        )
        assertEquals(PrDescription(), decoded)
        assertNull(json.decodeFromString(PrDescription.serializer(), "{}").body)
    }
}
