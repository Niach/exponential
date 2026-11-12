package com.exponential.app.domain

import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.SessionIssueEntity
import com.exponential.app.data.db.SessionReadEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull

/**
 * EXP-1291: the session tree's view models — the Android mirror of
 * `packages/domain-contract/src/session-tree.ts` (ONE reference ×4: desktop
 * `domain::session_tree`, iOS ExpCore `SessionTree.swift`). Pure and
 * byte-locked by the fixtures `SessionTreeTest` replays:
 *   fixtures/session-card.json   → [sessionCard]
 *   fixtures/session-chat.json → [buildSessionChat], [guidePullRequest]
 *   fixtures/chat-list.json   → [partitionChats] + [listAgents] + [treeState]
 * The copy / icons / tones / testIds below are the fixtures' (the test
 * proves equality).
 */
object SessionTree {

    // ── inputs (the synced rows, projected) ─────────────────────────────────

    /** [kind] = contract `blockedKind`-ish: `rate_limit` (a usage wall) or a
     *  watchdog flag (`device_offline` / `unclaimed` / `stuck` / `restarted`);
     *  null = a wall. EXP-1374: [resume] = an interruption of a run that was
     *  mid-turn (its device continues it by itself when back). */
    data class Blocked(
        val window: String? = null,
        val resetsAt: String? = null,
        val kind: String? = null,
        val resume: Boolean? = null,
        /** EXP-1374: since when its device could continue it (null = never back). */
        val resumeSince: String? = null,
    )

    data class GuideEntry(val text: String? = null, val label: String? = null)

    data class Session(
        val id: String,
        val teamId: String,
        val parentId: String?,
        val rootId: String,
        val userId: String,
        val status: String,
        val busy: Boolean,
        val needsInput: Boolean,
        val blocked: Blocked?,
        val shared: Boolean,
        val title: String?,
        val guide: List<GuideEntry>?,
        val startedAt: String?,
        val endedAt: String?,
        val createdAt: String,
        val updatedAt: String,
        /** The steer device that runs it (`devices.device_id`); null = none. */
        val deviceId: String? = null,
        /** NEW-2: the agent whose conversation this root is (null = a chat
         *  or a child); the kind derives from it ([SessionKind.sessionKindOf]). */
        val agentId: String? = null,
        /** VAPP-110: the app this root is a member's editor session of (null = none). */
        val appId: String? = null,
        /** NEW-2: the agent whose conversation started this root (null = none);
         *  S-3: names the root's `origin` (Started by <that conversation>). */
        val startedByAgentId: String? = null,
        /** EXP-1374: busy only because background tasks still run after the
         *  turn (absent = false). */
        val background: Boolean = false,
    )

    /** EXP-1323: the viewer's read marks (`session_reads`, already user-scoped). */
    data class SessionRead(
        val sessionId: String,
        val readAt: String,
        /** #EXP-1388: the viewer archived the chat (null = not). */
        val archivedAt: String? = null,
    )

    /** NEW-1: [sessionId] = the session that HOLDS the issue (the row sits on it). */
    data class SessionIssue(
        val sessionId: String,
        val issueId: String,
        val sortOrder: Double,
    )

    /** [parentId] (EXP-1334) = the issue's `parent` relation (null = none):
     *  a linked sub-issue folds under its nearest LINKED ancestor. */
    data class Issue(val id: String, val identifier: String, val title: String, val parentId: String? = null)

    data class Message(
        val id: String,
        val sessionId: String,
        val authorKind: String,
        val authorId: String?,
        val body: String,
        /** The card's jsonb object; null = a plain text message. */
        val card: JsonObject?,
        val answeredAt: String?,
        val consumedAt: String?,
        val createdAt: String,
        /** EXP-1323: the session that posted it (absent = null). */
        val authorSessionId: String? = null,
        /** EXP-1459: sealed content this install cannot open (yet):
         *  `noKey` | `unreadable` ([SealedPlaceholder]); [body] is empty and
         *  [card] its clear part. Null = opened or plaintext. */
        val locked: String? = null,
        /** #EXP-1682: the files the message carries in the clear
         *  (`messages.attachment_ids`; null = none). */
        val attachmentIds: List<String>? = null,
    )

    /** #EXP-1682 (session-chat.json `$comment`, "ATTACHMENTS"): a message's
     *  row body = its body, then a blank line and one
     *  `![image](/api/attachments/<id>)` line per [attachmentIds] entry the
     *  body does not already name (the steer image block). A sealed or
     *  main-composer message keeps its files ONLY in `attachment_ids`. */
    fun messageRowBody(body: String, attachmentIds: List<String>?): String {
        val missing = attachmentIds.orEmpty().distinct()
            .filter { it.isNotEmpty() && !body.contains("/api/attachments/$it") }
        if (missing.isEmpty()) return body
        val embeds = missing.joinToString("\n") { "![image](/api/attachments/$it)" }
        val text = body.trim()
        return if (text.isNotEmpty()) "$text\n\n$embeds" else embeds
    }

    /** [state] (NEW-1) = contract `prState`; only [treeState] reads it (null = unknown, never merged). */
    data class PullRequest(val id: String, val number: Int, val state: String? = null)

    // ── copy (= the fixtures) ───────────────────────────────────────────────

    object CardCopy {
        /** S-3: the FOUR display states' words. */
        val STATES: Map<SessionState, String> = mapOf(
            SessionState.Working to "Working",
            SessionState.NeedsYou to "Needs you",
            SessionState.Idle to "Idle",
            SessionState.Done to "Done",
        )
        /** Icon CONCEPT names (icons.json), per state. */
        val ICONS: Map<SessionState, String> = mapOf(
            SessionState.Working to "coding-running",
            SessionState.NeedsYou to "coding-needs-input",
            SessionState.Idle to "coding-assistant",
            SessionState.Done to "coding-ended",
        )
        /** Tone tokens: muted | foreground | amber. */
        val TONES: Map<SessionState, String> = mapOf(
            SessionState.Working to "foreground",
            SessionState.NeedsYou to "amber",
            SessionState.Idle to "muted",
            SessionState.Done to "muted",
        )
        const val TEST_ID_CARD = "session-card"
        /** NEW-1 `copy.blockedKinds`: a `blocked.kind` → its word (S-3: a
         *  `needs_you` session's; an interruption is invisible). */
        val BLOCKED_KINDS: Map<String, String> = mapOf(
            "rate_limit" to "Usage limit",
            "unclaimed" to "Not picked up",
            "stuck" to "Stuck",
            // EXP-1536: a failed start reads as failed, never Idle.
            "start_failed" to "Couldn't start",
            // #EXP-1703: a device that stayed away.
            "device_offline" to "Device offline",
        )
        /** EXP-1374 `banner.background`. */
        const val BANNER_BACKGROUND = "The turn is over; background tasks still run on its device."
        /** #EXP-1405 `banner.queued`. */
        const val BANNER_QUEUED = "Waiting for its device to pick it up. It starts by itself once the device is online."
        /** S-3 `bannerIcons` / `bannerLabels`, by banner id. */
        val BANNER_ICONS: Map<String, String> =
            mapOf("background" to "trace-background", "queued" to "ui-clock", "offline" to "ui-device-offline")
        val BANNER_LABELS: Map<String, String> =
            mapOf("background" to "Background task", "queued" to "Queued", "offline" to "Device offline")
        /** #EXP-1703 `banner.offline` ({device} = the host's name, else [BANNER_DEVICE]). */
        const val BANNER_OFFLINE = "{device} is offline. This session continues when it's back."
        /** #EXP-1703 `bannerActions`. */
        val BANNER_ACTIONS: Map<String, String> = mapOf("offline" to "Move")
        /** #EXP-1703 `bannerDevice`. */
        const val BANNER_DEVICE = "Its device"
    }

    object ChatCopy {
        const val UNTITLED = "Untitled"
        const val PLACEHOLDER = "Type / for commands"
        const val CONVERSATION = "Conversation"
        const val GUIDE = "Guide"
        /** Event card templates by contract `messageEventKind`. */
        val EVENTS: Map<String, String> = mapOf(
            "issue_linked" to "Linked {issue}",
            "issue_filed" to "Filed {issue}",
            "issue_unlinked" to "Unlinked {issue}",
            "pr_opened" to "Opened PR {pr}",
            "pr_merged" to "Merged PR {pr}",
            "pr_closed" to "Closed PR {pr}",
            "session_ended" to "Session ended",
            "session_claimed" to "{session} started",
            "session_blocked" to "{session} hit a usage limit",
            "session_unclaimed" to "{session} was not picked up",
            "session_stuck" to "{session} shows no sign of life",
            // NEW-2: a trigger firing in an agent's conversation.
            "trigger_fired" to "Trigger fired: {name}",
            // EXP-1391: a refused merge the server posted into the root.
            "pr_merge_refused" to "Merge of PR {pr} refused",
            "pr_checks" to "CI on PR {pr}",
            // EXP-1709: the merge train's line; #EXP-1710 reads it by
            // `mergeOutcome` (MergeTrain.eventTemplate), this = no outcome.
            "pr_merge" to "Auto-merge of PR {pr}",
        )
        /** #EXP-1772 `events.trigger_schedule`: a schedule firing's marker
         *  (a `trigger_fired` without a name). */
        const val TRIGGER_SCHEDULE = "Schedule"
        const val TEST_ID_TABS = "chat-tabs"
        const val TEST_ID_CONVERSATION = "chat-conversation"
        const val TEST_ID_COMPOSER = "chat-composer"
        /** EXP-1306: the Guide's stack band title. */
        const val STACK = "Stack"
        const val TEST_ID_GUIDE_STACK = "chat-guide-stack"
        const val TEST_ID_GUIDE_MERGE = "chat-guide-merge"
        /** EXP-1307: the header `…` on the Guide (Close PR, EXP-1338). */
        const val TEST_ID_GUIDE_MENU = "chat-guide-menu"
        /** EXP-1323: a child's report row. */
        const val REPORT = "Report from {session}"
        /** S-3: the inter-session link's lead by [SessionXref.kind]. */
        val XREF: Map<String, String> = mapOf(
            "message" to "Message from",
            "report" to "Report from",
            "started" to "Started by",
            "reported" to "Reported to",
        )
        const val TEST_ID_XREF = "chat-xref"
    }

