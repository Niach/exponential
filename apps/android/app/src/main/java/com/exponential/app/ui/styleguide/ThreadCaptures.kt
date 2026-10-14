package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.thread.SessionCard

/** Which thread capture to draw (one shot each). */
enum class ThreadCaptureKind(val testId: String) {
    /** sg_session-row: THE session card, one per state. */
    SessionCards("session-cards-capture-screen"),
}

/**
 * EXP-1296: THE session card ([SessionCard]) in each of its states, built
 * through the real domain model ([SessionTree.sessionCard]) — the one thread
 * capture with no seed counterpart (web and desktop photograph a styleguide
 * island). The thread, the agent channel and the session view are walked
 * live against the seed (StyleguideScreenshotsTest).
 */
@Composable
fun ThreadCaptures(kind: ThreadCaptureKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            when (kind) {
                ThreadCaptureKind.SessionCards -> Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .statusBarsPadding()
                        .padding(horizontal = 16.dp, vertical = 72.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    CardStates.forEachIndexed { index, session ->
                        SessionCard(SessionTree.sessionCard(session, index + 1, CaptureNow), onClick = {})
                    }
                }
            }
        }
    }
}

// ── fake data (Acme) ─────────────────────────────────────────────────────────

private const val CaptureNow = "2026-10-13T20:04:00.000Z"
private const val CaptureTeam = "t-acme"
private const val Alex = "u-alex"
private const val CaptureCreatedAt = "2026-10-13T19:56:00.000Z"

private fun captureSession(
    id: String,
    status: String = DomainContract.sessionStatusRunning,
    busy: Boolean = false,
    needsInput: Boolean = false,
    blocked: SessionTree.Blocked? = null,
    title: String? = null,
    startedAt: String? = "2026-10-13T19:56:00.000Z",
    endedAt: String? = null,
) = SessionTree.Session(
    id = id,
    teamId = CaptureTeam,
    parentId = null,
    rootId = id,
    kind = DomainContract.sessionKindThread,
    userId = Alex,
    status = status,
    busy = busy,
    needsInput = needsInput,
    blocked = blocked,
    shared = true,
    title = title,
    guide = null,
    actionName = null,
    name = null,
    icon = null,
    color = null,
    sortOrder = 0.0,
    startedAt = startedAt,
    endedAt = endedAt,
    createdAt = CaptureCreatedAt,
    updatedAt = endedAt ?: CaptureCreatedAt,
)

/** One session per card state: queued, working, waiting, needs you, usage
 *  limit, ended. */
private val CardStates = listOf(
    captureSession("c-1", title = "Profile the launch path", status = DomainContract.sessionStatusQueued, startedAt = null),
    captureSession("c-2", title = "Defer the shape subscribe", busy = true, startedAt = "2026-10-13T19:51:26.000Z"),
    captureSession("c-3", title = "Board snapshot cache", startedAt = "2026-10-13T19:40:00.000Z"),
    captureSession("c-4", title = "Lazy-load the markdown editor", needsInput = true, startedAt = "2026-10-13T19:58:57.000Z"),
    captureSession(
        "c-5",
        title = "Migrate image cache",
        blocked = SessionTree.Blocked(window = "five_hour", resetsAt = "2026-10-13T22:00:00.000Z"),
        startedAt = "2026-10-13T18:59:00.000Z",
    ),
    captureSession(
        "c-6",
        status = DomainContract.sessionStatusEnded,
        startedAt = "2026-10-13T17:00:00.000Z",
        endedAt = "2026-10-13T17:37:00.000Z",
    ),
)
