package com.exponential.app.domain

import org.junit.Assert.assertNotEquals
import org.junit.Test

// EXP-390: every known server error slug must map to copy more specific than
// the generic fallback (mirror of the desktop's
// error_copy_covers_every_server_slug).
class GithubConnectErrorTest {
    @Test
    fun knownSlugsBeatTheFallback() {
        val fallback = githubConnectErrorMessage("definitely-unknown")
        for (slug in listOf("session", "exchange", "none", "notowner", "orgperm", "forbidden")) {
            assertNotEquals(slug, fallback, githubConnectErrorMessage(slug))
        }
    }
}