    /** S-3 inter-session links: a compact header naming ANOTHER session (the
     *  lead [ChatCopy.XREF], the agent's mark, the session's name, `›`); a
     *  tap opens that session on the back stack. [kind] = `message` |
     *  `report` | `started` | `reported`; [agentId] = the agent whose tree
     *  the session belongs to (its mark), else null. */
    data class SessionXref(
        val kind: String,
        val sessionId: String,
        val name: String,
        val agentId: String?,
    )

    /** EXP-1323 `copy.suggestions`. */
    object SuggestionsCopy {
        const val ONE = "1 suggested task"
        const val MANY = "{count} suggested tasks"
        const val STARTED = "{count} started"
        const val START_ALL = "Start all"
        const val START = "Start"
        const val HERE = "Do it here"
        const val OPTIONS = "Options"
        const val START_ON = "Start on…"
        const val START_WITH = "Start with…"
        const val FILE = "File as issue"
        const val DISMISS = "Dismiss"
        const val SENT_HERE = "Sent here"
        const val FILED = "Filed {issue}"
    }

    /** EXP-1211 `copy.replies` + `testIds.replies`: the quick-reply row. */
    object RepliesCopy {
        const val LABEL = "Quick replies"
        const val DISMISS = "Dismiss quick replies"
        const val TEST_ID = "chat-replies"
    }

    object ListCopy {
        const val TITLE = "Chats"
        const val AGENTS = "Agents"
        val GROUPS: Map<ChatsGroupKey, String> = mapOf(
            ChatsGroupKey.NeedsYou to "Needs you",
            ChatsGroupKey.Running to "Working",
            ChatsGroupKey.Recent to "Recent",
        )
        const val EMPTY = "No sessions yet"
        const val AGENT_DEFAULT_ICON = "bot"
        const val AGENT_DEFAULT_COLOR = "#6366f1"
        const val TEST_ID_LIST = "chats-list"
        const val TEST_ID_ROW = "chats-row"
        const val TEST_ID_AGENT_ROW = "agents-row"
        /** EXP-1318: the rail band (web + IDE). */
        const val RAIL = "Active"
        /** NEW-1 fixture `recent` (hours, max). */
        const val RECENT_HOURS = 24
        const val RECENT_MAX = 20
        /** S-3 fixture `recentChildMinutes`: [visibleChild]'s window. */
        const val RECENT_CHILD_MINUTES = 120
    }

    // ── session state + card ────────────────────────────────────────────────

    /** S-3 (owner 2026-10-28): the FOUR display states ×4, nothing else (no
     *  running, paused, interrupted or stale in the UI). [Idle] doubles as a
     *  chat row's `waiting` mark ([chatRowMarkState], wire `waiting`). */
    enum class SessionState(val wire: String) {
        Working("working"),
        NeedsYou("needs_you"),
        Idle("idle"),
        Done("done"),
    }

    private fun ms(iso: String?): Long? =
        if (iso.isNullOrEmpty()) null else WireTimestamps.parseEpochMs(iso)

    /** The work card's elapsed ladder: `37s`, `2m 04s`, `1h 03m`. */
    fun formatElapsed(durationMs: Long): String {
        val total = maxOf(0L, Math.floorDiv(durationMs, 1000L))
        val hours = total / 3600
        val minutes = (total % 3600) / 60
        val seconds = total % 60
        fun pad(value: Long) = value.toString().padStart(2, '0')
        return when {
            hours > 0 -> "${hours}h ${pad(minutes)}m"
            minutes > 0 -> "${minutes}m ${pad(seconds)}s"
            else -> "${seconds}s"
        }
    }

    /** S-3: one state per session (`working` = queued or busy, background
     *  tasks included; `needs_you` = needsInput; `done` = ended; else `idle`,
     *  an interruption included), the card's kind word and tone. */
    fun sessionState(session: Session): SessionState = when {
        session.status == DomainContract.sessionStatusEnded -> SessionState.Done
        session.needsInput -> SessionState.NeedsYou
        session.status == DomainContract.sessionStatusQueued || session.busy -> SessionState.Working
        else -> SessionState.Idle
    }

    /** S-3: a NESTED session (a child row, a strip chip) shows while it works
     *  or needs you, or while its last activity (ended: endedAt, else
     *  updatedAt) is within [ListCopy.RECENT_CHILD_MINUTES] of [now] (null =
     *  never). */
    fun visibleChild(session: Session, now: String?): Boolean {
        val state = sessionState(session)
        if (state == SessionState.Working || state == SessionState.NeedsYou) return true
        val at = ms(now)
        val last = if (state == SessionState.Done) ms(session.endedAt) ?: ms(session.updatedAt) else ms(session.updatedAt)
        return at != null && last != null && at - last <= ListCopy.RECENT_CHILD_MINUTES * 60L * 1000L
    }

    /** S-3: the ids of a tree's sessions that show nested: [visibleChild] or
     *  an ancestor of one (a working grandchild keeps its idle parent). */
    fun visibleTree(tree: List<Session>, now: String?): Set<String> {
        val byId = tree.associateBy { it.id }
        val out = HashSet<String>()
        for (session in tree) {
            if (!visibleChild(session, now)) continue
            var at: Session? = session
            val seen = HashSet<String>()
            while (at != null && seen.add(at.id)) {
                out.add(at.id)
                at = at.parentId?.let { byId[it] }
            }
        }
        return out
    }

    /** EXP-1312: a chat row's lead outside the Chats tab (the home's
     *  Running band) ×4: needs you beats working, else the bare mark
     *  (`chat-list.json` `markCases`; its `waiting` = [SessionState.Idle]). */
    fun chatRowMarkState(needsYou: Boolean, busy: Boolean): SessionState = when {
        needsYou -> SessionState.NeedsYou
        busy -> SessionState.Working
        else -> SessionState.Idle
    }

    /** The `markCases` wire of a [chatRowMarkState] (`waiting` for idle). */
    fun chatRowMarkWire(state: SessionState): String =
        if (state == SessionState.Idle) "waiting" else state.wire

    /** Sessions of one tree in creation order (ties by id). */
    fun treeOrder(sessions: List<Session>): List<Session> =
        sessions.sortedByEpoch(tie = compareBy { it.id }) { ms(it.createdAt) ?: 0L }

    data class SessionCard(
        val sessionId: String,
        /** Icon CONCEPT name (`ui-clock`, `coding-running`, …). */
        val icon: String,
        val name: String,
        val kindWord: String,
        val state: SessionState,
        val tone: String,
        val elapsed: String?,
        /** A spinner while true. */
        val running: Boolean,
        val testId: String,
    )

    /**
     * THE session card = the Work card's header row: icon, name, the muted
     * state word, trailing elapsed, a spinner while [SessionCard.running].
     * Tapping it opens that session's raw view. [title] = its [sessionTitle]
     * (EXP-1341, never `Session N`); [now] = an ISO timestamp.
     */
    /**
     * NEW-1 (TS `sessionKindWord`): the card's state word; a session that
     * needs you because of a flag names WHY (`copy.blockedKinds[blocked.kind]`),
     * an unknown kind keeps `states.needs_you`.
     */
    fun sessionKindWord(session: Session, state: SessionState): String {
        if (state == SessionState.NeedsYou) {
            // #EXP-1703: `device_offline` names it only while the device is away.
            if (session.blocked?.kind == "device_offline" && !session.blocked?.resumeSince.isNullOrEmpty()) {
                return CardCopy.STATES.getValue(state)
            }
            session.blocked?.kind?.let { kind -> CardCopy.BLOCKED_KINDS[kind] }?.let { return it }
        }
        return CardCopy.STATES.getValue(state)
    }

    /** S-3: elapsed only while working (not queued; startedAt → now) and once
     *  done (startedAt → endedAt, else updatedAt); idle / needs you = none. */
    fun sessionCard(session: Session, title: String, now: String): SessionCard {
        val state = sessionState(session)
        val start = ms(session.startedAt)
        val end = when {
            state == SessionState.Done -> ms(session.endedAt) ?: ms(session.updatedAt)
            state == SessionState.Working && session.status != DomainContract.sessionStatusQueued -> ms(now)
            else -> null
        }
        val elapsed = if (start != null && end != null) formatElapsed(end - start) else null
        return SessionCard(
            sessionId = session.id,
            icon = CardCopy.ICONS.getValue(state),
            name = title,
            kindWord = sessionKindWord(session, state),
            state = state,
            tone = CardCopy.TONES.getValue(state),
            elapsed = elapsed,
            running = state == SessionState.Working,
            testId = CardCopy.TEST_ID_CARD,
        )
    }

    /** EXP-1374: the Dock card of a background session and (#EXP-1405) of a
     *  queued one waiting for its device (a dock.json `banner` source without
     *  its `kind`); [tone] is always `muted`, [action] always null. S-3: an
     *  interruption has no card. */
    data class SessionBanner(
        /** `background` | `queued` | `offline`. */
        val id: String,
        /** Concept icon. */
        val icon: String,
        val label: String,
        val text: String,
        val tone: String,
        val action: String?,
    ) {
        /** The Dock source it draws as. */
        val source: Dock.Source.Banner
            get() = Dock.Source.Banner(id, icon, label, text, if (tone == "warning") Dock.Tint.Warning else Dock.Tint.Muted, action)
    }

    /** #EXP-1703 (TS `sessionOffline`): on a person because its device stayed
     *  away: it needs you, its flag is `device_offline`, its device never came back. */
    fun sessionOffline(session: Session): Boolean =
        session.status != DomainContract.sessionStatusEnded &&
            session.needsInput &&
            session.blocked?.kind == "device_offline" &&
            session.blocked?.resumeSince.isNullOrEmpty()

