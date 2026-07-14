package com.exponential.app.domain

/**
 * Builds the canonical web-app URL for sharing an issue. Mirrors the web route
 * shape and the iOS `WebLinks` helper:
 *   issue  {base}/w/{workspace}/projects/{project}/issues/{identifier}
 *
 * `base` is the account's instance URL; its trailing slash is trimmed so the
 * joined path never doubles up. All slugs/identifiers are synced locally, so
 * this composes without a network call. (Board-level sharing was removed —
 * sharing is issue-only across all clients.)
 */
object WebLinks {
    fun issueUrl(
        base: String,
        workspaceSlug: String,
        projectSlug: String,
        identifier: String,
    ): String = "${base.trimEnd('/')}/w/$workspaceSlug/projects/$projectSlug/issues/$identifier"

    /** A web-app URL the native app can render (EXP-92 App Links). */
    sealed interface Parsed {
        data class IssueRef(
            val workspaceSlug: String,
            val projectSlug: String,
            val identifier: String,
        ) : Parsed

        data class Invite(val token: String) : Parsed
    }

    /**
     * Inverse of [issueUrl] plus the invite route — the only two shapes the
     * manifest's autoVerify filter claims. Takes the already-decoded path
     * (Uri.getPath) rather than a Uri so it stays JVM-unit-testable; empty
     * segments are dropped (trailing slash tolerated) and deeper paths fail
     * the exact-length match — the app only claims what it can render.
     */
    fun parsePath(path: String?): Parsed? {
        val parts = path.orEmpty().split('/').filter { it.isNotEmpty() }
        return when {
            parts.size == 6 && parts[0] == "w" && parts[2] == "projects" && parts[4] == "issues" ->
                Parsed.IssueRef(parts[1], parts[3], parts[5])
            parts.size == 2 && parts[0] == "invite" -> Parsed.Invite(parts[1])
            else -> null
        }
    }
}
