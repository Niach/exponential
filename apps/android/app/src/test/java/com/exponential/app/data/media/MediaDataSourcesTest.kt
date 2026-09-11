package com.exponential.app.data.media

import com.exponential.app.data.auth.ServerAccount
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-824: the inline player's request resolution mirrors the Coil
 * interceptor's same-origin rule — a relative attachment URL resolves against
 * the active instance and gets that account's bearer + client version; a
 * foreign host must never see the token.
 */
class MediaDataSourcesTest {

    private val cloud = ServerAccount(id = "a", instanceUrl = "https://app.example.com", token = "tok-a")
    private val selfHost = ServerAccount(id = "b", instanceUrl = "https://exp.corp.io/", token = "tok-b")

    @Test
    fun relativeUrlResolvesAgainstTheActiveInstanceWithItsToken() {
        val request = resolveMediaRequest(
            "/api/attachments/att-1",
            listOf(cloud, selfHost),
            activeAccountId = "b",
            activeInstanceUrl = "https://exp.corp.io/",
        )
        assertEquals("https://exp.corp.io/api/attachments/att-1", request.url)
        assertEquals("Bearer tok-b", request.headers["Authorization"])
        assertTrue(request.headers["x-client-version"].orEmpty().startsWith("android/"))
    }

    @Test
    fun absoluteSameInstanceUrlGetsTheOwningAccountsToken() {
        val request = resolveMediaRequest(
            "https://app.example.com/api/attachments/att-1",
            listOf(cloud, selfHost),
            activeAccountId = "b",
            activeInstanceUrl = "https://exp.corp.io",
        )
        assertEquals("Bearer tok-a", request.headers["Authorization"])
    }

    @Test
    fun foreignHostGetsNoHeadersAtAll() {
        val request = resolveMediaRequest(
            "https://cdn.evil.example/clip.mp4",
            listOf(cloud, selfHost),
            activeAccountId = "a",
            activeInstanceUrl = "https://app.example.com",
        )
        assertEquals("https://cdn.evil.example/clip.mp4", request.url)
        assertTrue(request.headers.isEmpty())
    }

    @Test
    fun prefixLookalikeHostIsForeign() {
        // `https://app.example.com.evil` must not match `https://app.example.com`.
        val request = resolveMediaRequest(
            "https://app.example.com.evil/api/attachments/x",
            listOf(cloud),
            activeAccountId = "a",
            activeInstanceUrl = "https://app.example.com",
        )
        assertNull(request.headers["Authorization"])
    }

    @Test
    fun signedOutAccountSendsNoToken() {
        val request = resolveMediaRequest(
            "/api/attachments/att-1",
            listOf(cloud.copy(token = null)),
            activeAccountId = "a",
            activeInstanceUrl = "https://app.example.com",
        )
        assertEquals("https://app.example.com/api/attachments/att-1", request.url)
        assertTrue(request.headers.isEmpty())
    }

    @Test
    fun relativeUrlWithoutAnInstanceStaysRelative() {
        val request = resolveMediaRequest("/api/attachments/att-1", emptyList(), null, null)
        assertEquals("/api/attachments/att-1", request.url)
        assertTrue(request.headers.isEmpty())
    }
}