    /** EXP-1374 (TS `sessionBanner`): null unless background or (#EXP-1405)
     *  queued, waiting for its device; never once ended or needing you. */
    fun sessionBanner(session: Session, deviceName: String? = null): SessionBanner? {
        // #EXP-1703: a device that stayed away names itself, with Move.
        if (sessionOffline(session)) {
            return SessionBanner(
                id = "offline",
                icon = CardCopy.BANNER_ICONS.getValue("offline"),
                label = CardCopy.BANNER_LABELS.getValue("offline"),
                text = CardCopy.BANNER_OFFLINE.replace("{device}", deviceName?.takeIf { it.isNotEmpty() } ?: CardCopy.BANNER_DEVICE),
                tone = "warning",
                action = CardCopy.BANNER_ACTIONS.getValue("offline"),
            )
        }
        if (session.status == DomainContract.sessionStatusEnded || session.needsInput) return null
        val (id, text) = when {
            session.status == DomainContract.sessionStatusQueued -> "queued" to CardCopy.BANNER_QUEUED
            session.busy && session.background -> "background" to CardCopy.BANNER_BACKGROUND
            else -> return null
        }
        return SessionBanner(
            id = id,
            icon = CardCopy.BANNER_ICONS.getValue(id),
            label = CardCopy.BANNER_LABELS.getValue(id),
            text = text,
            tone = "muted",
            action = null,
        )
    }

    // ── EXP-1341: titles via [SessionTitle] ─────────────────────────────────

    private fun titleSession(s: Session) =
        SessionTitle.TitleSession(id = s.id, rootId = s.rootId, title = s.title)

    private fun titleLink(link: SessionIssue) =
        SessionTitle.TitleLink(link.sessionId, link.issueId, link.sortOrder)

    private fun titleIssue(issue: Issue) = SessionTitle.TitleIssue(issue.id, issue.identifier, issue.title)

    private fun titleMessage(m: Message) = SessionTitle.TitleMessage(
        id = m.id,
        sessionId = m.sessionId,
        authorKind = m.authorKind,
        authorSessionId = m.authorSessionId,
        body = m.body,
        hasCard = m.card != null,
        createdAt = m.createdAt,
    )

    /** THE display title of [session] (fixture `session-title.json`). */
    fun sessionTitle(
        session: Session,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
    ): String = SessionTitle.sessionTitle(
        titleSession(session),
        sessionIssues.map(::titleLink),
        issues.map(::titleIssue),
        messages.map(::titleMessage),
    )

    /** Every given session's title, keyed by id. */
    fun sessionTitles(
        sessions: List<Session>,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
    ): Map<String, String> = SessionTitle.sessionTitles(
        sessions.map(::titleSession),
        sessionIssues.map(::titleLink),
        issues.map(::titleIssue),
        messages.map(::titleMessage),
    )

    /** A session chip's lead ×4: THAT session's display state (S-3). */
    fun sessionTabMarkState(session: Session): SessionState = sessionState(session)

    /** NEW-1: a chat's issues = the UNION over its tree's rows: the
     *  sessions in tree order (the root first, then creation order), each
     *  one's rows by sortOrder, every issue once. */
    fun treeIssues(tree: List<Session>, sessionIssues: List<SessionIssue>, issues: List<Issue>): List<Issue> {
        val byId = issues.associateBy { it.id }
        val seen = HashSet<String>()
        val out = mutableListOf<Issue>()
        for (session in tree) {
            val rows = sessionIssues
                .filter { it.sessionId == session.id }
                .sortedWith(compareBy<SessionIssue>({ it.sortOrder }, { it.issueId }))
            for (link in rows) {
                val issue = byId[link.issueId] ?: continue
                if (!seen.add(issue.id)) continue
                out.add(issue)
            }
        }
        return out
    }

    // ── the chat screen ───────────────────────────────────────────────────

    /** EXP-1334: one row of a folded chip's subtree; [depth] relative to the chip. */
    data class NestedIssue(val issueId: String, val identifier: String, val depth: Int)

    data class NestedSession(val sessionId: String, val name: String, val markState: SessionState, val depth: Int)

    sealed interface ChatTab {
        data object Add : ChatTab
        data object Conversation : ChatTab
        /** EXP-1334: a TOP-LEVEL linked issue; [nested] = the linked sub-issues folded under it. */
        data class IssueTab(
            val issueId: String,
            val identifier: String,
            val nested: List<NestedIssue> = emptyList(),
        ) : ChatTab
        /** EXP-1334: a DIRECT child of the dispatcher; [markState] = its lead,
         *  [nested] = every deeper session folded under it. */
        data class SessionTab(
            val sessionId: String,
            val name: String,
            val markState: SessionState,
            val nested: List<NestedSession> = emptyList(),
        ) : ChatTab
        /** results-tab.json: the strip's last tab, pinned right (not part of
         *  session-chat.json `tabs`: [chatView] never emits it, the host
         *  appends it after the last chip). */
        data object Results : ChatTab
        /** #EXP-1662 (plan-page.json): the plan tab right after Conversation
         *  while the chat holds a plan ([chatView] never emits it, the
         *  host inserts it). */
        data object Plan : ChatTab
    }

    sealed interface SessionChatRow {
        val id: String

        data class MessageRow(
            override val id: String,
            /** `user` | `agent` | `system`. */
            val author: String,
            val authorId: String?,
            val body: String,
            /** A person's message the device has not consumed yet. */
            val queued: Boolean,
            /** EXP-1323: the session that posted it (a child asking its parent). */
            val authorSessionId: String? = null,
            /** S-3: posted by ANOTHER (synced) session = its `message` link. */
            val xref: SessionXref? = null,
        ) : SessionChatRow

        data class SessionRow(override val id: String, val card: SessionCard) : SessionChatRow

        /** EXP-1323: a child's end on its parent: `Report from <name>` + the summary. */
        data class Report(
            override val id: String,
            val sessionId: String,
            val text: String,
            val summary: String,
            /** S-3: renders AS the `Report from <chip> ›` header line. */
            val xref: SessionXref,
        ) : SessionChatRow

        /** EXP-1323: ONE suggestions card; [sessionId] = the posting session. */
        data class Suggestions(
            override val id: String,
            val sessionId: String,
            val header: String,
            val startAll: Boolean,
            val items: List<SuggestionRow>,
        ) : SessionChatRow

        data class Question(
            override val id: String,
            /** EXP-1334: the session whose card it is (the root, or a child). */
            val sessionId: String,
            val question: String,
            val options: List<QuestionChoice>,
            /** A free-text "Other" answer is offered (default true; EXP-1280 `custom`). */
            val custom: Boolean,
            val answer: String?,
            val answered: Boolean,
            /** EXP-1280: several picks allowed. */
            val multi: Boolean = false,
            /** EXP-1280: the inline Exponential UI surface, if any. */
            val surface: CardSurface? = null,
            /** EXP-1280: the picked labels (null on a legacy card = [answer] alone). */
            val answers: List<String>? = null,
            /** EXP-1280: the surface's data model as the person answered it. */
            val surfaceData: JsonObject? = null,
            /** #EXP-1561 (waiting-cards.json): a person dismissed it; true or null (absent). */
            val dismissed: Boolean? = null,
        ) : SessionChatRow

        /** One choice of a question card: the answer is the [label] (EXP-1322);
         *  [attachmentId] = its picture (EXP-1280 plan-card.json `pictureFor`). */
        data class QuestionChoice(val label: String, val description: String?, val attachmentId: String? = null)

        /** EXP-1280: one choice question of a plan card. */
        data class PlanQuestion(
            val id: String,
            val prompt: String,
            val multi: Boolean,
            val custom: Boolean,
            val options: List<QuestionChoice>,
        )

        /** [fresh] = an approval that started in a fresh context (ask-dock.json). */
        data class Plan(
            override val id: String,
            val sessionId: String,
            val plan: String,
            val decision: String?,
            val fresh: Boolean = false,
            /** EXP-1280: the choice questions, the inline surface. */
            val questions: List<PlanQuestion> = emptyList(),
            val surface: CardSurface? = null,
            /** EXP-1280: the picked labels per question id, the note. */
            val answers: Map<String, List<String>>? = null,
            val feedback: String? = null,
            /** EXP-1280: the surface's data model as the person answered it. */
            val surfaceData: JsonObject? = null,
        ) : SessionChatRow

        data class Subagent(override val id: String, val title: String, val status: String) : SessionChatRow

        data class Picture(
            override val id: String,
            val attachmentId: String,
            val width: Int?,
            val height: Int?,
            val caption: String?,
        ) : SessionChatRow

        /** #EXP-1464: [sessionId] = the synced session the line is about (its
         *  card's `sessionId`), else null; chat-row-action.json opens it. */
        data class Event(
            override val id: String,
            val event: String,
            val text: String,
            val sessionId: String? = null,
        ) : SessionChatRow

        /** Session 58cb32df (chats.json): a person's `/new` or `/compact` line,
         *  painted as the centred divider pill; [kind] = `new` | `compact`. */
        data class Divider(override val id: String, val kind: String, val at: String) : SessionChatRow

        /** EXP-1459 (sealed-content.json `placeholder`): content this install
         *  cannot open (yet), whatever its kind: THE transcript pill with the
         *  lock; [reason] = `noKey` | `unreadable`, [author] user | agent | system. */
        data class Locked(override val id: String, val reason: String, val author: String) : SessionChatRow

        /** Session 58cb32df (chats.json): an agent's inline Exponential UI preview. */
        data class Surface(
            override val id: String,
            val sessionId: String,
            val title: String?,
            val surface: CardSurface,
            /** A result's screenshots (plan-card pictures' shape). */
            val pictures: List<PlanCard.Picture> = emptyList(),
        ) : SessionChatRow
    }

    /** EXP-1323: one suggested task's state (dismissed items drop). */
    enum class SuggestionState(val wire: String) { Open("open"), Started("started"), Here("here"), Filed("filed") }

    /** EXP-1323: one suggested task: `open` (× · Options · Do it here · Start),
     *  `started` (the child's card once synced), `here` (sent to the posting
     *  session), `filed` (the issue it became). */
    data class SuggestionRow(
        val id: String,
        val title: String,
        val text: String,
        val state: SuggestionState,
        val card: SessionCard?,
        val issueId: String?,
        val identifier: String?,
    )

    data class Composer(val sessionId: String, val placeholder: String)

