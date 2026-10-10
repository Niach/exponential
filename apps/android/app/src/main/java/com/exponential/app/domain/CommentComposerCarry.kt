package com.exponential.app.domain

/**
 * What the comment composer keeps across a `bind(issueId)` (release train
 * 2026-10-10, F34): the draft text and its queued files belong to the issue
 * they were typed on. The Work screen's Stack-card swap rebinds ONE
 * screen-level `CommentThreadViewModel` to another issue, so a changed id
 * starts the composer empty; re-binding the same id (a recomposition, a
 * rotation) keeps everything.
 */
data class CommentComposerCarry<T>(val draft: String, val pending: List<T>)

fun <T> commentComposerAfterBind(
    boundIssueId: String?,
    issueId: String,
    draft: String,
    pending: List<T>,
): CommentComposerCarry<T> =
    if (boundIssueId == issueId) CommentComposerCarry(draft, pending) else CommentComposerCarry("", emptyList())
