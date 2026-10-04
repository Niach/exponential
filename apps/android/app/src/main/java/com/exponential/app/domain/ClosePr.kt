package com.exponential.app.domain

// EXP-1154: the Close PR copy, byte-identical x4 (fixture
// `packages/domain-contract/fixtures/close-pr.json`). The menu item itself is
// the contract's [DomainContract.diffUiClosePr] with the `prClosed` glyph,
// destructive, members only while the PR is open; the confirm below; a failure
// toasts the merge-failure message.
object ClosePr {
    const val TITLE = "Close pull request?"
    const val BODY = "Closes the pull request on GitHub without merging. Use this when the issue " +
        "was dropped even though the work exists. The branch is kept and the PR can be reopened on GitHub."
    const val BATCH_LINE = "It also closes the pull request for {n} linked issues."
    const val CONFIRM = "Close PR"

    /** The confirm's body; [linkedIssues] = the OTHER issues the PR links (a batch PR). */
    fun body(linkedIssues: Int): String =
        if (linkedIssues > 0) "$BODY ${BATCH_LINE.replace("{n}", linkedIssues.toString())}" else BODY
}