    data class ChatView(
        val rootId: String,
        val title: String,
        val private: Boolean,
        /** The session open in place (`focus`), null on the chat itself. */
        val focusId: String?,
        val tabs: List<ChatTab>,
        val rows: List<SessionChatRow>,
        val composer: Composer,
        val live: Boolean,
        val busy: Boolean,
        val needsYou: Boolean,
        /** EXP-1323: the live tail (Tasks card + status line) reads this
         *  session's relay state; null once the root ended. */
        val liveSessionId: String? = null,
        /** NEW-6 (N-1): a chat with ONE session (the root alone) shows that
         *  session's raw work as the main thing: its id, and no Conversation
         *  tab. Null once a second session exists. */
        val mainSessionId: String? = null,
        /** S-3: who started the viewed session (the focus, else the root):
         *  its parent, else the agent conversation that started the root;
         *  `reported` once the viewed session ended. Null = a person. */
        val origin: SessionXref? = null,
    )

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    /** JS `String(x ?? "")` for a card field. */
    private fun JsonObject.text(key: String): String = when (val value = this[key]) {
        null, is JsonNull -> ""
        is JsonPrimitive -> value.content
        else -> value.toString()
    }

    private fun JsonObject.isTrue(key: String): Boolean =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true"

    private fun JsonObject.isFalse(key: String): Boolean =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "false"

    // ── EXP-1280: the rich plan / question card (plan-card.json) ────────────

