package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.SessionIssueEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.TeamMemberEntity
import com.exponential.app.data.db.UserEntity
import java.time.LocalDate
import java.time.OffsetDateTime
import java.time.ZoneId
import kotlin.math.floor

// VAPP-119: the `exp:` binding SOURCES beyond devices and records, the Android
// mirror of the pure reference `packages/domain-contract/src/app-sources.ts`
// (fixtures/app-sources.json is the spec; `AppSourcesTest` replays its cases
// and locks the constants): what `exp:issues`, `exp:boards`, `exp:members`,
// `exp:sessions`, `exp:comments` and `exp:attachments` emit over the synced
// rows of a team.
object AppSources {
    // Fixture `sources`, `limits` (locked by AppSourcesTest).
    val SOURCES: List<String> = listOf("issues", "boards", "members", "me", "sessions", "comments", "attachments")
    const val LIMIT_DEFAULT = 50
    const val LIMIT_MAX = 500

    data class Board(
        val id: String,
        val slug: String,
        val name: String,
        val prefix: String,
        val icon: String?,
        val color: String?,
        val sortOrder: Double,
        val deletedAt: String?,
    )

    data class Issue(
        val id: String,
        val boardId: String,
        val identifier: String,
        val title: String,
        val status: String,
        val priority: String?,
        val assigneeId: String?,
        val creatorId: String?,
        val dueDate: String?,
        val sortOrder: Double,
        val createdAt: String,
        val updatedAt: String,
    )

    data class Member(val userId: String, val role: String)

    data class User(val id: String, val name: String, val email: String?, val image: String?)

    data class Session(
        val id: String,
        val parentId: String?,
        val agentId: String?,
        val appId: String?,
        val userId: String?,
        val status: String,
        val busy: Boolean,
        val needsInput: Boolean,
        val title: String?,
        val createdAt: String,
        val updatedAt: String,
    )

    data class Comment(
        val id: String,
        val issueId: String,
        val authorId: String?,
        val parentId: String?,
        val source: String,
        val body: String,
        val createdAt: String,
    )

    data class Attachment(
        val id: String,
        val issueId: String?,
        val commentId: String?,
        val uploaderId: String?,
        val filename: String,
        val contentType: String,
        val sizeBytes: Long,
        val width: Int?,
        val height: Int?,
        val createdAt: String,
    )

    /** FEED-243: an issue's timeline row (`issue_events`), only who and when. */
    data class IssueEvent(val id: String, val issueId: String, val actorUserId: String?, val createdAt: String)

    /** FEED-243: a session linked to an issue (`session_issues`). */
    data class SessionIssue(val sessionId: String, val issueId: String)

    /** The synced rows of ONE team, as the host reads them. */
    data class Store(
        val boards: List<Board> = emptyList(),
        val issues: List<Issue> = emptyList(),
        val members: List<Member> = emptyList(),
        val users: List<User> = emptyList(),
        val sessions: List<Session> = emptyList(),
        val comments: List<Comment> = emptyList(),
        val attachments: List<Attachment> = emptyList(),
        val issueEvents: List<IssueEvent> = emptyList(),
        val sessionIssues: List<SessionIssue> = emptyList(),
    )

    /** UTF-16 code-unit order (JavaScript's `<`), no locale: the same ×4. */
    private fun cmp(a: String, b: String): Int = a.compareTo(b).coerceIn(-1, 1)

    private fun str(v: String?): JsonValue = v?.let { JsonValue.Str(it) } ?: JsonValue.Null
    private fun num(v: Number?): JsonValue = v?.let { JsonValue.Num(it.toDouble()) } ?: JsonValue.Null
    private fun obj(vararg entries: Pair<String, JsonValue>) = JsonValue.Obj(linkedMapOf(*entries))
    private fun list(rows: List<JsonValue>, count: Int, vararg extra: Pair<String, JsonValue>) =
        obj("rows" to JsonValue.Arr(rows), "count" to JsonValue.Num(count.toDouble()), *extra)

    /** JavaScript's `Number(text)` for a decimal literal (trimmed, "" = 0); null = NaN. */
    private val DECIMAL = Regex("^[+-]?(\\d+\\.?\\d*|\\.\\d+)([eE][+-]?\\d+)?$")
    private fun jsNumber(raw: String?): Double? {
        if (raw == null) return null
        val t = raw.trim()
        if (t.isEmpty()) return 0.0
        return if (DECIMAL.matches(t)) t.toDoubleOrNull() else null
    }

