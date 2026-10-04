package com.exponential.app.data.auth

import com.exponential.app.data.api.AuthWire
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.UsersApi
import javax.inject.Inject
import javax.inject.Singleton

/**
 * SLOP-26: the ONE "Connect GitHub" hop on Android, shared by the
 * Add-repository picker and team settings' connection block.
 *
 * Connect = the EXP-1126 link-ticket path: `users.mintSignInLinkTicket({
 * provider: "github"})`, then `/api/mobile-oauth-start?provider=github&
 * link=<ticket>` in a Custom Tab (the same handoff the Sign-in methods
 * section runs), returning as `exponential://oauth-return?linked=github`
 * → [AuthRepository.linkResult]. A server that does not offer the GitHub
 * link (the mint answers BAD_REQUEST) falls back to the guided web page
 * ([Hop.Page] = `connectUrl`, `/integrations/github?return=app`), which hands
 * back through `exponential://github-connected`.
 *
 * Installing the app is NOT a hop of this class: `installUrl` opens in a
 * Custom Tab on its own (the App's setup URL lands on the web page, whose
 * "Return to the app" fires the same `github-connected` deep link).
 */
@Singleton
class GithubConnectStarter @Inject constructor(
    private val usersApi: UsersApi,
    private val auth: AuthRepository,
) {
    sealed interface Hop {
        val url: String

        /** The link-ticket handoff; its outcome arrives on [AuthRepository.linkResult]. */
        data class Link(override val url: String) : Hop

        /** The guided web page; its done state fires `exponential://github-connected`. */
        data class Page(override val url: String) : Hop
    }

    /**
     * The URL the connect hop opens, or null when neither path is available
     * (no instance URL for [accountId], or the link is not offered and the
     * server sent no [connectUrl]). Throws the mint's other refusals.
     */
    suspend fun start(accountId: String, connectUrl: String?): Hop? {
        val baseUrl = auth.accounts.value.firstOrNull { it.id == accountId }?.instanceUrl ?: return null
        val ticket = try {
            usersApi.mintSignInLinkTicket(accountId, PROVIDER)
        } catch (e: TrpcException) {
            if (isLinkNotOffered(e.code)) {
                return connectUrl?.let { Hop.Page(it) }
            }
            throw e
        }
        val challenge = auth.beginLinkAttempt(accountId, PROVIDER)
        return Hop.Link(AuthWire.linkStartUrl(baseUrl, ticket.ticket, PROVIDER, challenge))
    }

    companion object {
        const val PROVIDER = "github"

        /**
         * Whether a refused `users.mintSignInLinkTicket({provider: "github"})`
         * means the server does not offer the GitHub link (BAD_REQUEST: "That
         * sign-in provider is not offered on this instance") — the hop then
         * falls back to the guided web page. Any other refusal is shown.
         */
        fun isLinkNotOffered(code: String?): Boolean = code == "BAD_REQUEST"
    }
}
