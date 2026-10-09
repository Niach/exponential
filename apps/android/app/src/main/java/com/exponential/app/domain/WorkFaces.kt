package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

// EXP-893: the PHONE's Work screen — one screen per subject (an issue, or a
// session) with up to three FACES held as screen state, never as navigation:
// Issue, Run and Guide (EXP-1251: Changes + Results merged into the Guide,
// its diff counts moved into the body). EXP-1150: the faces are TABS —
// a segmented strip under the top bar (the ONE segmented control every list
// strip wears) names every available face in its fixed order, and a
// horizontal swipe on the face's body moves to the neighbour ([swipeTarget]).
// EXP-1152: that swipe is a native PAGER (the neighbour follows the finger).
// The header band carries the strip with the Merge PR pill at its end (every
// face); the bottom bar keeps only the face's OWN controls, and Stop / Resume
// sit in the top bar's trailing slot while the Run face shows. These are
// the PURE rules every phone client mirrors byte for byte: web
// `lib/work-faces.ts` (the spec and its tests), iOS `ExpCore/Domain/WorkFaces.swift` — same names, same cases,
// same test names (`WorkFacesTest`).

/**
 * The three faces. EXP-1251: [Guide] = the run's Guide (its published
 * sections + pictures) over the diff (the run's live diff, else the issue's
 * open PR).
 */
enum class WorkFaceKind { Issue, Run, Guide }

const val ISSUE_FACE_LABEL = "Issue"
const val RUN_FACE_LABEL = "Run"
/** EXP-886: the Run face's label with MORE THAN ONE own run on the issue. */
const val RUNS_FACE_LABEL = "Runs"
/** EXP-1251: the Guide face (contract `diffUi.guideFace`). */
val GUIDE_FACE_LABEL: String = DomainContract.diffUiGuideFace
/** EXP-933: the transcript card under a settled `sessions_guide` call that
 *  switches the run to its Guide face. */
val OPEN_RESULTS_LABEL: String = "Open $GUIDE_FACE_LABEL"
/** The Start circle's label — the Run face's bar once the shown run ended
 *  for good, the Issue face's bar while the issue can start. */
const val START_CODING_LABEL = "Start coding"

/** The steer composer's placeholder, byte-identical ×4 (desktop
 *  `steer-composer.tsx` / `session_screen.rs`). */
const val STEER_COMPOSER_PLACEHOLDER = "Type / for commands"
/** The composer footer's plan-mode word, blue while plan mode is on. */
const val PLAN_MODE_LABEL = "Plan mode"

fun faceLabel(face: WorkFaceKind, multipleRuns: Boolean = false): String = when (face) {
    WorkFaceKind.Issue -> ISSUE_FACE_LABEL
    WorkFaceKind.Run -> if (multipleRuns) RUNS_FACE_LABEL else RUN_FACE_LABEL
    WorkFaceKind.Guide -> GUIDE_FACE_LABEL
}

/** The faces a subject can show, in their fixed order. EXP-1251: the Guide
 *  shows when the run published results OR there is a diff (live, PR or
 *  branch), independent of Run: an issue with an open PR and no run of mine
 *  still has its PR files. It comes last: what the run did, then its Guide. */
fun availableFaces(
    hasIssue: Boolean,
    hasRun: Boolean,
    hasResults: Boolean,
    hasDiff: Boolean,
): List<WorkFaceKind> =
    buildList {
        if (hasIssue) add(WorkFaceKind.Issue)
        if (hasRun) add(WorkFaceKind.Run)
        if (hasResults || hasDiff) add(WorkFaceKind.Guide)
    }

/**
 * EXP-1251: a route's `?face=` value as a face (web `parseGuideSearch`'s
 * legacy mapping): `guide`, and the old `results` / `changes`, land on the
 * Guide; `issue` / `run` stand; anything else is null (the subject's default).
 */