    fun limitOf(params: Map<String, String>): Int {
        val n = jsNumber(params["limit"])
        return if (n != null && n.isFinite() && n == floor(n) && n > 0) minOf(n, LIMIT_MAX.toDouble()).toInt() else LIMIT_DEFAULT
    }

    private fun userName(store: Store, id: String?): String =
        id?.let { u -> store.users.firstOrNull { it.id == u }?.name }?.takeIf { it.isNotEmpty() } ?: ""

    /** A board by id or slug. */
    private fun boardOf(store: Store, ref: String?): Board? {
        if (ref.isNullOrEmpty()) return null
        return store.boards.firstOrNull { it.id == ref || it.slug == ref }
    }

    /** An issue by identifier (case-insensitive) or id. */
    private fun issueOf(store: Store, ref: String?): Issue? {
        if (ref.isNullOrEmpty()) return null
        val upper = ref.uppercase()
        return store.issues.firstOrNull { it.id == ref || it.identifier.uppercase() == upper }
    }

    private val DAY = Regex("^\\d{4}-\\d{2}-\\d{2}$")
    private val INSTANT = Regex("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}(\\.\\d+)?(Z|[+-]\\d{2}:\\d{2})$")

    /** A window bound in epoch ms; [open] = exclusive (a date-only `until`). */
    private data class Bound(val ms: Long, val open: Boolean)

    /** FEED-243: a `since` / `until` bound. A date-only bound reads in [zone]:
     *  `since` = the day's start (inclusive), `until` = the NEXT day's start
     *  (exclusive); an instant is inclusive both ways; anything else = no bound. */
    private fun boundOf(raw: String?, end: Boolean, zone: ZoneId): Bound? {
        val v = raw?.trim()?.takeIf { it.isNotEmpty() } ?: return null
        if (DAY.matches(v)) {
            val day = runCatching { LocalDate.parse(v) }.getOrNull() ?: return null
            return if (end) Bound(day.plusDays(1).atStartOfDay(zone).toInstant().toEpochMilli(), true)
            else Bound(day.atStartOfDay(zone).toInstant().toEpochMilli(), false)
        }
        if (!INSTANT.matches(v)) return null
        return runCatching { OffsetDateTime.parse(v).toInstant().toEpochMilli() }.getOrNull()?.let { Bound(it, false) }
    }

    /** The latest in-window touch of an issue: [at] = the stored timestamp. */
    private class Touch(var at: String, var ms: Long, val touches: MutableList<String>)

    /** FEED-243: the issues [actor] touched inside [since, until]: an
     *  `issue_events` row they acted, a comment they wrote, a session they
     *  started linked to the issue (at its start and its last update). */
    private fun touchesOf(store: Store, actor: String, params: Map<String, String>, timeZone: String?): Map<String, Touch> {
        val zone = TeamZone.hostZoneId(timeZone)
        val since = boundOf(params["since"], false, zone)
        val until = boundOf(params["until"], true, zone)
        val out = LinkedHashMap<String, Touch>()
        fun add(issueId: String, kind: String, at: String) {
            val ms = WireTimestamps.parseEpochMs(at) ?: return
            if (since != null && ms < since.ms) return
            if (until != null && (if (until.open) ms >= until.ms else ms > until.ms)) return
            val t = out[issueId]
            if (t == null) {
                out[issueId] = Touch(at, ms, mutableListOf(kind))
                return
            }
            if (kind !in t.touches) t.touches.add(kind)
            if (ms > t.ms) {
                t.at = at
                t.ms = ms
            }
        }
        for (e in store.issueEvents) if (e.actorUserId == actor) add(e.issueId, "event", e.createdAt)
        for (c in store.comments) if (c.authorId == actor) add(c.issueId, "comment", c.createdAt)
        val sessions = store.sessions.associateBy { it.id }
        for (link in store.sessionIssues) {
            val s = sessions[link.sessionId] ?: continue
            if (s.userId != actor) continue
            add(link.issueId, "session", s.createdAt)
            add(link.issueId, "session", s.updatedAt)
        }
        for (t in out.values) t.touches.sortWith { a, b -> cmp(a, b) }
        return out
    }

