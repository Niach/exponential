package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * Pure wire helpers for the EXP-857 login paths (one-time code + passkey), kept
 * out of [AuthApi] so they can be unit-tested without a live HttpClient.
 */
object AuthWire {

    /**
     * The `name=value` pair of every `Set-Cookie` header on a response, in the
     * order the server sent them.
     *
     * The passkey ceremony needs this: the app's HTTP client has cookies
     * disabled on purpose, so the signed challenge cookie the
     * generate-authenticate-options call sets (a `better-auth-passkey` name,
     * usually `__Secure-` prefixed) has to be replayed by hand on the verify
     * call. Everything after the first `;` is attributes (Path, HttpOnly,
     * SameSite, Expires) and is dropped — a request `Cookie` header carries
     * pairs only. Names are taken verbatim, prefixes included: `__Secure-x` and
     * `x` are DIFFERENT cookies to the server.
     */
    fun setCookiePairs(headers: List<String>): List<String> = headers.mapNotNull { raw ->
        val pair = raw.substringBefore(';').trim()
        if (!pair.contains('=') || pair.substringBefore('=').isBlank()) null else pair
    }

    /** The request `Cookie` header value for [pairs] (`name=value; name2=value2`). */
    fun cookieHeader(pairs: List<String>): String = pairs.joinToString("; ")

    /**
     * `{"response": <AuthenticationResponseJSON>}` — the verify-authentication
     * body. [responseJson] is the string Android's CredentialManager hands back,
     * and it must be EMBEDDED AS AN OBJECT, not as a JSON string, so it is
     * parsed here rather than interpolated. Throws when it isn't an object,
     * which the caller reports as a failed ceremony.
     */
    fun passkeyVerifyBody(json: Json, responseJson: String): JsonObject = buildJsonObject {
        put("response", json.parseToJsonElement(responseJson) as JsonObject)
    }

    /**
     * Login-screen copy for a failed `/sign-in/email-otp` (contract EXP-857).
     * The three known codes get wording that says what to do next; anything else
     * falls through to the server's own `message`.
     */
    fun otpErrorMessage(code: String?, message: String?): String = when (code) {
        "INVALID_OTP" -> "That code is not right. Check the email and try again."
        "OTP_EXPIRED" -> "That code expired. Request a new one."
        "TOO_MANY_ATTEMPTS" -> "Too many attempts. Request a new code."
        else -> message?.takeIf { it.isNotBlank() } ?: "Couldn't verify that code. Please try again."
    }

    /** Login-screen copy for a failed send-verification-otp. */
    fun sendCodeErrorMessage(status: Int, message: String?): String = when {
        !message.isNullOrBlank() -> message
        status == 429 -> "Too many requests. Wait a moment and try again."
        else -> "Couldn't send the code (HTTP $status)"
    }
}

/** The options + challenge cookies one passkey ceremony runs with. */
data class PasskeyOptions(
    /** The PublicKeyCredentialRequestOptionsJSON exactly as served. */
    val requestJson: String,
    /** Every `name=value` pair from the options response's Set-Cookie headers. */
    val cookies: List<String>,
)
