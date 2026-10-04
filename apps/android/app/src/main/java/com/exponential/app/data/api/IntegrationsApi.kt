package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// SLOP-7/SLOP-26: ONE GitHub flow. Mirrors apps/web/src/lib/trpc/integrations.ts.
// A member's GitHub connection is their own linked GitHub account; the
// installations and push-able repositories are listed LIVE off GitHub with
// that token. Nothing is claimed per team any more, so `installations[]`
// carries no re-auth / stale marks — a dead token is reported ONCE on the
// whole connection (`needsReconnect`). GitHub is server-only — these back the
// connection block in team settings and the Add-repository picker.

/**
 * One GitHub App installation the viewer's token sees (`installations[]` on
 * both `status` and `repos`). `suspended` = a GitHub-side App suspension
 * (REV2-29): the installation lists no repos and mints no tokens until it is
 * UNSUSPENDED on GitHub — a reconnect cannot fix it, so the UI never nudges
 * one. `hasMore` (repos only) marks a truncated repo list for this installation.
 * The pre-SLOP-7 `needsReauth`/`stale` keys the server still emits are ignored.
 */
@Serializable
data class GithubInstallation(
    val installationId: Long,
    val accountLogin: String? = null,
    val accountType: String? = null,
    val manageUrl: String,
    val suspended: Boolean = false,
    val hasMore: Boolean = false,
)

/**
 * The viewer's GitHub connection for a team context (`integrations.github.status`).
 * `linked` falls back to [installed] and `needsReconnect` to false on a server
 * predating the fields (the decoder's defaults are applied by [withFallbacks]).
 */
@Serializable
data class GithubStatusResult(
    val configured: Boolean = false,
    /** The viewer has a GitHub account linked (a token exists, live or dead). */
    val linked: Boolean? = null,
    /** Linked, but GitHub refuses the token — Reconnect is the one fix. */
    val needsReconnect: Boolean = false,
    /** The linked account's GitHub login; null until the token answered. */
    val login: String? = null,
    /** The App is installed on at least one account the token can see. */
    val installed: Boolean = false,
    /** GitHub's install page for the App (null without `GITHUB_APP_SLUG`). */
    val installUrl: String? = null,
    /**
     * The guided web page (`/integrations/github?return=app`), which hands
     * back through exponential://github-connected. The fallback connect hop
     * when the server does not offer the GitHub link ticket.
     */
    val connectUrl: String? = null,
    val installations: List<GithubInstallation> = emptyList(),
) {
    val isLinked: Boolean get() = linked ?: installed

    /** The missing prerequisite, null once repositories can be listed. */
    val prerequisite: GithubPrerequisite?
        get() = githubPrerequisite(configured, isLinked, needsReconnect, installed)
}

/**
 * One repo the viewer can push to (a web `InstallationRepo` row). `private` is
 * a Kotlin keyword so it's mapped via @SerialName; the extra fields seed the
 * registry row when this repo is connected inline via `boards.create`.
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
    val linked: Boolean? = null,
    val needsReconnect: Boolean = false,
    val login: String? = null,
    val installed: Boolean = false,
    val installUrl: String? = null,
    val connectUrl: String? = null,
    val repos: List<GithubPickerRepo> = emptyList(),
    val hasMore: Boolean = false,
    val installations: List<GithubInstallation> = emptyList(),
) {
    val isLinked: Boolean get() = linked ?: installed

    /** The missing prerequisite, null once the live list renders. */
    val prerequisite: GithubPrerequisite?
        get() = githubPrerequisite(configured, isLinked, needsReconnect, installed)
}

/** What stands between the viewer and the live repository list, in the order the guided web page walks them. */
enum class GithubPrerequisite {
    /** GitHub isn't configured on this server — nothing to do here. */
    NOT_CONFIGURED,
    /** No GitHub account linked → Connect GitHub. */
    NOT_LINKED,
    /** Linked, token dead → Reconnect GitHub. */
    EXPIRED,
    /** Linked, the App installed nowhere the token sees → Install the app. */
    NOT_INSTALLED,
}

/** The first missing prerequisite, null once repositories can be listed (iOS `GithubConnect.prerequisite`). */
fun githubPrerequisite(configured: Boolean, linked: Boolean, needsReconnect: Boolean, installed: Boolean): GithubPrerequisite? =
    when {
        !configured -> GithubPrerequisite.NOT_CONFIGURED
        !linked -> GithubPrerequisite.NOT_LINKED
        needsReconnect -> GithubPrerequisite.EXPIRED
        !installed -> GithubPrerequisite.NOT_INSTALLED
        else -> null
    }

@Serializable
private data class StatusInput(
    val teamId: String,
    // The connect/install hops are mobile-marked: the guided page's done
    // state then fires the exponential://github-connected deep link back
    // into the app instead of continuing in the browser.
    val platform: String? = null,
)

@Serializable
private data class ReposInput(
    val teamId: String,
    val refresh: Boolean? = null,
    val platform: String? = null,
)

// FEED-30: `integrations.github.lookupRepo` input — the picker's "Add by
// name" field. Internal so the wire-format test can lock it.
@Serializable
internal data class LookupRepoInput(
    val teamId: String,
    val fullName: String,
)

/** `{}` — the body of a mutation that takes no input. */
@Serializable
internal object DisconnectInput

@Singleton
class IntegrationsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun githubStatus(accountId: String, teamId: String): GithubStatusResult =
        trpc.query(
            accountId,
            path = "integrations.github.status",
            input = StatusInput(teamId = teamId, platform = "mobile"),
            inputSerializer = StatusInput.serializer(),
            outputSerializer = GithubStatusResult.serializer(),
        )

    /**
     * The repositories the viewer may add: every push-able repo of every
     * installation their token sees, deduped and sorted. [refresh] bypasses
     * the server's per-user discovery cache so returning from a GitHub hop
     * reflects new repos. Always mobile-marked.
     */
    suspend fun githubRepos(
        accountId: String,
        teamId: String,
        refresh: Boolean = false,
    ): GithubReposResult =
        trpc.query(
            accountId,
            path = "integrations.github.repos",
            input = ReposInput(
                teamId = teamId,
                refresh = if (refresh) true else null,
                platform = "mobile",
            ),
            inputSerializer = ReposInput.serializer(),
            outputSerializer = GithubReposResult.serializer(),
        )

    /**
     * FEED-30: the Add-repository picker's "Add by name" escape hatch
     * (`integrations.github.lookupRepo`). Resolves an `owner/name` through
     * EXACTLY the connect gate (the viewer's token, push access, the App
     * installed), so a failure's message names the real reason and is shown
     * verbatim. Read-only; the result is exactly a picker row.
     */
    suspend fun lookupRepo(accountId: String, teamId: String, fullName: String): GithubPickerRepo =
        trpc.query(
            accountId,
            path = "integrations.github.lookupRepo",
            input = LookupRepoInput(teamId = teamId, fullName = fullName),
            inputSerializer = LookupRepoInput.serializer(),
            outputSerializer = GithubPickerRepo.serializer(),
        )

    /**
     * Disconnect the viewer's own GitHub account (`integrations.github.disconnect`).
     * Repositories already added keep working — their tokens mint off the App
     * installation, not off this person's token. The `{ ok: true }` response
     * is ignored.
     */
    suspend fun githubDisconnect(accountId: String) =
        trpc.mutationUnit(
            accountId,
            path = "integrations.github.disconnect",
            input = DisconnectInput,
            inputSerializer = DisconnectInput.serializer(),
        )
}
