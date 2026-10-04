package com.exponential.app.data.auth

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// SLOP-26: only the mint's "not offered" refusal (BAD_REQUEST) sends the
// connect hop to the guided web page; every other refusal is shown as is.
class GithubConnectStarterTest {
    @Test
    fun onlyBadRequestFallsBackToTheGuidedPage() {
        assertTrue(GithubConnectStarter.isLinkNotOffered("BAD_REQUEST"))
        assertFalse(GithubConnectStarter.isLinkNotOffered("UNAUTHORIZED"))
        assertFalse(GithubConnectStarter.isLinkNotOffered(null))
    }
}
