package com.exponential.app.domain

/**
 * Builds canonical web-app URLs for sharing. Mirrors the web route shapes and
 * the iOS `WebLinks` helper:
 *   board  {base}/w/{workspace}/projects/{project}
 *   issue  {base}/w/{workspace}/projects/{project}/issues/{identifier}
 *
 * `base` is the account's instance URL; its trailing slash is trimmed so the
 * joined path never doubles up. All slugs/identifiers are synced locally, so
 * these compose without a network call.
 */
object WebLinks {
    private fun trimBase(base: String): String = base.trimEnd('/')

    fun boardUrl(base: String, workspaceSlug: String, projectSlug: String): String =
        "${trimBase(base)}/w/$workspaceSlug/projects/$projectSlug"

    fun issueUrl(
        base: String,
        workspaceSlug: String,
        projectSlug: String,
        identifier: String,
    ): String = "${boardUrl(base, workspaceSlug, projectSlug)}/issues/$identifier"
}
