package com.exponential.app.ui.agent

import com.exponential.app.data.api.SteerDevice
import com.exponential.app.ui.components.accountOptionsFor
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-1158: the composer sends the picked login's id VERBATIM; an absent
 * account means "the last used login". The ambient login is never named.
 */
class ComposerAccountWireTest {

    @Test
    fun `a machine with no logins offers unpinned options that send no account`() {
        // A machine that reports no profiles: one unpinned option per runnable
        // agent, the machine's last used login decides.
        val options = accountOptionsFor(SteerDevice(deviceId = "dev"), listOf("claude"))
        val unpinned = options.single()
        assertEquals("", unpinned.id)

        val draft = LaunchDraft(agent = "claude").withAccount(unpinned)
        assertNull(draft.wireAccount)
        assertEquals("claude:", draft.accountKey)
        assertEquals(unpinned.key, draft.accountKey)
    }

    @Test
    fun `a legacy system account sends none`() {
        assertNull(LaunchDraft(agent = "claude", account = "system").wireAccount)
    }

    @Test
    fun `a named profile rides as itself and no pick sends no account`() {
        val draft = LaunchDraft(agent = "claude")
        assertNull(draft.wireAccount)
        assertNull(draft.withAccount(null).wireAccount)
        assertEquals("work", draft.copy(account = "work").wireAccount)
    }
}
