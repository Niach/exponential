package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.nullable

// Mirrors apps/web/src/lib/trpc/repositories.ts + boards.ts. Repositories are
// server-only (NOT an Electric shape) — read on demand over tRPC for the
// team-settings registry and the create-board / retarget pickers.
// The GitHub-App install/connect hop runs in-app via a Custom Tab (EXP-45);
// registering a picked repo in the registry is `add` (EXP-225).

/**
 * A board that points at a repo, computed from `boards.repository_id`
 * (masterplan v4 §3.2 — `repositories.list` no longer returns join rows).
 * Powers the settings "used by" chips and the picker "in use" hints.
 */
@Serializable
data class RepoBoardRef(
    val id: String,
    val name: String,
    val slug: String,
)

/**
 * One connected repo in the team registry (`repositories.list` row).
 * `private` is a Kotlin keyword so it's mapped via @SerialName.
 */
@Serializable
data class TeamRepo(
    val id: String,
    val fullName: String,
    val defaultBranch: String = "main",
    @SerialName("private") val isPrivate: Boolean = false,
    // v4: the boards backed by this repo (many for a monorepo).
    val boards: List<RepoBoardRef> = emptyList(),
)

@Serializable
private data class RepoTeamIdInput(val teamId: String)

@Serializable
private data class RepositoryIdInput(val repositoryId: String)

@Serializable
private data class AddRepoInput(
    val teamId: String,
    val fullName: String,
    val defaultBranch: String,
    @SerialName("private") val isPrivate: Boolean,
)

@Serializable
private data class SetRepositoryInput(
    val boardId: String,
    val repositoryId: String,
)

@Serializable
private data class BranchDiffInput(@SerialName("issueId") val issueId: String)

@Singleton
class RepositoriesApi @Inject constructor(private val trpc: TrpcClient) {

    /** Member-readable: the team's repos with their backing boards. */
    suspend fun list(accountId: String, teamId: String): List<TeamRepo> =
        trpc.query(
            accountId,
            path = "repositories.list",
            input = RepoTeamIdInput(teamId),
            inputSerializer = RepoTeamIdInput.serializer(),
            outputSerializer = ListSerializer(TeamRepo.serializer()),
        )

    /**
     * Owner-only (server-enforced): register a repo reachable through one of the
     * team's linked GitHub accounts (`repositories.add`, web parity —
     * repositories-section.tsx). The `{repository}` response is discarded;
     * callers re-fetch the registry list.
     */
    suspend fun add(
        accountId: String,
        teamId: String,
        fullName: String,
        defaultBranch: String,
        isPrivate: Boolean,
    ) =
        trpc.mutationUnit(
            accountId,
            path = "repositories.add",
            input = AddRepoInput(teamId, fullName, defaultBranch, isPrivate),
            inputSerializer = AddRepoInput.serializer(),
        )

    /**
     * Owner-only (server-enforced): remove a repo. Blocked (CONFLICT — "repository
     * backs N boards") while any board still points at it, via the
     * `boards.repository_id` FK `restrict`.
     */
    suspend fun remove(accountId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "repositories.remove",
            input = RepositoryIdInput(repositoryId),
            inputSerializer = RepositoryIdInput.serializer(),
        )

    /**
     * Owner/admin: retarget a board's backing repo (masterplan v4 §3.2 —
     * `boards.setRepository`, replacing the deleted link/unlink/setPrimary).
     */
    suspend fun setRepository(accountId: String, boardId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "boards.setRepository",
            input = SetRepositoryInput(boardId, repositoryId),
            inputSerializer = SetRepositoryInput.serializer(),
        )

    /**
     * Member-gated middle tier of remote Changes visibility (masterplan v4 §4.8,
     * L18): the issue's `exp/<IDENTIFIER>` branch compared against the repo
     * default branch, returned in the shared `prFiles` shape (reuses [PrFilesResult]).
     * Null when the branch was never pushed (the caller falls through to the
     * "being coded on <device>" tier).
     */
    suspend fun branchDiff(accountId: String, issueId: String): PrFilesResult? =
        trpc.query(
            accountId,
            path = "repositories.branchDiff",
            input = BranchDiffInput(issueId),
            inputSerializer = BranchDiffInput.serializer(),
            outputSerializer = PrFilesResult.serializer().nullable,
        )
}
