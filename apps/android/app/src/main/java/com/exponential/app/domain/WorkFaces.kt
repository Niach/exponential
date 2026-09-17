package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

// EXP-893: the PHONE's Work screen — one screen per subject (an issue, or a
// session) with up to four FACES held as screen state, never as navigation:
// Issue, Run, Changes and Results (EXP-879). The desktop's face toggle (EXP-877) becomes a
// floating bottom-right circle that either switches straight to the one other
// face or opens a menu above itself; Stop / Resume sit in the top bar's
// trailing slot only while the Run face shows. These are the PURE rules every
// phone client mirrors byte for byte: web `lib/work-faces.ts` (the spec and
// its tests), iOS `ExpCore/Domain/WorkFaces.swift` — same names, same cases,
// same test names (`WorkFacesTest`).

/**
 * The four faces. [Changes] is the run's diff, else the issue's open PR;
 * [Results] (EXP-879) is the shown run's published screenshots and, like
 * Changes, is a SUB-FACE of Run — no run of mine, no results.
 */
enum class WorkFaceKind { Issue, Run, Changes, Results }

const val ISSUE_FACE_LABEL = "Issue"
const val RUN_FACE_LABEL = "Run"
/** EXP-886: the Run face's label with MORE THAN ONE own run on the issue. */
const val RUNS_FACE_LABEL = "Runs"
const val CHANGES_FACE_LABEL = "Changes"
/** EXP-879: the run's published screenshots. */
const val RESULTS_FACE_LABEL = "Results"
/** The switcher menu's extra row once the shown run ended for good. */
const val START_CODING_LABEL = "Start coding"

/** The steer composer's placeholder, byte-identical ×4 (desktop
 *  `steer-composer.tsx` / `session_screen.rs`). */
const val STEER_COMPOSER_PLACEHOLDER = "Type / for commands"
/** The composer footer's plan-mode word, blue while plan mode is on. */
const val PLAN_MODE_LABEL = "Plan mode"

fun faceLabel(face: WorkFaceKind, multipleRuns: Boolean = false): String = when (face) {
    WorkFaceKind.Issue -> ISSUE_FACE_LABEL
    WorkFaceKind.Run -> if (multipleRuns) RUNS_FACE_LABEL else RUN_FACE_LABEL
    WorkFaceKind.Changes -> CHANGES_FACE_LABEL
    WorkFaceKind.Results -> RESULTS_FACE_LABEL
}

/** The faces a subject can show, in their fixed order. Changes is independent
 *  of Run: an issue with an open PR and no run of mine still has its PR files.
 *  Results (EXP-879) comes LAST and is not: it is the shown run's own output,
 *  so it only ever appears beside a Run face. */
fun availableFaces(
    hasIssue: Boolean,
    hasRun: Boolean,
    hasChanges: Boolean,
    hasResults: Boolean,
): List<WorkFaceKind> =
    buildList {
        if (hasIssue) add(WorkFaceKind.Issue)
        if (hasRun) add(WorkFaceKind.Run)
        if (hasChanges) add(WorkFaceKind.Changes)
        if (hasResults) add(WorkFaceKind.Results)
    }

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
 * EXP-934: the top bar's `…` CONTEXT MENU (Share · Move to board · Unmark
 * duplicate · Delete issue) belongs to the ISSUE, so it shows on the Issue
 * face alone. On Run, Changes and Results the trailing slot carries the run's
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

sealed interface SwitcherTarget {
    data class Face(val face: WorkFaceKind) : SwitcherTarget
    data class Run(val id: String) : SwitcherTarget
    data object StartCoding : SwitcherTarget
}

/** What the switcher circle offers from the shown face: the OTHER faces in
 *  order, the Run face expanded into one row per own run when there are two
 *  or more (EXP-886), and `Start coding` first when the shown run ended and
 *  cannot be resumed (desktop shows Start in that state; on the phone the
 *  circle is the switcher, so the menu carries it). */
fun switcherTargets(
    faces: List<WorkFaceKind>,
    shown: WorkFaceKind,
    runIds: List<String>,
    shownRunId: String?,
    offerStart: Boolean,
): List<SwitcherTarget> = buildList {
    if (offerStart) add(SwitcherTarget.StartCoding)
    for (face in faces) {
        if (face == WorkFaceKind.Run && runIds.size >= 2) {
            for (id in runIds) {
                if (shown == WorkFaceKind.Run && id == shownRunId) continue
                add(SwitcherTarget.Run(id))
            }
            continue
        }
        if (face == shown) continue
        add(SwitcherTarget.Face(face))
    }
}

sealed interface SwitcherMode {
    data object Hidden : SwitcherMode
    data class Toggle(val target: SwitcherTarget) : SwitcherMode
    data class Menu(val targets: List<SwitcherTarget>) : SwitcherMode
}

/** No target = no circle; exactly one = a direct switch wearing the
 *  destination's icon; two or more = the faces glyph and a menu above. */
fun switcherMode(targets: List<SwitcherTarget>): SwitcherMode = when (targets.size) {
    0 -> SwitcherMode.Hidden
    1 -> SwitcherMode.Toggle(targets[0])
    else -> SwitcherMode.Menu(targets.toList())
}

/** EXP-862: the ONE session-dot palette, mirrored from web `session-dot.ts`
 *  and the desktop's `session_dot_tone` — running/review emerald, needs-input
 *  amber, done sky, ended/paused muted. */
enum class SessionDotTone { Running, Review, NeedsInput, Done, Muted }

sealed interface SwitcherBadge {
    data class Session(val tone: SessionDotTone) : SwitcherBadge
    data object Changes : SwitcherBadge
}

/** The circle's badge dot: off the Run face the shown session's state dot
 *  (none without a session); on the Run face a green dot while changes exist,
 *  so the reader knows a diff is waiting behind the switcher. */
fun switcherBadge(
    shown: WorkFaceKind,
    sessionTone: SessionDotTone?,
    hasChanges: Boolean,
): SwitcherBadge? {
    if (shown == WorkFaceKind.Run) return if (hasChanges) SwitcherBadge.Changes else null
    return sessionTone?.let { SwitcherBadge.Session(it) }
}

/** Where a face lands when it vanishes under the reader (the diff cleared,
 *  the run row went): changes → run → issue, and results → run → issue.
 *  `null` = nothing left. */
fun fallbackFace(shown: WorkFaceKind, available: List<WorkFaceKind>): WorkFaceKind? {
    if (shown in available) return shown
    val order = when (shown) {
        WorkFaceKind.Changes -> listOf(WorkFaceKind.Run, WorkFaceKind.Issue)
        WorkFaceKind.Results -> listOf(WorkFaceKind.Run, WorkFaceKind.Issue)
        WorkFaceKind.Run -> listOf(WorkFaceKind.Issue)
        WorkFaceKind.Issue -> emptyList()
    }
    return order.firstOrNull { it in available } ?: available.firstOrNull()
}

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
