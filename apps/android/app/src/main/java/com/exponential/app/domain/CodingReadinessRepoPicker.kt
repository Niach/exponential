package com.exponential.app.domain

/**
 * EXP-1121: the "Ready to code?" sheet's inline repository picker — which of
 * the team's repositories to offer a board with none, in what order, and what
 * small tag each row wears. Pure (iOS `CodingReadinessRepoPicker` mirrors the
 * same rule):
 *
 * - search = case-insensitive substring of the FULL name (`owner/name`);
 * - order = the repos that MATCH the board first, then the rest, both in the
 *   API's original order (stable);
 * - match = the repo NAME (after the `/`), lowercased, equals the board's
 *   lowercased name or slug;
 * - tag = [CodingReadiness.Copy.PICKER_MATCHES_BOARD] on a match, else
 *   [CodingReadiness.pickerUsedBy] naming the first OTHER board using it,
 *   else none.
 */
object CodingReadinessRepoPicker {

    data class Board(val id: String, val name: String)

    /** One team repository (`repositories.list` row) and the boards it backs. */
    data class Repo(val id: String, val fullName: String, val boards: List<Board>)

    data class Row(val repo: Repo, val matchesBoard: Boolean, val tag: String?)

    fun matchesBoard(fullName: String, boardName: String, boardSlug: String): Boolean {
        val name = fullName.substringAfter('/').lowercase()
        return name == boardName.lowercase() || name == boardSlug.lowercase()
    }

    fun rows(
        repos: List<Repo>,
        boardId: String,
        boardName: String,
        boardSlug: String,
        query: String,
    ): List<Row> {
        val needle = query.trim().lowercase()
        val all = repos
            .filter { needle.isEmpty() || it.fullName.lowercase().contains(needle) }
            .map { repo ->
                val matches = matchesBoard(repo.fullName, boardName, boardSlug)
                val tag = when {
                    matches -> CodingReadiness.Copy.PICKER_MATCHES_BOARD
                    else -> repo.boards.firstOrNull { it.id != boardId }
                        ?.let { CodingReadiness.pickerUsedBy(it.name) }
                }
                Row(repo, matches, tag)
            }
        // `partition` keeps each half in its original order: a stable sort.
        val (matching, rest) = all.partition { it.matchesBoard }
        return matching + rest
    }
}
