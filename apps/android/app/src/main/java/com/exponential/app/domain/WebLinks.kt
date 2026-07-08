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
}
