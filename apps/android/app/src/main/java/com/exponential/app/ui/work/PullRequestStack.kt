package com.exponential.app.ui.work

import androidx.compose.runtime.Composable
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction

/**
 * EXP-1291: the stack merge over `pull_requests` rows (EXP-1248's ONE stack
 * confirm, `stack-merge-choice.json` `confirm`, re-keyed from issue rows to
 * the PR's own `base_pull_request_id`). Only a LINEAR open stack of 2+ PRs
 * (no fork) has one; Merge = `pullRequests.merge({pullRequestId: top, stack:
 * true})`, landing the whole open chain bottom-up.
 */
object PullRequestStack {
    /** The confirm: [title] IS the primary button, [throughId] the PR merged
     *  with `stack = true`. */
    data class Confirm(val title: String, val body: String, val throughId: String)

    private fun PullRequestEntity.isOpen() = state == DomainContract.prStateOpen

    /** The open linear chain [pr] sits in, bottom-first; null on a fork. */
    fun openChain(pr: PullRequestEntity, all: List<PullRequestEntity>): List<PullRequestEntity>? {
        if (!pr.isOpen()) return null
        val open = all.filter { it.isOpen() }
        val byId = open.associateBy { it.id }
        val below = mutableListOf<PullRequestEntity>()
        var cur = pr
        val seen = mutableSetOf(pr.id)
        while (true) {
            val base = cur.basePullRequestId?.let(byId::get) ?: break
            if (!seen.add(base.id)) break
            below.add(0, base)
            cur = base
        }
        val chain = below + pr
        // Walking up: a member with 2+ open children is a fork — no stack.
        for (member in chain) {
            if (open.count { it.basePullRequestId == member.id } > 1) return null
        }
        cur = pr
        val above = mutableListOf<PullRequestEntity>()
        while (true) {
            val child = open.firstOrNull { it.basePullRequestId == cur.id } ?: break
            if (!seen.add(child.id)) break
            if (open.count { it.basePullRequestId == child.id } > 1) return null
            above += child
            cur = child
        }
        return chain + above
    }

    private fun label(pr: PullRequestEntity) = pr.number?.let { "#$it" } ?: pr.head

    /** Non-null = [pr] is a member of an open stack: Merge lands it all. */
    fun stackConfirm(pr: PullRequestEntity, all: List<PullRequestEntity>): Confirm? {
        val chain = openChain(pr, all) ?: return null
        if (chain.size < 2) return null
        val labels = chain.map(::label)
        return Confirm(
            title = DomainContract.diffUiMergeStack,
            body = "Lands ${chain.size} pull requests, bottom-up: ${labels.joinToString(", ")}.",
            throughId = chain.last().id,
        )
    }
}

/** The confirm's dismiss (`stack-merge-choice.json` `confirm.labels.cancel`). */
internal const val STACK_CONFIRM_CANCEL_LABEL = "Cancel"

/**
 * EXP-1248: the ONE stack merge confirm — the shared [GlassAlert] card,
 * Cancel · the primary (the title, the default).
 */
@Composable
fun StackMergeDialog(
    confirm: PullRequestStack.Confirm,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    GlassAlert(
        title = confirm.title,
        body = confirm.body,
        onDismiss = onDismiss,
        trailing = listOf(
            GlassAlertAction(STACK_CONFIRM_CANCEL_LABEL, onClick = onDismiss),
            GlassAlertAction(
                confirm.title,
                primary = true,
                testTag = "merge-stack",
                onClick = {
                    onDismiss()
                    onConfirm()
                },
            ),
        ),
        defaultAction = 1,
    )
}
