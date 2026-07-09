package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/integrations.ts. Both procedures are
// query-shaped: `status`/`repos` take an optional `workspaceId` (installations
// are now claimed per workspace) and `repos` also accepts a `refresh` to bypass
// the server's per-user cache after an install lands. GitHub is server-only —
// these back the inline connect + repo picker in the onboarding / create-project
// flow. `connectUrl` is a mobile-friendly single-consent OAuth authorize URL
// that claims the GitHub account for the workspace; the connect hop prefers it
// over `installUrl` (the App install page, which also grants more repos).

/**
 * One GitHub App installation the workspace has claimed (`installations[]` on
 * both `status` and `repos`). Repos are grant-scoped per user now: a link made
 * before the grant model (or whose grants were revoked) returns NO repos and
 * flags `needsReauth` until a member re-runs the OAuth connect flow —
 * reconnect via `connectUrl` (the install page never re-captures grants).
 * `hasMore` (repos only) marks a truncated repo list for this installation.
 */
@Serializable
data class GithubInstallation(
    val installationId: Long,
    val accountLogin: String? = null,
    val accountType: String? = null,
    val manageUrl: String,
    val needsReauth: Boolean = false,
    val hasMore: Boolean = false,
)

/** GitHub App install state for the signed-in user (`integrations.github.status`). */
@Serializable
data class GithubStatusResult(
    val configured: Boolean = false,
    val installed: Boolean = false,
    val installUrl: String? = null,
    val connectUrl: String? = null,
    val accounts: List<String> = emptyList(),
    val installations: List<GithubInstallation> = emptyList(),
)

/**
 * One repo the user's GitHub App can connect (a web `InstallationRepo` row).
 * `private` is a Kotlin keyword so it's mapped via @SerialName; the extra fields
 * seed the registry row when this repo is connected inline via `projects.create`.
 */
@Serializable
data class GithubPickerRepo(
    val fullName: String,
    @SerialName("private") val isPrivate: Boolean = false,
    val defaultBranch: String = "main",
    val installationId: Int = 0,
)

@Serializable
data class GithubReposResult(
    val configured: Boolean = false,
    val installed: Boolean = false,
    val installUrl: String? = null,
    val connectUrl: String? = null,
    val repos: List<GithubPickerRepo> = emptyList(),
    val hasMore: Boolean = false,
    val installations: List<GithubInstallation> = emptyList(),
)

// Scopes the query to a workspace (installations are claimed per workspace;
// the server requires it).
@Serializable
private data class StatusInput(
    val workspaceId: String,
)

@Serializable
private data class ReposInput(
    val workspaceId: String,
    val refresh: Boolean? = null,
    // Marks the caller as a mobile client so the server hands back a
    // mobile-marked installUrl/connectUrl: the post-install page then fires the
    // exp://github-connected deep link back into the app instead of
    // continuing in the browser. (Servers predating the marker just
    // ignore the extra field.)
    val platform: String? = null,
)

@Singleton
class IntegrationsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun githubStatus(accountId: String, workspaceId: String): GithubStatusResult =
        trpc.query(
            accountId,
            path = "integrations.github.status",
            input = StatusInput(workspaceId = workspaceId),
            inputSerializer = StatusInput.serializer(),
            outputSerializer = GithubStatusResult.serializer(),
        )

    /**
     * `refresh` bypasses the server cache so returning from an install reflects new
     * repos. `workspaceId` scopes the lookup to the workspace claiming the
     * installation. Always sends `platform: "mobile"` so the returned
     * installUrl/connectUrl finishes with the exp://github-connected deep link
     * instead of staying in the browser.
     */
    suspend fun githubRepos(
        accountId: String,
        workspaceId: String,
        refresh: Boolean = false,
    ): GithubReposResult =
        trpc.query(
            accountId,
            path = "integrations.github.repos",
            input = ReposInput(
                workspaceId = workspaceId,
                refresh = if (refresh) true else null,
                platform = "mobile",
            ),
            inputSerializer = ReposInput.serializer(),
            outputSerializer = GithubReposResult.serializer(),
        )
}
