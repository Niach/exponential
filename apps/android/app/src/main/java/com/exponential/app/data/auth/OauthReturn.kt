package com.exponential.app.data.auth

/**
 * The `exponential://oauth-return` deep link, parsed without `android.net.Uri`
 * so it runs on the JVM (the caller passes the ENCODED fragment + query and a
 * URI-style decoder: `Uri.decode` on device, `+` stays literal).
 *
 * Three answers ride it: `error=<reason>` (any failed hop, REV2-53),
 * `linked=<providerId>` (EXP-1126: the handoff's link mode added a method to
 * the signed-in account — no token involved), and `code=<pkce code>` (a login
 * to redeem, REV-13). Each rides the fragment AND the query (EXP-21: browsers
 * may drop the fragment handing a custom scheme to the OS), fragment first.
 */
object OauthReturn {

    sealed interface Result {
        data class Error(val reason: String) : Result
        data class Linked(val providerId: String) : Result
        data class Code(val code: String) : Result
        data object None : Result
    }

    /** Precedence error > linked > code: a failure always wins. */
    fun parse(encodedFragment: String?, encodedQuery: String?, decode: (String) -> String): Result {
        val sources = listOfNotNull(encodedFragment, encodedQuery)
        fun param(key: String): String? {
            for (encoded in sources) {
                val value = encoded
                    .split("&")
                    .map { it.split("=", limit = 2) }
                    .firstOrNull { it.firstOrNull() == key }
                    ?.getOrNull(1)
                    ?.let(decode)
                if (!value.isNullOrEmpty()) return value
            }
            return null
        }
        param("error")?.let { return Result.Error(it) }
        param("linked")?.let { return Result.Linked(it) }
        param("code")?.let { return Result.Code(it) }
        return Result.None
    }

    /**
     * Copy for a failed LINK. Byte-identical to the web's
     * `oauthLinkErrorMessage` (apps/web/src/lib/deep-link.ts); an unknown
     * reason gets the generic line so a new server slug is never shown raw.
     */
    fun linkErrorMessage(reason: String): String = when (reason) {
        "access_denied" -> "Linking was cancelled."
        "link_ticket_invalid", "state_missing", "state_invalid", "state_mismatch", "state_not_found",
        "please_restart_the_process" -> "That link request expired. Please try again."
        "account_already_linked_to_different_user" -> "That account is already linked to a different user."
        else -> "Couldn't link that account. Please try again."
    }
}