    fun issuesValue(store: Store, params: Map<String, String>, me: String?, timeZone: String? = null): JsonValue {
        val boardRef = params["board"]
        val board = boardOf(store, boardRef)
        if (!boardRef.isNullOrEmpty() && board == null) return list(emptyList(), 0)
        val assigneeParam = params["assignee"]?.takeIf { it.isNotEmpty() }
        val assignee = if (assigneeParam == "me") me else assigneeParam
        val status = params["status"]?.takeIf { it.isNotEmpty() }
        val priority = params["priority"]?.takeIf { it.isNotEmpty() }
        val search = params["search"]?.trim()?.lowercase()?.takeIf { it.isNotEmpty() }
        // FEED-243: `touched=me|<userId>` (signed out + me = no rows).
        val touchedParam = params["touched"]?.takeIf { it.isNotEmpty() }
        val touched: Map<String, Touch>? = when {
            touchedParam == null -> null
            touchedParam == "me" && me == null -> emptyMap()
            else -> touchesOf(store, if (touchedParam == "me") me!! else touchedParam, params, timeZone)
        }
        val matched = store.issues.filter { i ->
            if (touched != null && i.id !in touched) return@filter false
            if (board != null && i.boardId != board.id) return@filter false
            if (status != null && i.status != status) return@filter false
            if (priority != null && (i.priority ?: "") != priority) return@filter false
            if (assigneeParam == "none" && i.assigneeId != null) return@filter false
            if (assigneeParam != null && assigneeParam != "none" && i.assigneeId != assignee) return@filter false
            if (search != null && !i.title.lowercase().contains(search) && !i.identifier.lowercase().contains(search)) return@filter false
            true
        }
        val explicit = params["sort"]?.takeIf {
            it == "updatedAt" || it == "createdAt" || it == "sortOrder" || (touched != null && it == "touchedAt")
        }
        val sort = explicit ?: if (touched != null) "touchedAt" else "sortOrder"
        val asc = when (params["order"]) {
            "desc" -> false
            "asc" -> true
            else -> sort == "sortOrder"
        }
        val rows = matched.sortedWith { a, b ->
            val c = when (sort) {
                "updatedAt" -> cmp(a.updatedAt, b.updatedAt)
                "createdAt" -> cmp(a.createdAt, b.createdAt)
                "touchedAt" -> touched!![a.id]!!.ms.compareTo(touched[b.id]!!.ms)
                else -> a.sortOrder.compareTo(b.sortOrder)
            }
            if (c != 0) (if (asc) c else -c) else cmp(a.id, b.id)
        }
        val boards = store.boards.associateBy { it.id }
        return list(
            rows.take(limitOf(params)).map { i ->
                val t = touched?.get(i.id)
                val touchFields = if (t == null) emptyArray<Pair<String, JsonValue>>() else arrayOf<Pair<String, JsonValue>>(
                    "touchedAt" to JsonValue.Str(t.at),
                    "touches" to JsonValue.Arr(t.touches.map { JsonValue.Str(it) }),
                )
                obj(
                    "id" to JsonValue.Str(i.id),
                    "identifier" to JsonValue.Str(i.identifier),
                    "title" to JsonValue.Str(i.title),
                    // A Select / ChipInput option (shape `option`): bind the rows as its options.
                    "value" to JsonValue.Str(i.identifier),
                    "label" to JsonValue.Str("${i.identifier} ${i.title}"),
                    "status" to JsonValue.Str(i.status),
                    "priority" to str(i.priority),
                    "assignee" to str(i.assigneeId),
                    "assigneeName" to JsonValue.Str(userName(store, i.assigneeId)),
                    "board" to JsonValue.Str(i.boardId),
                    "boardSlug" to JsonValue.Str(boards[i.boardId]?.slug ?: ""),
                    "dueDate" to str(i.dueDate),
                    "updatedAt" to JsonValue.Str(i.updatedAt),
                    *touchFields,
                )
            },
            rows.size,
        )
    }

