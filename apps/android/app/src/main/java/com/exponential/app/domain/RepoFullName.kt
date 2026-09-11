package com.exponential.app.domain

// FEED-30: the ONE "owner/name" shape every repo-by-name entry point accepts —
// mirror of the web REPO_FULL_NAME_RE (`/^[^/\s]+\/[^/\s]+$/`, in
// lib/repo-full-name.ts): exactly one slash, both halves non-empty, no
// whitespace anywhere. The picker's "Add by name" field validates against it
// so a name the client lets through is never one the server rejects on shape
// alone.
private val REPO_FULL_NAME = Regex("""^[^/\s]+/[^/\s]+$""")

fun isRepoFullName(value: String): Boolean = REPO_FULL_NAME.matches(value)

// FEED-32 (web board-repo-field.tsx `triggerLabel`): the board form's
// Repository select label, rendered EXPLICITLY and never blank. A linked repo
// the local list doesn't know reads "Loading repository…" while the one-shot
// re-list for that id is in flight (or the first load is), and "Repository
// unavailable" once the list came back without it; an unlinked board reads
// "No repository" (or "Loading…" before the first list).
object BoardRepoLabel {
    const val NO_REPOSITORY = "No repository"
    const val LOADING = "Loading…"
    const val LOADING_REPOSITORY = "Loading repository…"
    const val UNAVAILABLE = "Repository unavailable"

    /**
     * @param selectedName the resolved (inline or registry) repo name, if any
     * @param repositoryId the board's linked registry id, if any
     * @param loading the first list hasn't arrived yet
     * @param resolving the one-shot re-list for [repositoryId] is in flight
     */
    fun trigger(
        selectedName: String?,
        repositoryId: String?,
        loading: Boolean,
        resolving: Boolean,
    ): String = when {
        selectedName != null -> selectedName
        repositoryId == null -> if (loading) LOADING else NO_REPOSITORY
        loading || resolving -> LOADING_REPOSITORY
        else -> UNAVAILABLE
    }
}
