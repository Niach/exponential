package com.exponential.app.data.api

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * EXP-1026: a one-time code for an address with no account creates one, and the
 * account needs a name. The natives ASK for it (`X-Exp-Ask-Name: 1` on every
 * `/sign-in/email-otp`): the server then answers an unknown address sent
 * without a `name` with 400 `code: "NAME_REQUIRED"` WITHOUT consuming the code,
 * the login screen shows the name step, and the SAME code is resubmitted as
 * `{email, otp, name}`. Existing accounts never see the step. Mirrors iOS
 * `EmailCodeSignUp` (ExpCore `EmailOtp.swift`).
 */
object EmailCodeSignUp {
    const val ASK_NAME_HEADER = "X-Exp-Ask-Name"
    const val ASK_NAME_VALUE = "1"
    const val NAME_REQUIRED_CODE = "NAME_REQUIRED"

    /** The `/sign-in/email-otp` body; `name` rides only once the user gave one. */
    fun requestBody(email: String, otp: String, name: String?): JsonObject = buildJsonObject {
        put("email", email)
        put("otp", otp)
        name?.trim()?.takeIf { it.isNotEmpty() }?.let { put("name", it) }
    }

    /** Whether a failed verify is the server asking for the name step. */
    fun isNameRequired(status: Int, code: String?): Boolean =
        status == 400 && code?.uppercase() == NAME_REQUIRED_CODE
}