fun workFaceFromParam(raw: String?): WorkFaceKind? = when (raw?.trim()?.lowercase()) {
    "issue" -> WorkFaceKind.Issue
    "run" -> WorkFaceKind.Run
    "guide", "results", "changes", "diff" -> WorkFaceKind.Guide
    else -> null
}

// ── EXP-1251: the Guide's section pages ─────────────────────────────────────
// A Changes row opens its section's changes as a page under the Guide: a
// numbered section, the Summary's own files ([GuideSectionKey.Lead]), the
// automatic Other changes ([GuideSectionKey.Other]) or Show complete diff
// ([GuideSectionKey.All]). Web `guideSectionPage` (`lib/work-faces.ts`).

sealed class GuideSectionKey {
    data class Numbered(val index: Int) : GuideSectionKey()
    data object Lead : GuideSectionKey()
    data object Other : GuideSectionKey()
    data object All : GuideSectionKey()
}

/** A section page: the back row's caption (`02 / 06`, numbered sections
 *  only), its title, the files it covers and their summed counts. */
data class GuideSectionPage(
    val section: GuideSectionKey,
    val caption: String?,
    val title: String,
    val files: List<Diff.File>,
    val additions: Int,
    val deletions: Int,
)

/** The page [section] opens over [files] (the diff the Guide counts): the
 *  same coverage the Guide's rows read, so a page never shows a file its row
 *  did not count. Null = no such section (a stale link) or no diff loaded.
 *  [numbered] = false for the PR-body fallback (an unnumbered Guide): its
 *  section page carries no `01 / 01` caption, as its band shows no number. */
fun guideSectionPage(
    groups: List<SessionResultGroup>,
    files: List<Diff.File>?,
    section: GuideSectionKey,
    numbered: Boolean = true,
): GuideSectionPage? {
    if (files == null) return null
    val coverage = guideCoverage(groups, files)
    fun page(title: String, caption: String?, set: GuideChangeSet?): GuideSectionPage? =
        set?.let { GuideSectionPage(section, caption, title, it.files, it.additions, it.deletions) }
    return when (section) {
        GuideSectionKey.All -> page(GUIDE_CHANGES_TOPIC, null, coverage.complete)
        GuideSectionKey.Other -> coverage.other?.let { page(it.topic, null, it.changes) }
        GuideSectionKey.Lead -> coverage.lead?.let { page(it.group.topic, null, it.changes) }
        is GuideSectionKey.Numbered -> coverage.sections.firstOrNull { it.index == section.index }?.let {
            page(it.group.topic, if (numbered) guideSectionCaption(it.index, it.total) else null, it.changes)
        }
    }
}

/** EXP-1154: the PR-body fallback's band label without a PR title (web
 *  `PR_DESCRIPTION_FALLBACK_TOPIC`). */
const val PR_FALLBACK_TITLE = "Pull request"

/** EXP-1154: the PR-body fallback's text for a blank body (web `PR_DESCRIPTION_EMPTY`). */
const val PR_FALLBACK_EMPTY_BODY = "No description."

/**
 * EXP-1154 (web `prDescriptionGroups`): the open PR's GitHub body as ONE
 * Guide group: band = the PR title (else `Pull request`), text = the body
 * (`No description.` when blank). EXP-1251: no report + a PR = ONE Changes
 * section, so the group claims EVERY diff path: its band carries the one
 * Changes row, nothing is left for `Other changes`, and `Show complete diff`
 * still closes the page.
 */
fun prDescriptionGroup(title: String?, body: String?, files: List<Diff.File>?): SessionResultGroup =
    SessionResultGroup(
        topic = title?.trim()?.takeIf { it.isNotEmpty() } ?: PR_FALLBACK_TITLE,
        entries = emptyList(),
        text = body?.trim()?.takeIf { it.isNotEmpty() } ?: PR_FALLBACK_EMPTY_BODY,
        files = files.orEmpty().map { it.path },
    )

