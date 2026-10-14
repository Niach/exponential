package com.exponential.app.ui.work

import androidx.compose.runtime.Composable
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction

/**
 * EXP-1291: the stack merge over `pull_requests` rows (EXP-1248's ONE stack
 * confirm, `stack-merge-choice.json` `confirm`, re-keyed from issue rows to
 * the PR's own `base_pull_request_id`, else head == base in one repository).
 * Only a LINEAR open stack of 2+ PRs (no fork) has one; Merge = `pullRequests.merge({pullRequestId: top, stack:
 * true})`, landing the whole open chain bottom-up.
 */
object PullRequestStack {
    /** The confirm: [title] IS the primary button, [throughId] the PR merged
     *  with `stack = true`. */
    data class Confirm(val title: String, val body: String, val throughId: String)

    private fun PullRequestEntity.isOpen() = state == DomainContract.prStateOpen

    /**
     * The PR [pr] is based on among [open]: the explicit
     * `base_pull_request_id`, else (EXP-1306, desktop `pr_stack::lower_of`)
     * the first PR of the same repository whose head is [pr]'s base.
     */
    private fun lowerOf(pr: PullRequestEntity, open: List<PullRequestEntity>): PullRequestEntity? {
        pr.basePullRequestId?.takeIf { it.isNotBlank() }
            ?.let { id -> open.firstOrNull { it.id == id && it.id != pr.id } }
            ?.let { return it }
        val base = pr.base.trim().takeIf { it.isNotEmpty() } ?: return null
        return open.firstOrNull { it.id != pr.id && it.repositoryId == pr.repositoryId && it.head.trim() == base }
    }

    /** The open linear chain [pr] sits in, bottom-first; null on a fork. */
    fun openChain(pr: PullRequestEntity, all: List<PullRequestEntity>): List<PullRequestEntity>? {
        if (!pr.isOpen()) return null
        val open = all.filter { it.isOpen() }
        fun uppers(of: PullRequestEntity) = open.filter { lowerOf(it, open)?.id == of.id }
        val below = mutableListOf<PullRequestEntity>()
        var cur = pr
        val seen = mutableSetOf(pr.id)
        while (true) {
            val base = lowerOf(cur, open) ?: break
            if (!seen.add(base.id)) break
            below.add(0, base)
            cur = base
        }
        val chain = below + pr
        // Walking up: a member with 2+ open children is a fork — no stack.
        for (member in chain) {
            if (uppers(member).size > 1) return null
        }
        cur = pr
        val above = mutableListOf<PullRequestEntity>()
        while (true) {
            val child = uppers(cur).firstOrNull() ?: break
            if (!seen.add(child.id)) break
            if (uppers(child).size > 1) return null
            above += child
            cur = child
        }
        return chain + above
    }

    private fun label(pr: PullRequestEntity) = pr.number?.let { "#$it" } ?: pr.head

    /**
     * Non-null = [pr] is a member of an open stack: Merge lands it all.
     * [through] = the rail's Merge through here on that member: it lands that
     * member and everything beneath it, the rest stays open.
     */
    fun stackConfirm(
        pr: PullRequestEntity,
        all: List<PullRequestEntity>,
        through: PullRequestEntity? = null,
    ): Confirm? {
        val chain = openChain(pr, all) ?: return null
        if (chain.size < 2) return null
        val index = if (through == null) chain.lastIndex else chain.indexOfFirst { it.id == through.id }
        if (index < 0) return null
        val landing = chain.subList(0, index + 1).map(::label)
        val staysOpen = chain.subList(index + 1, chain.size).map(::label)
        val lands = if (landing.size == 1) {
            "Lands 1 pull request: ${landing[0]}."
        } else {
            "Lands ${landing.size} pull requests, bottom-up: ${landing.joinToString(", ")}."
        }
        val open = when (staysOpen.size) {
            0 -> ""
            1 -> " ${staysOpen[0]} stays open."
            else -> " ${staysOpen.joinToString(", ")} stay open."
        }
        return Confirm(
            title = if (through == null) DomainContract.diffUiMergeStack else DomainContract.diffUiMergeThrough,
            body = lands + open,
            throughId = chain[index].id,
        )
    }

    /** One stack-rail row (desktop `pr_stack::StackViewRow`). */
    data class StackViewRow(
        val pullRequestId: String,
        /** The thread a tap opens; null = not synced. */
        val rootSessionId: String?,
        /** The representative issue's identifier, else `#<n>`. */
        val identifier: String,
        /** The representative issue's title, else the PR's own. */
        val title: String,
        /** The subject's PR: the active wash. */
        val isCurrent: Boolean,
    )

    /** The rail: rows TOP first + the bottom member's base (null = unknown). */
    data class StackView(val rows: List<StackViewRow>, val baseBranch: String?)

    /**
     * EXP-1306: the Guide's stack rail of [pr] (desktop `pr_stack::stack_view`);
     * null unless it sits in a linear open stack of 2+. A PR closing several
     * issues is named by its lowest identifier.
     */
    fun stackView(
        pr: PullRequestEntity,
        all: List<PullRequestEntity>,
        links: List<PullRequestIssueEntity>,
        issues: List<IssueEntity>,
    ): StackView? {
        val chain = openChain(pr, all) ?: return null
        if (chain.size < 2) return null
        val issuesById = issues.associateBy { it.id }
        return StackView(
            rows = chain.asReversed().map { row ->
                val issue = links.asSequence()
                    .filter { it.pullRequestId == row.id }
                    .mapNotNull { issuesById[it.issueId] }
                    .minByOrNull { it.identifier }
                StackViewRow(
                    pullRequestId = row.id,
                    rootSessionId = row.rootSessionId,
                    identifier = issue?.identifier ?: row.number?.let { "#$it" }.orEmpty(),
                    title = issue?.title ?: row.title.orEmpty(),
                    isCurrent = row.id == pr.id,
                )
            },
            baseBranch = chain.first().base.trim().takeIf { it.isNotEmpty() },
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
