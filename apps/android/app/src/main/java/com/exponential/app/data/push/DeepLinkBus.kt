package com.exponential.app.data.push

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * One-shot bridge from MainActivity (which receives Android intents) to
 * the Compose nav graph (which decides how to react). Stores the most
 * recent target; the consumer must call consume() after acting on it.
 */
@Singleton
class DeepLinkBus @Inject constructor() {
    sealed interface Target {
        data class Issue(val id: String) : Target
        data class Invite(val token: String) : Target

        // exponential://support/{threadId} — a support_reply push tap (EXP-180).
        // Opens the ticket conversation directly (membership-gated server-side,
        // so it works whichever team of the active account it belongs to).
        data class SupportThread(val id: String) : Target

        // A verified https App Link (EXP-92): carries the web URL's slugs +
        // identifier; AppNavHost resolves them against the local DB of the
        // account matching `host` (falling back to a Custom Tab). `uri` is
        // kept for that fallback.
        data class WebIssueRef(
            val uri: android.net.Uri,
            val host: String,
            val teamSlug: String,
            val identifier: String,
        ) : Target

        // Content shared into the app from another app (ACTION_SEND). Image URIs
        // are stable file:// cache URIs (see ShareIntentParser). Same-process, so
        // holding Uri in a singleton-held data class is fine.
        data class ShareContent(
            val text: String?,
            val subject: String?,
            val imageUris: List<android.net.Uri>,
        ) : Target
    }

    private val _target = MutableStateFlow<Target?>(null)
    val target: StateFlow<Target?> = _target.asStateFlow()

    // exponential://github-connected[?error=<code>] — the GitHub App
    // install/reconnect finished in the Custom Tab and the server's page
    // deep-linked back into the app. Delivered on a monotonic `seq`, NOT via
    // `target` (EXP-365): the repo picker AND team settings can both be alive,
    // and the consume()-based one-shot let whichever collector won the race
    // null the value before the other saw it — the loser stayed stale. Every
    // collector observes every increment; collectors `drop(1)` to skip the
    // StateFlow replay. `error` is the server's failure slug (EXP-390) — null
    // means the connect succeeded.
    data class GithubConnected(val seq: Int = 0, val error: String? = null)

    private val _githubConnected = MutableStateFlow(GithubConnected())
    val githubConnected: StateFlow<GithubConnected> = _githubConnected.asStateFlow()

    fun openIssue(id: String) {
        _target.value = Target.Issue(id)
    }

    fun openInvite(token: String) {
        _target.value = Target.Invite(token)
    }

    fun openSupportThread(id: String) {
        _target.value = Target.SupportThread(id)
    }

    fun openWebIssueRef(
        uri: android.net.Uri,
        host: String,
        teamSlug: String,
        identifier: String,
    ) {
        _target.value = Target.WebIssueRef(uri, host, teamSlug, identifier)
    }

    fun openGithubConnected(error: String? = null) {
        _githubConnected.value = GithubConnected(_githubConnected.value.seq + 1, error)
    }

    fun openShare(text: String?, subject: String?, imageUris: List<android.net.Uri>) {
        _target.value = Target.ShareContent(text, subject, imageUris)
    }

    fun consume() {
        _target.value = null
    }
}