/** The section page's back-row summary: `+A −D · N files`. */
fun guideSectionSummary(page: GuideSectionPage): String =
    "${Diff.additionsLabel(page.additions)} ${Diff.deletionsLabel(page.deletions)} · ${guideFileCountLabel(page.files.size)}"

private fun stamp(value: String): Long = WireTimestamps.parseEpochMs(value) ?: 0L

/** Live by status AND heartbeat: a `running` row whose machine went quiet
 *  past the staleness window is neither steerable nor stoppable. */
fun isSessionLive(row: CodingSessionEntity, nowMs: Long): Boolean =
    // EXP-888: a sweep end (`ended_by = 'stale'`) is NOT an end — the host
    // ignores the flip and heartbeats the row back to `running`.
    (row.status == DomainContract.codingSessionStatusRunning ||
        row.status == DomainContract.codingSessionStatusInReview ||
        runIsStaleEnd(row)) &&
        !CodingSessionLiveness.isStale(row.updatedAt, nowMs)

private fun newest(rows: List<CodingSessionEntity>): CodingSessionEntity? {
    var best: CodingSessionEntity? = null
    for (row in rows) {
        val current = best
        if (current == null ||
            stamp(row.startedAt) > stamp(current.startedAt) ||
            (stamp(row.startedAt) == stamp(current.startedAt) && row.id > current.id)
        ) {
            best = row
        }
    }
    return best
}

/** The run an issue's Work screen shows — desktop `work_header.rs`
 *  `coding_target`: the bound run when it is mine and live, else my newest
 *  live run on the issue, else my newest run at all. `null` = no run of mine. */
fun codingTarget(
    rows: List<CodingSessionEntity>,
    issueId: String,
    boundId: String?,
    me: String?,
    nowMs: Long,
): CodingSessionEntity? {
    if (me == null) return null
    val mine = rows.filter { it.issueId == issueId && it.userId == me }
    val live = mine.filter { isSessionLive(it, nowMs) }
    if (boundId != null) {
        live.firstOrNull { it.id == boundId }?.let { return it }
    }
    return newest(live) ?: newest(mine)
}

/**
 * EXP-933: the run whose Guide an ISSUE shows — [codingTarget] when that run
 * has results, else the NEWEST run on the issue (startedAt, ties: larger id)
 * by ANY member with results, else (EXP-1251) the newest run anywhere whose
 * topics are tagged with the issue's PR [issuePrUrl] (a run that stacked this
 * issue's PR on its own); null = no results. Mirrors web `issueResultsRun`
 * (fixture `session-results.json`).
 */
fun issueResultsRun(
    rows: List<CodingSessionEntity>,
    issueId: String,
    boundId: String?,
    me: String?,
    nowMs: Long,
    issuePrUrl: String? = null,
): CodingSessionEntity? {
    val own = codingTarget(rows, issueId, boundId, me, nowMs)
    if (own != null && hasSessionResults(own.results)) return own
    val onIssue = newest(rows.filter { it.issueId == issueId && hasSessionResults(it.results) })
    if (onIssue != null || issuePrUrl == null) return onIssue
    return newest(rows.filter { issuePrUrl in sessionResultPrUrls(it.results) })
}

/**
 * EXP-934: the top bar's `…` CONTEXT MENU (Share · Move to board · Unmark
 * duplicate · Delete issue) belongs to the ISSUE, so it shows on the Issue
 * face alone. On Run and Guide the trailing slot carries the run's
 * own verb (Stop / Resume) and nothing else — a Delete issue sitting beside a
 * running agent acts on a subject that face is not even showing.
 *
 * Mirrored ×4 (web `faceShowsContextMenu`, iOS `faceShowsContextMenu`).
 */
fun faceShowsContextMenu(face: WorkFaceKind): Boolean = face == WorkFaceKind.Issue

enum class PrimaryAction { Stop, Resume, Start, None }

