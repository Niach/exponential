package com.exponential.app.ui.agent

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.domain.AccountOptions
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-1158: the composer sends the picked login's id VERBATIM. `system` NAMES
 * the ambient login; only an absent account means "the last used login".
 */
class ComposerAccountWireTest {

    @Test
    fun `the composer sends a picked ambient login as system`() {
        // A machine signed into claude without profiles: its one option is
        // the ambient `system` login, and it is the last used one.
        val options = AccountOptions.flatten(
            mapOf("claude" to AgentAccount(signedIn = true, email = "me@x.test")),
            usage = null,
            launchDefaults = null,
        )
        val ambient = AccountOptions.lastUsed(options)!!
        assertEquals(SYSTEM_PROFILE_ID, ambient.id)

        val draft = LaunchDraft(agent = "claude").withAccount(ambient)
        assertEquals("system", draft.wireAccount)
        assertEquals("claude:system", draft.accountKey)
    }

    @Test
    fun `a named profile rides as itself and no pick sends no account`() {
        val draft = LaunchDraft(agent = "claude")
        assertNull(draft.wireAccount)
        assertNull(draft.withAccount(null).wireAccount)
        assertEquals("work", draft.copy(account = "work").wireAccount)
    }
}