    fun boardsValue(store: Store, params: Map<String, String>): JsonValue {
        val rows = store.boards.filter { it.deletedAt == null }.sortedWith { a, b ->
            a.sortOrder.compareTo(b.sortOrder).takeIf { it != 0 } ?: cmp(a.name, b.name).takeIf { it != 0 } ?: cmp(a.id, b.id)
        }
        return list(
            rows.take(limitOf(params)).map { b ->
                obj(
                    "id" to JsonValue.Str(b.id),
                    "slug" to JsonValue.Str(b.slug),
                    "name" to JsonValue.Str(b.name),
                    "prefix" to JsonValue.Str(b.prefix),
                    "icon" to str(b.icon),
                    "color" to str(b.color),
                    "value" to JsonValue.Str(b.id),
                    "label" to JsonValue.Str(b.name),
                )
            },
            rows.size,
        )
    }

    /** VAPP-206: the viewer, so a package's `visible` reads their team role and app access (null = write). */
    fun meValue(store: Store, me: String?, access: String?): JsonValue {
        val user = store.users.firstOrNull { it.id == me }
        return obj(
            "id" to str(me),
            "name" to JsonValue.Str(user?.name ?: ""),
            "email" to str(user?.email),
            "avatar" to str(user?.image),
            "role" to str(store.members.firstOrNull { it.userId == me }?.role),
            "access" to JsonValue.Str(access ?: "write"),
        )
    }

    fun membersValue(store: Store, params: Map<String, String>, me: String?): JsonValue {
        val users = store.users.associateBy { it.id }
        val rows = store.members.map { m ->
            val u = users[m.userId]
            Triple(m, u, u?.name ?: "")
        }.sortedWith { a, b -> cmp(a.third, b.third).takeIf { it != 0 } ?: cmp(a.first.userId, b.first.userId) }
        return list(
            rows.take(limitOf(params)).map { (m, u, name) ->
                obj(
                    "id" to JsonValue.Str(m.userId),
                    "name" to JsonValue.Str(name),
                    "email" to str(u?.email),
                    "avatar" to str(u?.image),
                    "role" to JsonValue.Str(m.role),
                    "value" to JsonValue.Str(m.userId),
                    "label" to JsonValue.Str(name),
                )
            },
            rows.size,
            "me" to str(me),
        )
    }

    fun sessionsValue(store: Store, params: Map<String, String>, me: String?): JsonValue {
        val status = params["status"]?.takeIf { it.isNotEmpty() }
        val mine = params["mine"] == "1" || params["mine"] == "true"
        val rows = store.sessions
            .filter { it.parentId == null && it.appId == null }
            .filter { status == null || it.status == status }
            .filter { !mine || (me != null && it.userId == me) }
            .sortedWith { a, b -> (-cmp(a.updatedAt, b.updatedAt)).takeIf { it != 0 } ?: cmp(a.id, b.id) }
        return list(
            rows.take(limitOf(params)).map { s ->
                obj(
                    "id" to JsonValue.Str(s.id),
                    "title" to JsonValue.Str(s.title ?: ""),
                    "status" to JsonValue.Str(s.status),
                    "busy" to JsonValue.Bool(s.busy),
                    "needsInput" to JsonValue.Bool(s.needsInput),
                    "kind" to JsonValue.Str(if (!s.agentId.isNullOrEmpty()) "agent" else "thread"),
                    "updatedAt" to JsonValue.Str(s.updatedAt),
                )
            },
            rows.size,
        )
    }

    fun commentsValue(store: Store, params: Map<String, String>): JsonValue? {
        val issue = issueOf(store, params["issue"]) ?: return null
        val rows = store.comments.filter { it.issueId == issue.id }
            .sortedWith { a, b -> cmp(a.createdAt, b.createdAt).takeIf { it != 0 } ?: cmp(a.id, b.id) }
        return list(
            rows.take(limitOf(params)).map { c ->
                obj(
                    "id" to JsonValue.Str(c.id),
                    "body" to JsonValue.Str(c.body),
                    "author" to str(c.authorId),
                    "authorName" to JsonValue.Str(userName(store, c.authorId)),
                    "parent" to str(c.parentId),
                    "source" to JsonValue.Str(c.source),
                    "createdAt" to JsonValue.Str(c.createdAt),
                )
            },
            rows.size,
        )
    }