/** The top bar's trailing verb on the Run face, and the bottom-right circle's
 *  start glyph: Stop wins over everything; Resume needs an ended own run a
 *  machine can take; Start only for an issue subject that can be coded on. */
fun primaryAction(ownLive: Boolean, ownEndedResumable: Boolean, canStart: Boolean): PrimaryAction =
    when {
        ownLive -> PrimaryAction.Stop
        ownEndedResumable -> PrimaryAction.Resume
        canStart -> PrimaryAction.Start
        else -> PrimaryAction.None
    }

/** EXP-1150: which way the finger went. Left = the content followed the
 *  finger leftwards, so the NEXT face slides in; Right = the previous. */
enum class SwipeDirection { Left, Right }

/** The face a horizontal swipe on the body lands on: the neighbour in the
 *  strip's order, null at either end (or when the shown face is not in the
 *  strip at all — nothing to swipe from). */
fun swipeTarget(faces: List<WorkFaceKind>, shown: WorkFaceKind, direction: SwipeDirection): WorkFaceKind? {
    val index = faces.indexOf(shown)
    if (index < 0) return null
    val next = if (direction == SwipeDirection.Left) index + 1 else index - 1
    return faces.getOrNull(next)
}

/** EXP-862: the ONE session-dot palette, mirrored from web `session-dot.ts`
 *  and the desktop's `session_dot_tone` — running/review emerald, needs-input
 *  amber, done sky, ended/paused muted. */
enum class SessionDotTone { Running, Review, NeedsInput, Done, Muted }

/** Where a face lands when it vanishes under the reader (the diff cleared
 *  and the results list was empty, the run row went): guide → run → issue.
 *  `null` = nothing left. */
fun fallbackFace(shown: WorkFaceKind, available: List<WorkFaceKind>): WorkFaceKind? {
    if (shown in available) return shown
    val order = when (shown) {
        WorkFaceKind.Guide -> listOf(WorkFaceKind.Run, WorkFaceKind.Issue)
        WorkFaceKind.Run -> listOf(WorkFaceKind.Issue)
        WorkFaceKind.Issue -> emptyList()
    }
    return order.firstOrNull { it in available } ?: available.firstOrNull()
}

/** EXP-1251: a Stack card swap is an ARRIVAL: the new member's issue row
 *  loads a frame late (no PR yet = no Guide), so the swapped-to Guide is held
 *  instead of falling back, until that row is in or the reader picks another
 *  face. True = keep waiting. */
fun holdSwappedGuide(shown: WorkFaceKind, wanted: WorkFaceKind, swappedRowLoaded: Boolean): Boolean =
    shown != wanted && wanted == WorkFaceKind.Guide && !swappedRowLoaded

/** The composer footer's model — the `model` config option's value, null
 *  when the engine reported none or a blank. */
fun sessionModel(config: SessionConfigState?): String? {
    val value = config?.options?.firstOrNull { it.id == "model" }?.value
    return if (value.isNullOrEmpty()) null else value
}

data class PhaseDot(val tone: SessionDotTone, val connecting: Boolean)

/** The state dot's tone off the viewer phase — the `PhaseDot` rule: emerald
 *  while live, amber while it waits on a human (or went quiet), muted once it
 *  is over or paused. `connecting` says whether to pulse it. */
fun phaseDotTone(
    live: Boolean,
    connecting: Boolean,
    awaitingInput: Boolean,
    paused: Boolean,
    stale: Boolean,
): PhaseDot {
    val pulse = !paused && connecting
    val tone = when {
        !paused && live -> if (awaitingInput || stale) SessionDotTone.NeedsInput else SessionDotTone.Running
        else -> SessionDotTone.Muted
    }
    return PhaseDot(tone, pulse)
}

