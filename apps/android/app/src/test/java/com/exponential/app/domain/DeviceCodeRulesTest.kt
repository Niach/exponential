package com.exponential.app.domain

import com.exponential.app.AppConstants
import com.exponential.app.ui.components.DeviceSetupCopy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1169: the server card's code and command rules (web `lib/auth/device-code.ts`). */
class DeviceCodeRulesTest {
    @Test
    fun normalizesTheWebVectors() {
        assertEquals("ZP3H-V7HK", DeviceCodeRules.normalizeUserCode("zp3hv7hk"))
        assertEquals("ZP3H", DeviceCodeRules.normalizeUserCode("zp3h"))
        assertEquals("ZP3H", DeviceCodeRules.normalizeUserCode("ZP3H-"))
        assertEquals("ZP3H-V", DeviceCodeRules.normalizeUserCode("zp3hv"))
        assertEquals("ZP3H-V7HK", DeviceCodeRules.normalizeUserCode("ZP3H-V7HK99"))
    }

    @Test
    fun completeMeansEightCharactersWithoutTheDash() {
        assertTrue(DeviceCodeRules.isCompleteUserCode("ZP3H-V7HK"))
        assertTrue(DeviceCodeRules.isCompleteUserCode("ZP3HV7HK"))
        assertFalse(DeviceCodeRules.isCompleteUserCode("ZP3H-V7H"))
        assertFalse(DeviceCodeRules.isCompleteUserCode(""))
    }

    @Test
    fun mapsBetterAuthErrors() {
        assertEquals(DeviceSetupCopy.CODE_EXPIRED, DeviceCodeRules.errorMessage("expired_token"))
        assertEquals(DeviceSetupCopy.CODE_INVALID, DeviceCodeRules.errorMessage("invalid_request"))
        assertEquals(DeviceSetupCopy.CODE_INVALID, DeviceCodeRules.errorMessage("invalid_grant"))
        assertEquals(DeviceSetupCopy.CODE_OTHER_ACCOUNT, DeviceCodeRules.errorMessage("access_denied"))
        assertEquals(DeviceSetupCopy.FAILED, DeviceCodeRules.errorMessage("slow_down"))
        assertEquals(DeviceSetupCopy.FAILED, DeviceCodeRules.errorMessage(null))
    }

    @Test
    fun copiedCommandIsOneLine() {
        assertEquals(
            "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://x.test EXP_INSTALL_TOKEN=abc sh",
            AppConstants.serverInstallSnippet("https://x.test", "abc"),
        )
        assertEquals(
            "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://x.test sh",
            AppConstants.serverInstallSnippet("https://x.test"),
        )
    }

    @Test
    fun displayedCommandSitsOnFixedLines() {
        assertEquals(
            "curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=https://x.test \\\n  EXP_INSTALL_TOKEN=abc sh",
            AppConstants.serverInstallSnippetDisplayed("https://x.test", "abc"),
        )
        assertEquals(
            "curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=https://x.test sh",
            AppConstants.serverInstallSnippetDisplayed("https://x.test", null),
        )
    }
}
