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

        // EXP-1284: a tapped thread link in agent prose — the Thread screen.
        data class Thread(val id: String) : Target

        // EXP-1291: a tapped `/t/{team}/sessions/{id}` link in agent prose
        // (the PR body footer's shape). A synced ROOT opens its thread (on
        // the Guide when [guide], the link's `?view=guide`); a child, or a
        // row this device has not synced, opens the raw session view.
        data class SessionLink(val id: String, val guide: Boolean) : Target

        // A verified https `/t/{team}/agent` App Link (EXP-825): the Agent
        // page composer, opened with an empty seed on the active account.
        data object Agent : Target

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

        // A verified https `/t/{team}/threads/{id}` App Link (EXP-1288): the
        // thread twin of [WebIssueRef], resolved the same way (account by
        // host, brief poll for the synced row, Custom Tab on a miss).
        data class WebThreadRef(
            val uri: android.net.Uri,
            val host: String,
            val teamSlug: String,
            val threadId: String,
            // EXP-1291: the link was `/t/{team}/sessions/{id}` — a root opens
            // its thread (Guide on `?view=guide`), a child its session view.
            val sessionPath: Boolean = false,
        ) : Target

        // A verified https App Link the manifest claims but the app cannot
        // render (its pathPattern `/t/.*/agent` also matches nested paths
        // such as `/t/x/boards/b/issues/APP-1/agent`): AppNavHost hands it
        // to a Custom Tab, the same fallback an unresolvable issue link takes.
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

    fun openThread(id: String) {
        _target.value = Target.Thread(id)
    }

    fun openSessionLink(id: String, guide: Boolean) {
        _target.value = Target.SessionLink(id, guide)
    }

    fun openAgent() {
        _target.value = Target.Agent
    }

    fun openWebIssueRef(
        uri: android.net.Uri,
        host: String,
        teamSlug: String,
        identifier: String,
    ) {
        _target.value = Target.WebIssueRef(uri, host, teamSlug, identifier)
    }

    fun openWebThreadRef(
        uri: android.net.Uri,
        host: String,
        teamSlug: String,
        threadId: String,
        sessionPath: Boolean = false,
    ) {
        _target.value = Target.WebThreadRef(uri, host, teamSlug, threadId, sessionPath)
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