/**
 * EXP-935: whether the Work screen pops back to the list now that the run it
 * shows has ended. An ISSUE-bound subject never does (the screen stays, the
 * pill flips Stop → Resume); an issue-less run — a chat, a batch — does, but
 * ONLY once nothing is still landing in its place: a Resume and an account
 * switch both END the live run before the continuation row syncs, and a screen
 * that popped in that gap took the reader off the very run they just moved
 * (the successor then opened behind them, or not at all).
 *
 * [wasLive] keeps it edge-triggered — a screen opened straight onto an already
 * ended run stays put — and a continuation that never lands clears the flag,
 * so the pop falls back to happening then.
 */
fun shouldAutoBack(
    ended: Boolean,
    wasLive: Boolean,
    issueId: String?,
    continuationPending: Boolean,
): Boolean = ended && wasLive && issueId == null && !continuationPending

/**
 * Desktop `coding_action` (EXP-877): the verb an issue subject's coding
 * control wears, off the synced target row — the phone's top bar reads the
 * same rule so Stop / Resume / Start never disagree with the IDE.
 */
fun codingAction(
    target: CodingSessionEntity?,
    me: String?,
    nowMs: Long,
    resumable: Boolean,
    canStart: Boolean,
): PrimaryAction {
    val own = target != null && me != null && target.userId == me
    return primaryAction(
        ownLive = own && isSessionLive(target!!, nowMs),
        // EXP-888: a sweep end is not an end — never offer Resume on it.
        ownEndedResumable = own && runHasEnded(target!!) && resumable,
        canStart = canStart,
    )
}

// ── EXP-1175: the Run face's status row + Show work ─────────────────────────
// The Run face opens as the THREAD: one status row (the agent's run mark as
// the spinner, this caption, the last tool line muted, `Show work` on the
// right) over the run's published results in publish order ([sessionThread]),
// pending plan/question cards still in place. Show work swaps the thread for
// the full transcript IN PLACE; the choice is remembered per user. Fixture
// `packages/domain-contract/fixtures/run-row.json` (×4, web `work-faces.ts`).

/** The viewer's Show work preference before they ever touched it. */
const val SHOW_WORK_DEFAULT = false
const val SHOW_WORK_LABEL = "Show work"
const val HIDE_WORK_LABEL = "Hide work"

/** The row's trailing button: what pressing it DOES next. */
fun showWorkLabel(showWork: Boolean): String = if (showWork) HIDE_WORK_LABEL else SHOW_WORK_LABEL

/** The ×4 display state plus the two the row alone tells apart: a paused
 *  (offline host) run and an ended one. [wire] = the fixture's spelling. */
enum class RunRowState(val wire: String) {
    Working("working"),
    NeedsInput("needs_input"),
    Paused("paused"),
    Review("review"),
    Done("done"),
    Ended("ended"),
}

/** The caption's colour, by NAME — the UI maps it onto the session row's
 *  status-line colours. [wire] = the fixture's spelling. */
enum class RunRowTone(val wire: String) { Muted("muted"), Amber("amber"), Emerald("emerald"), Sky("sky") }

data class RunRowCaption(val text: String, val tone: RunRowTone)

/**
 * The row's state: the VIEWER's live signals folded over the synced ×4
 * display state, so the row never contradicts the Run tab's mark — paused
 * (offline host) first, then ended, then NeedsInput when the viewer sees a
 * pending plan/question ([awaitingInput]) or the display state says so, then
 * Working when the viewer's working predicate ([working] = `agentWorking`) or
 * the display state says so, else the display state (Review | Done).
 * Fixture `run-row.json` `states` (×4).
 */
fun runRowState(
    paused: Boolean,
    ended: Boolean,
    awaitingInput: Boolean,
    working: Boolean,
    display: CodingSessionDisplayState,
): RunRowState = when {
    paused -> RunRowState.Paused
    ended -> RunRowState.Ended
    awaitingInput || display == CodingSessionDisplayState.NeedsInput -> RunRowState.NeedsInput
    working || display == CodingSessionDisplayState.Working -> RunRowState.Working
    display == CodingSessionDisplayState.Review -> RunRowState.Review
    else -> RunRowState.Done
}

