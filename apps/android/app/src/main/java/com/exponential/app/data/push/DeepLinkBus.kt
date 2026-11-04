package com.exponential.app.data.push

import com.exponential.app.domain.AppLink
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
        // EXP-933: [face] = the Work screen face to open on (`results` for an
        // agent message's report); null = the default Issue face.
        // EXP-1241: [comment] = scroll to that comment and highlight it.
        data class Issue(val id: String, val face: String? = null, val comment: String? = null) : Target
        data class Invite(val token: String) : Target

        // exponential://inbox — an agent_message push tap (EXP-801). The row
        // renders in the My Work inbox and nowhere else.
        data object Inbox : Target

        // exponential://session/{id} — a session_blocked push tap (EXP-980):
        // straight to the run that hit the wall.
        data class Session(val id: String) : Target

        // exponential://device/{id} — an agent_signed_out push tap or inbox
        // row (#EXP-1534): the Devices page with that device's (devices row
        // id) settings open.
        data class Device(val deviceId: String) : Target

        // #EXP-1674: ONE LINK SYSTEM (app-link.json). Any in-app link, a tapped
        // one or a verified https App Link: AppNavHost resolves it against the
        // local DB of the account matching `host` and applies `linkOpenOutcome`.
        // `uri` = the absolute URL: its query (`?comment=`, `?view=guide`) and
        // the Custom Tab of a page with no native screen.
        data class Link(
            val link: AppLink,
            val uri: android.net.Uri,
            val host: String,
        ) : Target

        // A web source, or a claimed https link off the instance: a Custom Tab,
        // which never re-triggers App Links.
        data class WebUrl(val uri: android.net.Uri) : Target

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

    fun openIssue(id: String, face: String? = null, comment: String? = null) {
        _target.value = Target.Issue(id, face, comment)
    }

    fun openInvite(token: String) {
        _target.value = Target.Invite(token)
    }

    fun openInbox() {
        _target.value = Target.Inbox
    }

    fun openSession(id: String) {
        _target.value = Target.Session(id)
    }

    fun openDevice(deviceId: String) {
        _target.value = Target.Device(deviceId)
    }

    fun openLink(link: AppLink, uri: android.net.Uri, host: String) {
        _target.value = Target.Link(link, uri, host)
    }

    fun openWebUrl(uri: android.net.Uri) {
        _target.value = Target.WebUrl(uri)
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