    fun attachmentsValue(store: Store, params: Map<String, String>): JsonValue? {
        val issue = issueOf(store, params["issue"]) ?: return null
        val rows = store.attachments.filter { it.issueId == issue.id }
            .sortedWith { a, b -> cmp(a.createdAt, b.createdAt).takeIf { it != 0 } ?: cmp(a.id, b.id) }
        return list(
            rows.take(limitOf(params)).map { a ->
                obj(
                    "id" to JsonValue.Str(a.id),
                    "name" to JsonValue.Str(a.filename),
                    "type" to JsonValue.Str(a.contentType),
                    "size" to JsonValue.Num(a.sizeBytes.toDouble()),
                    "url" to JsonValue.Str("/api/attachments/${a.id}"),
                    "width" to num(a.width),
                    "height" to num(a.height),
                    "uploader" to str(a.uploaderId),
                    "uploaderName" to JsonValue.Str(userName(store, a.uploaderId)),
                    "createdAt" to JsonValue.Str(a.createdAt),
                )
            },
            rows.size,
        )
    }

    /** One `exp:<name>?params` over [store]: the value the host emits at the
     *  bound path (null for a name this module does not derive, or a comments /
     *  attachments source without a resolvable issue). [me] = the viewer;
     *  [timeZone] = the team zone date-only `since` / `until` read in (FEED-243;
     *  null / unknown = the device's). */
    fun sourceValue(
        name: String,
        params: Map<String, String>,
        store: Store,
        me: String?,
        access: String? = null,
        timeZone: String? = null,
    ): JsonValue? = when (name) {
        "me" -> meValue(store, me, access)
        "issues" -> issuesValue(store, params, me, timeZone)
        "boards" -> boardsValue(store, params)
        "members" -> membersValue(store, params, me)
        "sessions" -> sessionsValue(store, params, me)
        "comments" -> commentsValue(store, params)
        "attachments" -> attachmentsValue(store, params)
        else -> null
    }

    // The synced Room rows → the reference's rows (timestamps in the web
    // host's ISO form, `AppRecords.isoTimestamp`).
    private fun iso(raw: String): String = AppRecords.isoTimestamp(raw)

    fun board(e: BoardEntity) = Board(
        id = e.id, slug = e.slug, name = e.name, prefix = e.prefix, icon = e.icon, color = e.color,
        sortOrder = e.sortOrder, deletedAt = e.deletedAt?.let(::iso),
    )

    /** [IssueEntity.status] is the anchor column (the dual-written enum). */
    fun issue(e: IssueEntity) = Issue(
        id = e.id, boardId = e.boardId, identifier = e.identifier, title = e.title, status = e.status,
        priority = e.priority, assigneeId = e.assigneeId, creatorId = e.creatorId, dueDate = e.dueDate,
        sortOrder = e.sortOrder, createdAt = iso(e.createdAt), updatedAt = iso(e.updatedAt),
    )

    fun member(e: TeamMemberEntity) = Member(userId = e.userId, role = e.role)

    fun user(e: UserEntity) = User(id = e.id, name = e.name.orEmpty(), email = e.email, image = e.image)

    fun session(e: SessionEntity) = Session(
        id = e.id, parentId = e.parentId, agentId = e.agentId, appId = e.appId, userId = e.userId.takeIf { it.isNotEmpty() },
        status = e.status, busy = e.busy, needsInput = e.needsInput, title = e.title,
        createdAt = iso(e.createdAt), updatedAt = iso(e.updatedAt),
    )

    fun comment(e: CommentEntity) = Comment(
        id = e.id, issueId = e.issueId, authorId = e.authorId, parentId = e.parentId,
        source = e.source ?: DomainContract.commentSourceUser, body = e.body.orEmpty(), createdAt = iso(e.createdAt),
    )

    fun issueEvent(e: IssueEventEntity) = IssueEvent(
        id = e.id, issueId = e.issueId, actorUserId = e.actorUserId?.takeIf { it.isNotEmpty() }, createdAt = iso(e.createdAt),
    )

    fun sessionIssue(e: SessionIssueEntity) = SessionIssue(sessionId = e.sessionId, issueId = e.issueId)

    fun attachment(e: AttachmentEntity) = Attachment(
        id = e.id, issueId = e.issueId, commentId = e.commentId, uploaderId = e.uploaderId, filename = e.filename,
        contentType = e.contentType, sizeBytes = e.sizeBytes, width = e.width, height = e.height, createdAt = iso(e.createdAt),
    )
}