private fun runRowStamp(value: String?): Long? = value?.let { WireTimestamps.parseEpochMs(it) }

/**
 * The status row's first line and tone: `Building on <device> · <elapsed>`
 * while it works (now − start), `Ended on <device> · <elapsed>` once it is
 * over (end − start; the caller passes `ended_at`, else `updated_at`), else
 * the session list row's words. The elapsed part is the working caption's
 * ladder ([formatDurationMs]) and drops when a stamp is missing or unparsable.
 */
fun runRowCaption(
    state: RunRowState,
    device: String,
    startedAt: String?,
    endedAt: String?,
    nowMs: Long,
): RunRowCaption = when (state) {
    RunRowState.Paused -> RunRowCaption("Paused · $device", RunRowTone.Muted)
    RunRowState.NeedsInput -> RunRowCaption("Needs input · $device", RunRowTone.Amber)
    RunRowState.Review -> RunRowCaption("Ready for review · $device", RunRowTone.Emerald)
    RunRowState.Done -> RunRowCaption("Done · $device", RunRowTone.Sky)
    RunRowState.Working, RunRowState.Ended -> {
        val verb = if (state == RunRowState.Working) "Building on" else "Ended on"
        val start = runRowStamp(startedAt)
        val end = if (state == RunRowState.Working) nowMs else runRowStamp(endedAt)
        val elapsed = if (start != null && end != null) " · ${formatDurationMs(end - start)}" else ""
        RunRowCaption("$verb $device$elapsed", RunRowTone.Muted)
    }
}

// ── EXP-1245: one status row PER TURN ───────────────────────────────────────
// The owner's thread is a conversation of turns ([sessionTurns]): every turn
// draws its own status row. A settled turn reads `Done on <device> · <turn
// duration>`; the open (newest) turn reads the run's row ([runRowCaption])
// timed from the TURN's start, not the run's; a sent message still waiting
// for its turn has no row. Fixture `run-row.json` `turnCaptions` (x4).

/** [turnRowCaption] off a turn's edges (epoch ms); [runEndedAt] = the run's
 *  end (`ended_at`, else `updated_at`) for an open turn of an ended run. */
fun turnRowCaption(
    startedAt: Long?,
    endedAt: Long?,
    state: RunRowState,
    device: String,
    runEndedAt: String?,
    nowMs: Long,
    /** Web M6: false = the turn's end was not observed ([firstTurnEndKnown]),
     *  so the settled caption names no duration. */
    endKnown: Boolean = true,
): RunRowCaption? {
    val start = startedAt ?: return null
    if (endedAt != null) {
        if (!endKnown) return RunRowCaption("Done on $device", RunRowTone.Muted)
        return RunRowCaption("Done on $device · ${formatDurationMs(endedAt - start)}", RunRowTone.Muted)
    }
    return when (state) {
        RunRowState.Working, RunRowState.Ended -> {
            val verb = if (state == RunRowState.Working) "Building on" else "Ended on"
            val end = if (state == RunRowState.Working) nowMs else runRowStamp(runEndedAt)
            val elapsed = if (end != null) " · ${formatDurationMs(end - start)}" else ""
            RunRowCaption("$verb $device$elapsed", RunRowTone.Muted)
        }
        else -> runRowCaption(state, device, null, runEndedAt, nowMs)
    }
}

/** The turn's caption ([turnRowCaption]) for a [SessionTurn]. */
fun turnRowCaption(
    turn: SessionTurn,
    state: RunRowState,
    device: String,
    runEndedAt: String?,
    nowMs: Long,
    endKnown: Boolean = true,
): RunRowCaption? = turnRowCaption(turn.startedAt, turn.endedAt, state, device, runEndedAt, nowMs, endKnown)
