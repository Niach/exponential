package com.exponential.app.data.push

import com.exponential.app.data.auth.ServerAccount
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Push taps must land on the notification's own account (REV2-81): Android used
 * to drop the deep link whenever the push targeted a non-active signed-in
 * account, so the notification's content was unreachable from the tap.
 */
class PushDeepLinksTest {

    private fun account(
        id: String,
        userId: String?,
        token: String? = "tok",
        url: String = "https://app.exponential.at",
        lastUsedAt: Long = 0L,
    ) = ServerAccount(
        id = id,
        instanceUrl = url,
        token = token,
        userId = userId,
        lastUsedAt = lastUsedAt,
    )

    // MARK: - target

    @Test
    fun `an issue push targets the issue`() {
        assertEquals(
            PushDeepLinks.Target.Issue("i1"),
            PushDeepLinks.target(type = "issue_assigned", issueId = "i1", threadId = null),
        )
    }

    @Test
    fun `a support reply push targets the thread`() {
        assertEquals(
            PushDeepLinks.Target.SupportThread("t1"),
            PushDeepLinks.target(
                type = PushDeepLinks.TYPE_SUPPORT_REPLY,
                issueId = null,
                threadId = "t1",
            ),
        )
    }

    @Test
    fun `a thread id without the support type is ignored`() {
        assertNull(PushDeepLinks.target(type = "issue_comment", issueId = null, threadId = "t1"))
    }

    @Test
    fun `a payload with nothing to open has no target`() {
        assertNull(PushDeepLinks.target(type = null, issueId = null, threadId = null))
        assertNull(PushDeepLinks.target(type = null, issueId = "", threadId = ""))
    }

    // MARK: - uri

    @Test
    fun `carries the recipient so MainActivity can switch accounts`() {
        assertEquals(
            "exponential://issue/i1?userId=user-1",
            PushDeepLinks.uri(PushDeepLinks.Target.Issue("i1"), "user-1"),
        )
        assertEquals(
            "exponential://support/t1?userId=user-1",
            PushDeepLinks.uri(PushDeepLinks.Target.SupportThread("t1"), "user-1"),
        )
    }

    @Test
    fun `omits the recipient when the server sent none`() {
        assertEquals(
            "exponential://issue/i1",
            PushDeepLinks.uri(PushDeepLinks.Target.Issue("i1"), null),
        )
    }

    @Test
    fun `percent-encodes the recipient uri-style`() {
        // Uri.getQueryParameter decodes %XX but leaves `+` literal, so form
        // encoding on this side would corrupt the id.
        assertEquals(
            "exponential://issue/i1?userId=a%20b%2Bc",
            PushDeepLinks.uri(PushDeepLinks.Target.Issue("i1"), "a b+c"),
        )
    }

    // MARK: - resolveAccount

    private val active = account("acc-active", userId = "user-active")
    private val other = account("acc-other", userId = "user-other")

    @Test
    fun `no recipient hint stays on the active account`() {
        assertEquals(
            PushDeepLinks.Account.Active,
            PushDeepLinks.resolveAccount(listOf(active, other), "acc-active", null),
        )
    }

    @Test
    fun `a push for the active account needs no switch`() {
        assertEquals(
            PushDeepLinks.Account.Active,
            PushDeepLinks.resolveAccount(listOf(active, other), "acc-active", "user-active"),
        )
    }

    @Test
    fun `a push for another signed-in account switches to it`() {
        assertEquals(
            PushDeepLinks.Account.Switch("acc-other"),
            PushDeepLinks.resolveAccount(listOf(active, other), "acc-active", "user-other"),
        )
    }

    @Test
    fun `a push for a signed-out account is dropped`() {
        val signedOut = account("acc-other", userId = "user-other", token = null)
        assertEquals(
            PushDeepLinks.Account.Unknown,
            PushDeepLinks.resolveAccount(listOf(active, signedOut), "acc-active", "user-other"),
        )
        assertEquals(
            PushDeepLinks.Account.Unknown,
            PushDeepLinks.resolveAccount(listOf(active), "acc-active", "user-gone"),
        )
    }

    @Test
    fun `the same user on two servers resolves to the last used one`() {
        val older = account("acc-a", userId = "user-x", url = "https://a.example", lastUsedAt = 1L)
        val newer = account("acc-b", userId = "user-x", url = "https://b.example", lastUsedAt = 2L)
        assertEquals(
            PushDeepLinks.Account.Switch("acc-b"),
            PushDeepLinks.resolveAccount(listOf(older, newer), "acc-active", "user-x"),
        )
    }
}
