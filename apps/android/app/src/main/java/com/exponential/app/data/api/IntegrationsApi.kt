package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/integrations.ts. Both procedures are
// query-shaped and take no meaningful input (`repos` accepts an optional
// `refresh` to bypass the server's per-user cache after an install lands).
// GitHub is server-only — these back the inline connect + repo picker in the
// onboarding / create-project flow.

/** GitHub App install state for the signed-in user (`integrations.github.status`). */
@Serializable
data class GithubStatusResult(
    val configured: Boolean = false,
    val installed: Boolean = false,
    val installUrl: String? = null,
    val accounts: List<String> = emptyList(),
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
    val repos: List<GithubPickerRepo> = emptyList(),
    val hasMore: Boolean = false,
)

@Serializable
private object IntegrationsEmptyInput

@Serializable
private data class ReposInput(val refresh: Boolean? = null)

@Singleton
class IntegrationsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun githubStatus(accountId: String): GithubStatusResult =
        trpc.query(
            accountId,
            path = "integrations.github.status",
            input = IntegrationsEmptyInput,
            inputSerializer = IntegrationsEmptyInput.serializer(),
            outputSerializer = GithubStatusResult.serializer(),
        )

    /** `refresh` bypasses the server cache so returning from an install reflects new repos. */
    suspend fun githubRepos(accountId: String, refresh: Boolean = false): GithubReposResult =
        trpc.query(
            accountId,
            path = "integrations.github.repos",
            input = ReposInput(refresh = if (refresh) true else null),
            inputSerializer = ReposInput.serializer(),
            outputSerializer = GithubReposResult.serializer(),
        )
}
