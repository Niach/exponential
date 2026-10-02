package com.exponential.app.domain

import com.exponential.app.ui.components.DeviceSetupCopy

/**
 * RFC 8628 device codes (EXP-403, EXP-1169): the rules the device-setup
 * block's "enter the code the CLI shows" field follows, mirroring web
 * `lib/auth/device-code.ts`.
 */
object DeviceCodeRules {
    /**
     * Codes are always 8 chars (printed XXXX-XXXX); the server strips dashes
     * and the generated charset is uppercase-only. The dash goes in only once
     * a 5th char exists, so backspacing over it deletes instead of fighting
     * the formatter, and a pasted dashed code stays stable.
     */
    fun normalizeUserCode(input: String): String {
        val bare = input.uppercase().filter { it in 'A'..'Z' || it in '0'..'9' }.take(8)
        return if (bare.length >= 5) "${bare.take(4)}-${bare.drop(4)}" else bare
    }

    /** A complete code: 8 characters once the dash is stripped. */
    fun isCompleteUserCode(code: String): Boolean = code.replace("-", "").length == 8

    /** Better Auth's RFC 8628 `error` field to the card's copy. */
    fun errorMessage(error: String?): String = when (error) {
        "expired_token" -> DeviceSetupCopy.CODE_EXPIRED
        "invalid_request", "invalid_grant" -> DeviceSetupCopy.CODE_INVALID
        "access_denied" -> DeviceSetupCopy.CODE_OTHER_ACCOUNT
        else -> DeviceSetupCopy.FAILED
    }
}