    /** A `question` / `plan` message's row (null = another kind). */
    fun askRow(message: Message): SessionChatRow? {
        val card = message.card ?: return null
        return when (card.str("kind")) {
            DomainContract.messageCardKindQuestion -> SessionChatRow.Question(
                id = message.id,
                sessionId = message.sessionId,
                question = card.text("question"),
                options = questionOptions(card),
                custom = !card.isFalse("custom"),
                answer = card.str("answer"),
                answered = message.answeredAt != null,
                multi = card.isTrue("multi"),
                surface = cardSurface(card["surface"]),
                answers = (card["answers"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content },
                surfaceData = card["surfaceData"] as? JsonObject,
                dismissed = if (card.isTrue("dismissed")) true else null,
            )
            DomainContract.messageCardKindPlan -> SessionChatRow.Plan(
                id = message.id,
                sessionId = message.sessionId,
                plan = card.text("plan"),
                decision = card.str("decision"),
                fresh = card.isTrue("fresh"),
                questions = planQuestions(card),
                surface = cardSurface(card["surface"]),
                answers = planAnswers(card["answers"]),
                feedback = card.str("feedback")?.takeIf { it.isNotEmpty() },
                surfaceData = card["surfaceData"] as? JsonObject,
            )
            else -> null
        }
    }

    /** A `surface` message's row (null = another kind or no surface). */
    fun surfaceRow(message: Message): SessionChatRow.Surface? {
        val card = message.card ?: return null
        if (card.str("kind") != DomainContract.messageCardKindSurface) return null
        val surface = cardSurface(card["surface"]) ?: return null
        return SessionChatRow.Surface(
            id = message.id,
            sessionId = message.sessionId,
            title = card.str("title")?.trim()?.takeIf { it.isNotEmpty() },
            surface = surface,
            pictures = PlanCard.cardPictures(card["pictures"]),
        )
    }

    /** S-3: `typeof card[key] === 'boolean'` (null = absent or another type). */
    private fun JsonObject.bool(key: String): Boolean? =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content?.let {
            when (it) {
                "true" -> true
                "false" -> false
                else -> null
            }
        }

    /** EXP-1398: the session view's cards ([AskDock.sessionCards]) with the
     *  rows they draw as, oldest first. */
    fun sessionCardRows(messages: List<Message>): List<Pair<AskDock.SessionCard, SessionChatRow>> {
        val byId = messages.associateBy { it.id }
        return AskDock.sessionCards(messages).mapNotNull { card ->
            val message = byId[card.id] ?: return@mapNotNull null
            val row = if (card.kind == AskDock.SessionCardKind.Surface) surfaceRow(message) else askRow(message)
            row?.let { card to it }
        }
    }

    /** The question card's options: `options` = plain labels, descriptions ride
     *  the parallel `optionDescriptions` sidecar (null / "" = none, EXP-1322),
     *  pictures the `optionPictures` sidecar of picture LABELS resolved through
     *  `pictures`; zipped by ORIGINAL index before non-string options drop. */
    fun questionOptions(card: JsonObject): List<SessionChatRow.QuestionChoice> {
        val options = card["options"] as? JsonArray ?: return emptyList()
        val descriptions = card["optionDescriptions"] as? JsonArray
        val pictureLabels = card["optionPictures"] as? JsonArray
        val pictures = PlanCard.cardPictures(card["pictures"])
        return options.mapIndexedNotNull { index, option ->
            val label = (option as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return@mapIndexedNotNull null
            val description = (descriptions?.getOrNull(index) as? JsonPrimitive)
                ?.takeIf { it.isString && it.content.isNotEmpty() }?.content
            val picture = (pictureLabels?.getOrNull(index) as? JsonPrimitive)?.takeIf { it.isString }?.content
            SessionChatRow.QuestionChoice(label, description, PlanCard.pictureFor(pictures, picture))
        }
    }

    /** A plan card's `questions`; a malformed question (no string id/prompt)
     *  or option (no string label) drops. */
    fun planQuestions(card: JsonObject): List<SessionChatRow.PlanQuestion> {
        val questions = card["questions"] as? JsonArray ?: return emptyList()
        val pictures = PlanCard.cardPictures(card["pictures"])
        return questions.mapNotNull { raw ->
            val question = raw as? JsonObject ?: return@mapNotNull null
            val id = question.str("id") ?: return@mapNotNull null
            val prompt = question.str("prompt") ?: return@mapNotNull null
            val options = (question["options"] as? JsonArray).orEmpty().mapNotNull { entry ->
                val option = entry as? JsonObject ?: return@mapNotNull null
                val label = option.str("label") ?: return@mapNotNull null
                SessionChatRow.QuestionChoice(
                    label = label,
                    description = option.str("description")?.takeIf { it.isNotEmpty() },
                    attachmentId = PlanCard.pictureFor(pictures, option.str("picture")),
                )
            }
            SessionChatRow.PlanQuestion(id, prompt, question.isTrue("multi"), !question.isFalse("custom"), options)
        }
    }

    /** A card's inline surface: an object with a `root` object, else null. */
    fun cardSurface(raw: kotlinx.serialization.json.JsonElement?): CardSurface? {
        val surface = raw as? JsonObject ?: return null
        val root = surface["root"] as? JsonObject ?: return null
        val version = (surface["version"] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
        return CardSurface(root, surface["data"] as? JsonObject, version)
    }

    /** A plan card's `answers` (question id → labels), strings only. */
    fun planAnswers(raw: kotlinx.serialization.json.JsonElement?): Map<String, List<String>>? {
        val answers = raw as? JsonObject ?: return null
        return answers.mapNotNull { (id, labels) ->
            val list = labels as? JsonArray ?: return@mapNotNull null
            id to list.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
        }.toMap()
    }

    private fun JsonObject.int(key: String): Int? {
        val value = this[key] as? JsonPrimitive ?: return null
        if (value.isString) return null
        return value.doubleOrNull?.toInt()
    }

    private fun eventText(
        card: JsonObject,
        issues: Map<String, Issue>,
        prs: Map<String, PullRequest>,
        sessionName: (String) -> String? = { null },
    ): String? {
        val event = card.text("event")
        // #EXP-1710: the merge train's `pr_merge` line reads by its
        // `mergeOutcome` (merge-train.json `copy.events`); a card without
        // one keeps the generic copy.
        var text = (if (event == DomainContract.messageEventKindPrMerge) MergeTrain.eventTemplate(card.str("mergeOutcome")) else null)
            ?: ChatCopy.EVENTS[event] ?: return null
        val issue = card.str("issueId")?.let { issues[it] }
        val pr = card.str("pullRequestId")?.let { prs[it] }
        if (text.contains("{issue}")) {
            if (issue == null) return null
            text = text.replaceFirst("{issue}", issue.identifier)
        }
        if (text.contains("{pr}")) {
            if (pr == null) return null
            text = text.replaceFirst("{pr}", "#${pr.number}")
        }
        if (text.contains("{session}")) {
            // NEW-1: a bus line about a session of the tree; an unsynced one drops.
            val name = card.str("sessionId")?.let(sessionName) ?: return null
            text = text.replaceFirst("{session}", name)
        }
        if (text.contains("{name}")) {
            // NEW-2 `trigger_fired`: the event name; a schedule firing has
            // none and draws no line (its body says what fired).
            val name = card.str("name")?.takeIf { it.isNotBlank() } ?: return null
            text = text.replaceFirst("{name}", name)
        }
        return text
    }

    private fun suggestionsRow(
        message: Message,
        card: JsonObject,
        byId: Map<String, Session>,
        titles: Map<String, String>,
        issues: Map<String, Issue>,
        now: String,
    ): SessionChatRow? {
        val raws = card["items"] as? JsonArray ?: return null
        val items = mutableListOf<SuggestionRow>()
        for (element in raws) {
            val raw = element as? JsonObject ?: continue
            val id = raw.str("id") ?: continue
            if ((raw["dismissed"] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true") continue
            val sessionId = raw.str("sessionId")
            val issueId = raw.str("issueId")
            val target = sessionId?.let { byId[it] }
            val here = (raw["here"] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true"
            val state = when {
                sessionId != null -> SuggestionState.Started
                here -> SuggestionState.Here
                issueId != null -> SuggestionState.Filed
                else -> SuggestionState.Open
            }
            items.add(
                SuggestionRow(
                    id = id,
                    title = raw.text("title"),
                    text = raw.text("text"),
                    state = state,
                    card = target?.let { sessionCard(it, titles[it.id] ?: "", now) },
                    issueId = issueId,
                    identifier = issueId?.let { issues[it]?.identifier },
                ),
            )
        }
        if (items.isEmpty()) return null
        val started = items.count { it.state == SuggestionState.Started }
        val count = if (items.size == 1) {
            SuggestionsCopy.ONE
        } else {
            SuggestionsCopy.MANY.replace("{count}", items.size.toString())
        }
        return SessionChatRow.Suggestions(
            id = message.id,
            sessionId = message.sessionId,
            header = if (started > 0) "$count · ${SuggestionsCopy.STARTED.replace("{count}", started.toString())}" else count,
            startAll = items.count { it.state == SuggestionState.Open } > 1,
            items = items,
        )
    }

    // `compareBy({ ms(createdAt) ?: 0 }, { id })`, the time parsed once per row ([sortedByEpoch]).
    private val messageTie = compareBy<Message> { it.id }

    /** Depth-first walk of a forest given each node's FOLD parent (null = a
     *  top-level node), children in the given order. */
    private fun <T> foldForest(
        items: List<T>,
        id: (T) -> String,
        foldParent: (T) -> String?,
    ): List<Pair<T, List<Pair<T, Int>>>> {
        val children = LinkedHashMap<String?, MutableList<T>>()
        for (item in items) children.getOrPut(foldParent(item)) { mutableListOf() }.add(item)
        val seen = mutableSetOf<String>()
        fun fold(top: T): Pair<T, List<Pair<T, Int>>> {
            val nested = mutableListOf<Pair<T, Int>>()
            seen.add(id(top))
            fun walk(parentId: String, depth: Int) {
                for (child in children[parentId].orEmpty()) {
                    if (!seen.add(id(child))) continue
                    nested.add(child to depth)
                    walk(id(child), depth + 1)
                }
            }
            walk(id(top), 1)
            return top to nested.toList()
        }
        val out = children[null].orEmpty().map(::fold).toMutableList()
        // A fold-parent cycle (a ↔ b) reaches no top: its first unreached
        // item (input order) becomes a top-level chip, the rest fold under it.
        for (item in items) if (id(item) !in seen) out.add(fold(item))
        return out
    }

    /** EXP-1334: a linked issue's fold parent = its nearest LINKED ancestor
     *  (an unlinked parent is skipped; a cycle or none = top-level). */
    private fun issueFoldParent(issue: Issue, linked: Set<String>, all: Map<String, Issue>): String? {
        val seen = mutableSetOf(issue.id)
        var parent = issue.parentId
        while (parent != null && parent !in seen) {
            if (parent in linked) return parent
            seen.add(parent)
            parent = all[parent]?.parentId
        }
        return null
    }

    /**
     * The chat screen (EXP-1334: the conversation IS the dispatcher): the
     * header tabs (+ · Conversation · top-level issue chips (sub-issues
     * folded) · the dispatcher's direct children (deeper sessions folded) ·
     * Guide), the root's conversation as rows (its messages oldest first plus
     * the children's own question / plan cards; a `session` /
     * `thread_started` card becomes that session's card) and the composer
     * target (the root: a message to an ended chat continues it).
     */
    fun buildSessionChat(
        root: Session,
        sessions: List<Session>,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
        pullRequests: List<PullRequest>,
        now: String,
        focus: String? = null,
    ): ChatView {
        val tree = treeOrder(sessions.filter { it.rootId == root.id || it.id == root.id }).toMutableList()
        if (tree.none { it.id == root.id }) tree.add(0, root)
        val byId = sessions.associateBy { it.id }.toMutableMap()
        byId[root.id] = root
        val titles = sessionTitles(byId.values.toList(), sessionIssues, issues, messages)
        val linked = treeIssues(tree, sessionIssues, issues)
        val issueMap = issues.associateBy { it.id }
        val prMap = pullRequests.associateBy { it.id }
        // S-3: a link to another session = its name + its agent's mark; an
        // unsynced session (the viewer may not see it) never links.
        fun agentOf(s: Session): String? {
            val top = byId[s.rootId] ?: s
            return s.agentId ?: top.agentId ?: top.startedByAgentId
        }
        fun xref(kind: String, id: String?): SessionXref? {
            val target = id?.let { byId[it] } ?: return null
            return SessionXref(kind, target.id, sessionCard(target, titles[target.id] ?: "", now).name, agentOf(target))
        }
        val own = messages.filter { it.sessionId == root.id }.sortedByEpoch(tie = messageTie) { ms(it.createdAt) ?: 0L }
        val title = titles.getValue(root.id)
        val focused = focus?.let { id -> tree.firstOrNull { it.id == id } }
        // EXP-1334: the conversation IS the dispatcher (no chip of its own);
        // the chips = its DIRECT children, every deeper session folded under
        // its top-level ancestor (the chip's dropdown).
        fun nameOf(s: Session) = sessionCard(s, titles[s.id] ?: "", now).name
        val treeIds = tree.map { it.id }.toSet()
        // S-3: only the VISIBLE sessions get chips ([visibleTree]), the
        // focused one always; the rest stay reachable through their cards.
        val shown = visibleTree(tree.filter { it.id != root.id }, now)
        val sessionTabs = foldForest(
            tree.filter { it.id != root.id && (it.id in shown || it.id == focused?.id) },
            { it.id },
            { s -> if (s.parentId == root.id || s.parentId == null || s.parentId !in treeIds) null else s.parentId },
        ).map { (top, nested) ->
            ChatTab.SessionTab(
                sessionId = top.id,
                name = nameOf(top),
                markState = sessionTabMarkState(top),
                nested = nested.map { (item, depth) ->
                    NestedSession(item.id, nameOf(item), sessionTabMarkState(item), depth)
                },
            )
        }
        val linkedIds = linked.map { it.id }.toSet()
        val issueTabs = foldForest(linked, { it.id }, { issueFoldParent(it, linkedIds, issueMap) }).map { (top, nested) ->
            ChatTab.IssueTab(
                issueId = top.id,
                identifier = top.identifier,
                nested = nested.map { (item, depth) -> NestedIssue(item.id, item.identifier, depth) },
            )
        }

        // EXP-1334: no root card; a CHILD's own question / plan card surfaces
        // in the Conversation (answered here, straight back to that child).
        val asks = messages.filter { m ->
            m.sessionId != root.id && m.sessionId in treeIds &&
                m.card?.str("kind").let {
                    it == DomainContract.messageCardKindQuestion || it == DomainContract.messageCardKindPlan
                }
        }
        val conversation = (own + asks).sortedByEpoch(tie = messageTie) { ms(it.createdAt) ?: 0L }
        val rows = mutableListOf<SessionChatRow>()
        val messagesById = messages.associateBy { it.id }
        val escalated = mutableSetOf<String>()
        val fires = mutableSetOf<String>()
        for (message in conversation) {
            val card = message.card
            message.locked?.let { reason ->
                val author = when (message.authorKind) {
                    DomainContract.messageAuthorKindUser, DomainContract.messageAuthorKindAgent -> message.authorKind
                    else -> DomainContract.messageAuthorKindSystem
                }
                rows.add(SessionChatRow.Locked(message.id, reason, author))
                continue
            }
            if (card == null) {
                val author = when (message.authorKind) {
                    DomainContract.messageAuthorKindUser, DomainContract.messageAuthorKindAgent -> message.authorKind
                    else -> DomainContract.messageAuthorKindSystem
                }
                val body = messageRowBody(message.body, message.attachmentIds)
                if (body.isBlank()) continue
                val divider = if (author == DomainContract.messageAuthorKindUser) Chats.commandDivider(message.body) else null
                if (divider != null) {
                    rows.add(SessionChatRow.Divider(message.id, divider, message.createdAt))
                    continue
                }
                rows.add(
                    SessionChatRow.MessageRow(
                        id = message.id,
                        author = author,
                        authorId = message.authorId,
                        body = body,
                        queued = author == DomainContract.messageAuthorKindUser && message.consumedAt == null,
                        authorSessionId = message.authorSessionId,
                        xref = message.authorSessionId
                            ?.takeIf { it.isNotEmpty() && it != root.id }
                            ?.let { xref("message", it) },
                    ),
                )
                continue
            }
            when (card.str("kind")) {
                DomainContract.messageCardKindSession, DomainContract.messageCardKindThreadStarted -> {
                    val target = card.str("sessionId")?.let { byId[it] } ?: continue
                    rows.add(SessionChatRow.SessionRow(message.id, sessionCard(target, titles[target.id] ?: "", now)))
                }
                DomainContract.messageCardKindQuestion, DomainContract.messageCardKindPlan ->
                    askRow(message)?.let(rows::add)
                DomainContract.messageCardKindSubagent -> rows.add(
                    SessionChatRow.Subagent(id = message.id, title = card.text("title"), status = card.text("status")),
                )
                DomainContract.messageCardKindPicture -> {
                    val attachmentId = card.str("attachmentId") ?: continue
                    rows.add(
                        SessionChatRow.Picture(
                            id = message.id,
                            attachmentId = attachmentId,
                            width = card.int("width"),
                            height = card.int("height"),
                            caption = card.str("caption"),
                        ),
                    )
                }
                DomainContract.messageCardKindReport -> {
                    val target = card.str("sessionId")?.let { byId[it] } ?: continue
                    rows.add(
                        SessionChatRow.Report(
                            id = message.id,
                            sessionId = target.id,
                            text = ChatCopy.REPORT.replace(
                                "{session}",
                                sessionCard(target, titles[target.id] ?: "", now).name,
                            ),
                            summary = card.str("summary") ?: message.body,
                            xref = xref("report", target.id)!!,
                        ),
                    )
                }
                DomainContract.messageCardKindSuggestions -> {
                    suggestionsRow(message, card, byId, titles, issueMap, now)?.let(rows::add)
                }
                DomainContract.messageCardKindEvent -> {
                    // #EXP-1772: a firing draws nothing of its own; one the agent
                    // answered is THE marker above that reply (`fires`, below).
                    if (card.str("event") == DomainContract.messageEventKindTriggerFired) {
                        val named = !card.str("name").isNullOrBlank()
                        val text = (if (named) eventText(card, issueMap, prMap) else ChatCopy.TRIGGER_SCHEDULE)
                            ?: continue
                        fires.add(message.id)
                        rows.add(SessionChatRow.Event(message.id, DomainContract.messageEventKindTriggerFired, text, null))
                        continue
                    }
                    val text = eventText(card, issueMap, prMap) { id ->
                        byId[id]?.let { target -> sessionCard(target, titles[target.id] ?: "", now).name }
                    }
                    if (text != null) {
                        val about = card.str("sessionId")?.takeIf { byId.containsKey(it) }
                        rows.add(SessionChatRow.Event(message.id, card.text("event"), text, about))
                    }
                }
                DomainContract.messageCardKindSurface -> surfaceRow(message)?.let(rows::add)
                // NEW-1: a child's ask mirrored into its parent: the child's own
                // card already surfaces in the Conversation, the mirror draws nothing.
                // #EXP-1399: a mirror from ANOTHER tree (an agent's conversation
                // watching a chat, `escalate`) = the origin's session card + THE
                // origin card, read from its message.
                DomainContract.messageCardKindAsk -> {
                    val target = card.str("sessionId")?.let { byId[it] } ?: continue
                    val origin = card.str("messageId")?.let { messagesById[it] } ?: continue
                    if (target.id in treeIds || origin.sessionId != target.id) continue
                    // #EXP-1399 review: only the origin session's own mirror draws it.
                    if (message.authorSessionId != target.id) continue
                    val originKind = origin.card?.str("kind")
                    if (originKind != DomainContract.messageCardKindQuestion &&
                        originKind != DomainContract.messageCardKindPlan
                    ) continue
                    if (!escalated.add(origin.id)) continue
                    val ask = askRow(origin) ?: continue
                    rows.add(SessionChatRow.SessionRow(message.id, sessionCard(target, titles[target.id] ?: "", now)))
                    rows.add(ask)
                }
                else -> Unit
            }
        }

        // #EXP-1772: a firing stays only right above the chat's own agent reply.
        if (fires.isNotEmpty()) {
            val kept = rows.filterIndexed { index, row ->
                if (row.id !in fires) return@filterIndexed true
                val next = rows.getOrNull(index + 1) as? SessionChatRow.MessageRow
                next != null && next.author == DomainContract.messageAuthorKindAgent && next.xref == null
            }
            rows.clear()
            rows.addAll(kept)
        }

        val live = tree.filter { it.status != DomainContract.sessionStatusEnded }
        // NEW-6 (N-1): the root alone = the chat IS that session.
        val single = tree.all { it.id == root.id }
        val viewed = focused ?: root
        val starter = viewed.parentId
            ?: if (viewed.id == root.id && root.startedByAgentId != null) {
                byId.values.firstOrNull {
                    it.agentId == root.startedByAgentId && it.id == it.rootId && it.userId == root.userId
                }?.id
            } else null
        val origin = xref(
            if (viewed.status == DomainContract.sessionStatusEnded) "reported" else "started",
            starter,
        )
        return ChatView(
            rootId = root.id,
            title = title,
            private = !root.shared,
            focusId = focused?.id,
            tabs = if (chatMode(root.agentId) == ChatMode.Bubbles) {
                // EXP-1686 (chats.json AGENT CHAT): an agent's chat is the conversation only.
                listOf(ChatTab.Conversation)
            } else buildList {
                add(ChatTab.Add)
                // NEW-46: the Conversation tab is always in the strip; on a
                // single-session chat it opens `mainSessionId`'s work.
                add(ChatTab.Conversation)
                addAll(issueTabs)
                addAll(sessionTabs)
            },
            rows = rows,
            composer = Composer(root.id, ChatCopy.PLACEHOLDER),
            live = live.isNotEmpty(),
            busy = live.any { it.busy },
            needsYou = live.any { it.needsInput },
            liveSessionId = if (root.status == DomainContract.sessionStatusEnded) null else root.id,
            mainSessionId = if (single) root.id else null,
            origin = origin,
        )
    }

    // ── the Chats list + the agents ───────────────────────────────────────

    enum class ChatsGroupKey(val wire: String) {
        NeedsYou("needs_you"),
        Running("running"),
        Recent("recent"),
    }

    data class ChatsListRow(
        val sessionId: String,
        val teamId: String,
        val title: String,
        val busy: Boolean,
        val needsYou: Boolean,
        val activityAt: String,
        /** The ROOT's device (the row's trailing device icon); null = none. */
        val deviceId: String? = null,
        /** #EXP-1461: every list row is a ROOT (0, [parentId] null). */
        val depth: Int = 0,
        val parentId: String? = null,
        val rootId: String = sessionId,
        /** EXP-1323: bold title (the tree's newest agent message is newer
         *  than the viewer's read of the root). */
        val unread: Boolean = false,
        /** #EXP-1703: [treeOffline] (the fixture sets it only when true). */
        val offline: Boolean = false,
    )

    data class ChatsListGroup(val key: ChatsGroupKey, val title: String, val rows: List<ChatsListRow>)

    /**
     * The Chats page list: roots of kind `thread` in scope ([teamId] = the
     * ACTIVE team; null = every team, phones band rows per team), grouped
     * Needs you · Running · Recent, each by the tree's newest `updatedAt`
     * desc. Empty groups are omitted.
     */
    fun partitionChats(
        sessions: List<Session>,
        sessionIssues: List<SessionIssue>,
        issues: List<Issue>,
        messages: List<Message>,
        teamId: String?,
        /** EXP-1323: the viewer's `session_reads` (empty = none read). */
        reads: List<SessionRead> = emptyList(),
        /** NEW-1: the clock for Recent's window (null = no window: the last
         *  [ListCopy.RECENT_MAX] only). */
        now: String? = null,
    ): List<ChatsListGroup> {
        val readAt = reads.associate { it.sessionId to (ms(it.readAt) ?: 0L) }
        val rootOf = sessions.associate { it.id to it.rootId }
        val newestAgentInTree = HashMap<String, Long>()
        for (message in messages) {
            if (message.authorKind != DomainContract.messageAuthorKindAgent) continue
            val at = ms(message.createdAt) ?: 0L
            val rootId = rootOf[message.sessionId] ?: continue
            newestAgentInTree[rootId] = maxOf(newestAgentInTree[rootId] ?: 0L, at)
        }
        fun unreadAt(newest: Long?, sessionId: String) = newest != null && newest > (readAt[sessionId] ?: -1L)
        // Converted once: titles are resolved per row below.
        val titleLinks = sessionIssues.map(::titleLink)
        val titleIssues = issues.map(::titleIssue)
        val titleMessages = messages.map(::titleMessage)
        fun titleOf(s: Session) = SessionTitle.sessionTitle(titleSession(s), titleLinks, titleIssues, titleMessages)
        val roots = sessions.filter {
            it.parentId == null &&
                // NEW-2: an agent's conversation (a root with `agentId`) is no chat;
                // VAPP-110: neither is an app's editor session (`appId`).
                it.agentId.isNullOrEmpty() &&
                it.appId.isNullOrEmpty() &&
                (teamId == null || it.teamId == teamId)
        }
        val byRoot = LinkedHashMap<String, MutableList<Session>>()
        for (session in sessions) byRoot.getOrPut(session.rootId) { mutableListOf() }.add(session)
        val buckets = linkedMapOf<ChatsGroupKey, MutableList<ChatsListRow>>(
            ChatsGroupKey.NeedsYou to mutableListOf(),
            ChatsGroupKey.Running to mutableListOf(),
            ChatsGroupKey.Recent to mutableListOf(),
        )
        for (root in roots) {
            val tree = byRoot[root.id] ?: listOf(root)
            val live = tree.filter { it.status != DomainContract.sessionStatusEnded }
            val needsYou = live.any { it.needsInput }
            val running = live.any { it.status == DomainContract.sessionStatusQueued || it.busy }
            val activity = tree.fold(root.updatedAt) { latest, s ->
                if ((ms(s.updatedAt) ?: 0L) > (ms(latest) ?: 0L)) s.updatedAt else latest
            }
            val title = titleOf(root)
            val row = ChatsListRow(
                sessionId = root.id,
                teamId = root.teamId,
                title = title,
                busy = live.any { it.busy },
                needsYou = needsYou,
                activityAt = activity,
                deviceId = root.deviceId,
                depth = 0,
                parentId = null,
                rootId = root.id,
                unread = unreadAt(newestAgentInTree[root.id], root.id),
                offline = needsYou && treeOffline(tree),
            )
            // #EXP-1461 (owner 2026-10-30: 'we don't wanna see all the nested
            // child sessions, only the root ones'): a row is a ROOT, never a
            // child; the tree's state (needs you, working, unread) rolls up.
            val key = when {
                needsYou -> ChatsGroupKey.NeedsYou
                running -> ChatsGroupKey.Running
                else -> ChatsGroupKey.Recent
            }
            buckets.getValue(key).add(row)
        }
        // Newest activity first, then by session id: the time parsed once per row.
        val activity = HashMap<String?, Long>()
        fun activityMs(row: ChatsListRow): Long = activity.getOrPut(row.activityAt) { ms(row.activityAt) ?: 0L }
        val tie = compareBy<ChatsListRow> { it.sessionId }
        // NEW-1: Recent = every tree active in the last `recent.hours`, filled
        // up to the last `recent.max` by activity. Ended trees stay openable.
        val at = now?.let { ms(it) }
        val windowMs = ListCopy.RECENT_HOURS * 60L * 60L * 1000L
        return buckets.entries
            .map { (key, rows) ->
                var sorted = rows.sortedByEpoch(descending = true, tie = tie) { activityMs(it) }
                if (key == ChatsGroupKey.Recent) {
                    sorted = sorted.filterIndexed { index, row ->
                        index < ListCopy.RECENT_MAX ||
                            (at != null && at - activityMs(row) <= windowMs)
                    }
                }
                ChatsListGroup(key, ListCopy.GROUPS.getValue(key), sorted)
            }
            .filter { it.rows.isNotEmpty() }
    }

    /**
     * #PERF-9 / #FEED-232: exactly `NavBadges.chatsBadge` over [partitionChats]
     * for the Chats tab's count badge, without building the list: no titles,
     * no rows, and a root's activity time (the costly timestamp parsing) only
     * when an unread Recent root has to be ranked against Recent's cut. The
     * nav bar reads it under an open chat on every sync tick.
     */
    fun chatsNavBadge(
        sessions: List<Session>,
        messages: List<Message>,
        teamId: String?,
        reads: List<SessionRead> = emptyList(),
        now: String? = null,
    ): NavBadges.Badge {
        val roots = sessions.filter {
            it.parentId == null && it.agentId.isNullOrEmpty() && it.appId.isNullOrEmpty() &&
                (teamId == null || it.teamId == teamId)
        }
        if (roots.isEmpty()) return NavBadges.NONE
        val byRoot = HashMap<String, MutableList<Session>>()
        for (session in sessions) byRoot.getOrPut(session.rootId) { mutableListOf() }.add(session)
        val recent = ArrayList<Session>()
        val needs = HashSet<String>()
        val running = ArrayList<String>()
        for (root in roots) {
            val tree = byRoot[root.id] ?: listOf(root)
            val live = tree.filter { it.status != DomainContract.sessionStatusEnded }
            when {
                live.any { it.needsInput } -> needs += root.id
                live.any { it.status == DomainContract.sessionStatusQueued || it.busy } -> running += root.id
                else -> recent += root
            }
        }
        // Unread = an agent message newer than the viewer's read mark, on a
        // root outside the archive (a root that needs you counts anyway).
        val archived = reads.filter { !it.archivedAt.isNullOrEmpty() }.mapTo(HashSet()) { it.sessionId }
        val readAt = reads.associate { it.sessionId to it.readAt }
        val rootOf = HashMap<String, String>(sessions.size)
        for (session in sessions) rootOf[session.id] = session.rootId
        val rootIds = roots.mapTo(HashSet()) { it.id }
        val newestAgent = HashMap<String, Long>()
        for (message in messages) {
            if (message.authorKind != DomainContract.messageAuthorKindAgent) continue
            val rootId = rootOf[message.sessionId] ?: continue
            if (rootId !in rootIds) continue
            val at = ms(message.createdAt) ?: 0L
            newestAgent[rootId] = maxOf(newestAgent[rootId] ?: 0L, at)
        }
        fun unread(rootId: String): Boolean {
            if (rootId in archived) return false
            val newest = newestAgent[rootId] ?: return false
            return newest > (readAt[rootId]?.let { ms(it) ?: 0L } ?: -1L)
        }
        var count = needs.size + running.count(::unread)
        val candidates = recent.filter { unread(it.id) }
        if (candidates.isNotEmpty()) {
            // Recent's cut, exactly as [partitionChats] makes it.
            fun activity(root: Session): Long = (byRoot[root.id] ?: listOf(root))
                .fold(ms(root.updatedAt) ?: 0L) { latest, s -> maxOf(latest, ms(s.updatedAt) ?: 0L) }
            val at = now?.let { ms(it) }
            val windowMs = ListCopy.RECENT_HOURS * 60L * 60L * 1000L
            val ids = candidates.mapTo(HashSet()) { it.id }
            val activityById = HashMap<String, Long>()
            val sorted = recent.sortedByEpoch(descending = true, tie = compareBy { it.id }) { root ->
                activity(root).also { activityById[root.id] = it }
            }
            count += sorted.withIndex().count { (index, root) ->
                root.id in ids && (
                    index < ListCopy.RECENT_MAX ||
                        (at != null && at - activityById.getValue(root.id) <= windowMs)
                    )
            }
        }
        return NavBadges.badge(count, attention = needs.isNotEmpty())
    }

    /**
     * NEW-1 / S-3: a tree's derived state = the session's four. `needs_you` =
     * a live session waits on a person (needsInput: a card, a usage wall or a
     * watchdog flag); `working` = any live session queued or busy; `done` =
     * every session ended, or every PR of the chat merged (none = never
     * done this way); else `idle`. [now] is kept for the signature.
     */
    @Suppress("UNUSED_PARAMETER")
    fun treeState(tree: List<Session>, pullRequests: List<PullRequest>, now: String): SessionState {
        val live = tree.filter { it.status != DomainContract.sessionStatusEnded }
        if (live.any { it.needsInput }) return SessionState.NeedsYou
        if (live.any { it.status == DomainContract.sessionStatusQueued || it.busy }) return SessionState.Working
        if (live.isEmpty()) return SessionState.Done
        if (pullRequests.isNotEmpty() && pullRequests.all { it.state == DomainContract.prStateMerged }) {
            return SessionState.Done
        }
        return SessionState.Idle
    }

    /** EXP-1323: the Chats nav dot = any chat row is unread (`unreadCases`). */
    fun chatsUnread(groups: List<ChatsListGroup>): Boolean =
        groups.any { group -> group.rows.any { it.depth == 0 && it.unread } }

    data class AgentRow(
        val agentId: String,
        val teamId: String,
        val name: String,
        /** A curated board icon name (contract `boardIcon`). */
        val icon: String,
        /** `#rrggbb`. */
        val color: String,
        val busy: Boolean,
        val needsYou: Boolean,
        /** S-3: the viewer's own Assistant (agents.role assistant): the brand
         *  mark instead of icon/color, always first. */
        val assistant: Boolean = false,
    )

    /** NEW-2: an `agents` definition row as the list needs it. */
    data class AgentDefinition(
        val id: String,
        val teamId: String,
        val name: String,
        val icon: String?,
        val color: String?,
        val sortOrder: Double,
        /** S-3: contract `agentRole` (absent = `agent`). */
        val role: String? = null,
        /** S-3: who owns it (an Assistant shows to its owner only). */
        val ownerId: String? = null,
    ) {
        val isAssistant: Boolean get() = role == DomainContract.agentRoleAssistant
    }

    /**
     * The Agents list (rail above Boards, Settings → Agents): one row per
     * `agents` definition of the team (NEW-2), icon + color like boards, by
     * `sortOrder` then name then id. The spinner / needs-you dot read the
     * VIEWER's own conversation session with the agent (a root with `agentId`
     * and `userId == me`); a member without one sees a quiet row.
     */
    fun listAgents(
        agents: List<AgentDefinition>,
        sessions: List<Session>,
        teamId: String?,
        me: String?,
        teamIds: Collection<String>? = null,
    ): List<AgentRow> {
        val mine = HashMap<String, Session>()
        for (s in sessions) {
            val agentId = s.agentId
            if (s.parentId == null && !agentId.isNullOrEmpty() && s.userId == me) mine[agentId] = s
        }
        // #EXP-1714: with the synced teams known, the team also lists the
        // viewer's global Assistant (`AgentSettings.Assistant.agentsInTeam`).
        val inTeam = when {
            teamId == null -> agents
            teamIds == null -> agents.filter { it.teamId == teamId }
            else -> AgentSettings.Assistant.agentsInTeam(agents, teamId, teamIds, me) {
                AgentSettings.Assistant.TeamAgentLike(it.teamId, it.role, it.ownerId)
            }
        }
        return inTeam
            // S-3: another member's Assistant is never listed
            // (agent-settings.json `assistant.visibleCases`); the viewer's own
            // comes first.
            .filter { !it.isAssistant || it.ownerId == me }
            .sortedWith(compareBy<AgentDefinition>({ if (it.isAssistant) 0 else 1 }, { it.sortOrder }, { it.name }, { it.id }))
            .map {
                val conversation = mine[it.id]
                val live = conversation != null && conversation.status != DomainContract.sessionStatusEnded
                AgentRow(
                    agentId = it.id,
                    teamId = it.teamId,
                    name = it.name,
                    icon = it.icon ?: ListCopy.AGENT_DEFAULT_ICON,
                    color = it.color ?: ListCopy.AGENT_DEFAULT_COLOR,
                    busy = live && conversation!!.busy,
                    needsYou = live && conversation!!.needsInput,
                    assistant = it.isAssistant,
                )
            }
    }

    /** S-3 (session-chat.json `chatModeCases`): THE chat screen's mode:
     *  an agent conversation paints chat bubbles, a chat its transcript. */
    enum class ChatMode(val wire: String) { Bubbles("bubbles"), Transcript("transcript") }

    fun chatMode(rootAgentId: String?): ChatMode =
        if (!rootAgentId.isNullOrEmpty()) ChatMode.Bubbles else ChatMode.Transcript

    /** EXP-1686 (chats.json `agentChat`): the line under an agent's name on a
     *  phone header ([modelLabel] = the model's display name). */
    fun agentChatSubtitle(modelLabel: String?): String =
        if (modelLabel.isNullOrEmpty()) {
            Chats.AgentChat.SUBTITLE
        } else {
            "${Chats.AgentChat.SUBTITLE}${Chats.AgentChat.SUBTITLE_MODEL_SEPARATOR}$modelLabel"
        }

    /** NEW-2: the viewer's own conversation session per agent id (an Agents
     *  row opens it; absent = none yet, `sessions.send({agentId})` makes it). */
    fun conversations(sessions: List<Session>, me: String?): Map<String, String> {
        val out = HashMap<String, String>()
        for (s in sessions) {
            val agentId = s.agentId
            if (s.parentId == null && !agentId.isNullOrEmpty() && s.userId == me) out[agentId] = s.id
        }
        return out
    }

    fun agentDefinition(row: AgentDefinitionEntity): AgentDefinition = AgentDefinition(
        id = row.id,
        teamId = row.teamId,
        name = row.name,
        icon = row.icon,
        color = row.color,
        sortOrder = row.sortOrder,
        role = row.role,
        ownerId = row.ownerId,
    )

    // ── the Guide's PR + the session composer (EXP-1306) ────────────────────

    /**
     * The ONE pull request the chat's Guide speaks for (its stack rail + the
     * bottom Merge capsule): the newest OPEN one, else the newest MERGED one
     * (createdAt, ties by the higher number); a closed PR never counts.
     * Replays `session-chat.json` `guidePullRequestCases`.
     */
    fun guidePullRequest(pullRequests: List<PullRequestEntity>): PullRequestEntity? {
        fun newest(state: String): PullRequestEntity? = pullRequests
            .filter { it.state == state }
            .fold<PullRequestEntity, PullRequestEntity?>(null) { best, pr ->
                if (best == null) return@fold pr
                val diff = (ms(pr.createdAt) ?: 0L) - (ms(best.createdAt) ?: 0L)
                if (diff > 0 || (diff == 0L && (pr.number ?: 0) > (best.number ?: 0))) pr else best
            }
        return newest(DomainContract.prStateOpen) ?: newest(DomainContract.prStateMerged)
    }

    /** EXP-1211: one quick reply (the composer row's pill). */
    data class ComposerReply(val id: String, val text: String)

    /** EXP-1211: the row above a session's composer; [messageId] = its card. */
    data class ComposerReplies(val messageId: String, val items: List<ComposerReply>)

    /**
     * EXP-1211 ×4 (`repliesCases`): the session's NEWEST `replies` card (by
     * createdAt, then id); sent, id-less and blank items drop; null when none
     * remain. A replies card never renders as a Conversation row.
     */
    fun composerReplies(sessionId: String, messages: List<Message>): ComposerReplies? {
        var newest: Message? = null
        for (message in messages) {
            if (message.sessionId != sessionId || message.card?.str("kind") != DomainContract.messageCardKindReplies) continue
            val best = newest
            val at = ms(message.createdAt) ?: 0L
            val bestAt = best?.let { ms(it.createdAt) ?: 0L }
            if (best == null || at > bestAt!! || (at == bestAt && message.id > best.id)) newest = message
        }
        val card = newest?.card ?: return null
        val raws = card["items"] as? JsonArray ?: return null
        val items = raws.mapNotNull { element ->
            val raw = element as? JsonObject ?: return@mapNotNull null
            val id = raw.str("id") ?: return@mapNotNull null
            if ((raw["sent"] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true") return@mapNotNull null
            val text = raw.str("text")?.takeIf { it.isNotBlank() } ?: return@mapNotNull null
            ComposerReply(id, text)
        }
        return if (items.isEmpty()) null else ComposerReplies(newest.id, items)
    }

    /** EXP-1309/1318 (`chat-list.json` `copy.words`): a Chats row's ONE
     *  quiet word ([wire] = the fixture key). EXP-1339: [Merged] has NO text
     *  (null), it only turns the lead pr-merged violet. */
    enum class ChatRowWord(val wire: String, val text: String?) {
        Stack("stack", "stack"),
        ReadyToMerge("readyToMerge", "ready to merge"),
        Merged("merged", null),

        /** #EXP-1703: on a person only because its device stayed away. */
        Offline("offline", "device offline"),
        Question("question", "question"),

        /** An agent row waiting on a person. */
        NeedsInput("needsInput", "needs input"),
    }

    /** A PR's stack edge (`chat-list.json` `stackCases`). */
    data class StackPr(val id: String, val state: String, val basePullRequestId: String?)

    /**
     * An open PR of the chat sits in an open stack: based on another open
     * PR, or another open PR is based on it. [teamPrs] = the team's PRs (a
     * stack edge may cross chats). Replays `stackCases`.
     */
    fun chatInStack(pullRequests: List<StackPr>, teamPrs: List<StackPr> = pullRequests): Boolean {
        val openIds = teamPrs.filter { it.state == DomainContract.prStateOpen }.map { it.id }.toSet()
        return pullRequests.filter { it.state == DomainContract.prStateOpen }.any { pr ->
            (pr.basePullRequestId != null && pr.basePullRequestId in openIds) ||
                teamPrs.any { it.state == DomainContract.prStateOpen && it.basePullRequestId == pr.id }
        }
    }

    /**
     * Word order (`wordCases`): an open PR in a stack → [ChatRowWord.Stack];
     * an open PR while nothing works → [ChatRowWord.ReadyToMerge] (working =
     * no word); no open PR but a merged one → [ChatRowWord.Merged]; else
     * waiting on a person → [ChatRowWord.Question] (#EXP-1703: [offline] =
     * [treeOffline] → [ChatRowWord.Offline]); else none. [states] = the
     * root's PR states.
     */
    fun chatRowWord(
        states: List<String>,
        busy: Boolean,
        needsYou: Boolean,
        inStack: Boolean,
        offline: Boolean = false,
    ): ChatRowWord? {
        if (states.any { it == DomainContract.prStateOpen }) {
            if (inStack) return ChatRowWord.Stack
            return if (busy) null else ChatRowWord.ReadyToMerge
        }
        if (states.any { it == DomainContract.prStateMerged }) return ChatRowWord.Merged
        if (needsYou) return if (offline) ChatRowWord.Offline else ChatRowWord.Question
        return null
    }

    /** #EXP-1703 (TS `treeOffline`, `offlineCases`): a tree on a person ONLY
     *  because its device stayed away: some live session needs you and every
     *  one that does is [sessionOffline]. */
    fun treeOffline(tree: List<Session>): Boolean {
        val waiting = tree.filter { it.status != DomainContract.sessionStatusEnded && it.needsInput }
        return waiting.isNotEmpty() && waiting.all(::sessionOffline)
    }

    private fun stackPr(row: PullRequestEntity) = StackPr(row.id, row.state, row.basePullRequestId)

    /** [chatRowWord] off synced PR rows. */
    fun chatRowWord(
        busy: Boolean,
        needsYou: Boolean,
        prs: List<PullRequestEntity>,
        teamPrs: List<PullRequestEntity> = prs,
        offline: Boolean = false,
    ): ChatRowWord? = chatRowWord(
        states = prs.map { it.state },
        busy = busy,
        needsYou = needsYou,
        inStack = chatInStack(prs.map(::stackPr), teamPrs.map(::stackPr)),
        offline = offline,
    )

    /**
     * EXP-1318 the rail's Active band (`railCases`): every Needs you row,
     * every Running row, then the Recent rows whose word still asks for a
     * merge (ready to merge or stack), in group order.
     */
    fun railChats(groups: List<ChatsListGroup>, wordOf: (ChatsListRow) -> ChatRowWord?): List<ChatsListRow> =
        groups.flatMap { group ->
            if (group.key == ChatsGroupKey.Recent) {
                group.rows.filter {
                    val word = wordOf(it)
                    word == ChatRowWord.ReadyToMerge || word == ChatRowWord.Stack
                }
            } else {
                group.rows
            }
        }

    // ── synced rows → the pure inputs ───────────────────────────────────────

    private val json = Json { ignoreUnknownKeys = true }

    private fun parseObject(raw: String?): JsonObject? {
        if (raw.isNullOrBlank()) return null
        return runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonObject
    }

    private fun parseArray(raw: String?): JsonArray? {
        if (raw.isNullOrBlank()) return null
        return runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonArray
    }

    fun session(row: SessionEntity): Session = Session(
        id = row.id,
        teamId = row.teamId,
        parentId = row.parentId,
        rootId = row.rootId.ifEmpty { row.id },
        userId = row.userId,
        status = row.status,
        busy = row.busy,
        needsInput = row.needsInput,
        blocked = parseObject(row.blocked)?.let {
            Blocked(it.str("window"), it.str("resetsAt"), it.str("kind"), (it["resume"] as? JsonPrimitive)?.booleanOrNull, it.str("resumeSince"))
        },
        shared = row.shared,
        title = row.title,
        guide = parseArray(row.guide)?.map { element ->
            val obj = element as? JsonObject
            GuideEntry(text = obj?.str("text"), label = obj?.str("label"))
        },
        startedAt = row.startedAt,
        endedAt = row.endedAt,
        createdAt = row.createdAt,
        updatedAt = row.updatedAt,
        deviceId = row.deviceId.ifBlank { null },
        agentId = row.agentId,
        appId = row.appId,
        startedByAgentId = row.startedByAgentId,
        background = row.background,
    )

    fun sessionIssue(row: SessionIssueEntity): SessionIssue =
        SessionIssue(
            sessionId = row.sessionId,
            issueId = row.issueId,
            sortOrder = row.sortOrder,
        )

    fun issue(row: IssueEntity, parentId: String? = null): Issue =
        Issue(id = row.id, identifier = row.identifier, title = row.title, parentId = parentId)

    /** EXP-1334: child → parent off the synced `parent` relations
     *  (`issue_id` = the parent); several parents = the lowest id wins. */
    fun issueParents(relations: List<IssueRelationEntity>): Map<String, String> {
        val parents = HashMap<String, String>()
        for (relation in relations) {
            if (relation.type != DomainContract.issueRelationTypeParent || relation.issueId == relation.relatedIssueId) continue
            val current = parents[relation.relatedIssueId]
            if (current == null || relation.issueId < current) parents[relation.relatedIssueId] = relation.issueId
        }
        return parents
    }

    /** #EXP-1730: the rows keyed by their own `messages.session_id`
     *  ([MessageSession.keyMessagesBySession]); a row without one (a
     *  dm/channel message) is left out, input order kept. */
    fun messages(rows: List<MessageEntity>): List<Message> =
        MessageSession.keyMessagesBySession(rows) { it.sessionId }
            .map { (row, sessionId) -> message(row, sessionId) }

    /** [sessionId] = the session the row belongs to (resolved by [messages]
     *  or by a DAO query scoped to that session). */
    fun message(row: MessageEntity, sessionId: String): Message {
        // EXP-1459: the DAO already opened what this install can; what stays
        // sealed is LOCKED (empty body, the card's clear part). Per message
        // id: an opened row is plain here, a locked one reuses the DAO's
        // outcome (no second native open).
        val opened = SealedContent.openMessage(row)
        return Message(
            id = row.id,
            sessionId = sessionId,
            authorKind = row.authorKind,
            authorId = row.authorId,
            body = opened.body,
            card = parseObject(opened.cardJson),
            answeredAt = row.answeredAt,
            consumedAt = row.consumedAt,
            createdAt = row.createdAt,
            authorSessionId = row.authorSessionId,
            locked = opened.locked,
            attachmentIds = row.attachmentIdList,
        )
    }

    fun sessionRead(row: SessionReadEntity): SessionRead = SessionRead(sessionId = row.sessionId, readAt = row.readAt, archivedAt = row.archivedAt)

    fun pullRequest(row: PullRequestEntity): PullRequest? =
        row.number?.let { PullRequest(id = row.id, number = it, state = row.state) }

    /** The ISO `now` the view models take, off the wall clock. */
    fun nowIso(nowMs: Long = System.currentTimeMillis()): String = java.time.Instant.ofEpochMilli(nowMs).toString()
}
