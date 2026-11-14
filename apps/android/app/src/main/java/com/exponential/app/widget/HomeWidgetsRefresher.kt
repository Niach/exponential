package com.exponential.app.widget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.util.TypedValue
import android.view.View
import android.widget.RemoteViews
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import com.exponential.app.MainActivity
import com.exponential.app.R
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.Chats
import com.exponential.app.domain.HomeWidgets
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WidgetLinks
import com.exponential.app.ui.chats.needsYouFold
import com.exponential.app.ui.components.boardIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.resolvedStatusColor
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * #EXP-1830 (home-widgets.json): the three home widgets' data and drawing.
 * Reads the ACTIVE account's database in-process, debounced behind the sync's
 * bursts (like DirectShareShortcuts), and redraws every placed widget; a
 * provider callback ([draw]) redraws from the last data. The look is iOS
 * HomeWidgetViews': the card container, a header (glyph, title, muted count),
 * one-line rows `look.rowHeight` high, or one muted centred line.
 */
@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@Singleton
class HomeWidgetsRefresher @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** One status as a widget row draws it. */
    private data class StatusLook(val status: HomeWidgets.Status, val icon: String)

    /** Everything the three widgets read; null `me` = signed out. */
    private data class Data(
        val me: String?,
        val teams: List<TeamEntity>,
        val boards: List<BoardEntity>,
        val statuses: Map<String, StatusLook>,
        val issues: List<HomeWidgets.Issue>,
        val needsYou: List<HomeWidgets.NeedsYouInput>,
        val sealedRoots: Set<String>,
    )

    private val data = MutableStateFlow<Data?>(null)

    private val minuteClock: Flow<Long> = flow {
        while (true) {
            emit(System.currentTimeMillis())
            delay(60_000)
        }
    }

    fun start() {
        scope.launch {
            val sessions = dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeAll() }
            val issueInputs = combine(
                dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
                dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
                dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) { it.issueStatusDao().observeAll() },
                dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() },
            ) { issues, boards, statuses, teams -> IssueInputs(issues, boards, statuses, teams) }
            val chatInputs = combine(
                sessions,
                dbFlow.scopedQuery(emptyList<AgentDefinitionEntity>()) { it.agentDefinitionDao().observeAll() },
                dbFlow.scopedQuery(emptyList<PullRequestEntity>()) { it.pullRequestDao().observeAll() },
            ) { s, agents, prs -> ChatInputs(s, agents, prs) }
            combine(
                issueInputs,
                chatInputs,
                combine(auth.userId, auth.token.map { it != null }.distinctUntilChanged()) { me, signedIn -> me.takeIf { signedIn } },
                minuteClock,
            ) { issues, chats, me, now -> build(issues, chats, me, now) }
                .debounce(DEBOUNCE_MS)
                .distinctUntilChanged()
                .collect {
                    data.value = it
                    drawAll()
                }
        }
    }

    private data class IssueInputs(
        val issues: List<IssueEntity>,
        val boards: List<BoardEntity>,
        val statuses: List<IssueStatusEntity>,
        val teams: List<TeamEntity>,
    )

    private data class ChatInputs(
        val sessions: List<SessionEntity>,
        val agents: List<AgentDefinitionEntity>,
        val prs: List<PullRequestEntity>,
    )

    private fun build(issueInputs: IssueInputs, chats: ChatInputs, me: String?, now: Long): Data {
        if (me == null) return Data(null, emptyList(), emptyList(), emptyMap(), emptyList(), emptyList(), emptySet())
        val boards = issueInputs.boards.filter { it.deletedAt == null }
        val boardById = boards.associateBy { it.id }
        // Each team's statuses resolved like every issue row (glyph + colour).
        val resolvedByTeam: Map<String, List<ResolvedIssueStatus>> = issueInputs.teams.associate { team ->
            team.id to IssueStatusResolver.teamStatuses(issueInputs.statuses.filter { it.teamId == team.id })
                .ifEmpty { IssueStatusResolver.builtinDefaults }
        }
        val statuses = HashMap<String, StatusLook>()
        resolvedByTeam.values.flatten().forEach { status ->
            statuses[status.id] = StatusLook(
                HomeWidgets.Status(status.id, status.category.wire, hex(resolvedStatusColor(status)), 0.0),
                status.iconName,
            )
        }
        // The status sortOrder: the resolver's display order within a category.
        resolvedByTeam.values.forEach { list ->
            list.forEachIndexed { index, status ->
                statuses[status.id]?.let { statuses[status.id] = it.copy(status = it.status.copy(sortOrder = index.toDouble())) }
            }
        }
        val issues = issueInputs.issues.mapNotNull { issue ->
            val board = boardById[issue.boardId] ?: return@mapNotNull null
            val team = resolvedByTeam[board.teamId].orEmpty()
            val statusId = issue.statusId?.takeIf { id -> team.any { it.id == id } }
                ?: team.firstOrNull { it.builtinKey?.wire == issue.status }?.id
                ?: return@mapNotNull null
            HomeWidgets.Issue(
                id = issue.id,
                identifier = issue.identifier,
                title = issue.title,
                statusId = statusId,
                boardId = issue.boardId,
                teamId = board.teamId,
                priority = issue.priority,
                sortOrder = issue.sortOrder,
                assigneeId = issue.assigneeId,
                updatedAt = issue.updatedAt,
            )
        }
        // THE Needs you rows (the Chats screen's fold), every team.
        val sessionRows = chats.sessions.map(SessionTree::session)
        val messages = chats.sessions.flatMap { row -> Chats.digestMessages(row.id, Chats.parseDigest(row.digest)) }
        val rows = needsYouFold(
            sessionRows, emptyList(), emptyList(), messages,
            chats.agents.map(SessionTree::agentDefinition), chats.prs, null, me, now,
            teamIds = issueInputs.teams.map { it.id },
        )
        val needsYou = rows.map {
            HomeWidgets.NeedsYouInput(it.messageId, it.sessionId, it.rootId, it.kind.wire, it.opens.wire, it.title, it.age)
        }
        val sealedRoots = chats.sessions.filter { it.sealPublicKey != null || it.sealedTitle != null }.mapTo(HashSet()) { it.id }
        return Data(me, issueInputs.teams, boards, statuses, issues, needsYou, sealedRoots)
    }

    /** Redraw every placed widget of every kind. */
    private fun drawAll() {
        val manager = AppWidgetManager.getInstance(context)
        HomeWidgetKind.entries.forEach { kind ->
            val ids = manager.getAppWidgetIds(ComponentName(context, kind.providerClass()))
            if (ids.isNotEmpty()) draw(kind, ids)
        }
    }

    /** Draw [ids] of [kind] from the last data (a provider callback, a pick). */
    fun draw(kind: HomeWidgetKind, ids: IntArray) {
        val manager = AppWidgetManager.getInstance(context)
        val current = data.value
        ids.forEach { id ->
            val views = runCatching { views(kind, id, current, manager) }.getOrNull() ?: return@forEach
            manager.updateAppWidget(id, views)
        }
    }

    // ---- content (iOS HomeWidgetContent) ----

    private data class Row(
        val key: String,
        val glyph: String,
        val tint: Color,
        val title: String,
        val trailing: String,
        val url: String,
    )

    private data class Content(
        val glyph: androidx.compose.ui.graphics.vector.ImageVector?,
        val glyphTint: Color,
        val title: String,
        val count: Int,
        val rows: List<Row>,
        val empty: String,
        val url: String,
    )

    private fun content(kind: HomeWidgetKind, id: Int, data: Data?, max: Int): Content {
        val signedOut = Content(null, Color.Transparent, "", 0, emptyList(), HomeWidgets.SIGNED_OUT, WidgetLinks.HOME)
        val me = data?.me ?: return signedOut
        return when (kind) {
            HomeWidgetKind.Board -> {
                val picked = HomeWidgetPrefs.board(context, id)
                val ordered = orderedBoards(data)
                val board = if (picked != null) data.boards.firstOrNull { it.id == picked } else ordered.firstOrNull()
                if (board == null) {
                    Content(null, Color.Transparent, HomeWidgets.BOARD_GALLERY, 0, emptyList(), HomeWidgets.EMPTY_ISSUES, WidgetLinks.HOME)
                } else {
                    val list = HomeWidgets.boardWidget(board.id, data.statuses.values.map { it.status }, data.issues, max)
                    Content(
                        boardIcon(board),
                        runCatching { parseColor(board.color) }.getOrDefault(DesignTokens.Palette.MutedForeground),
                        board.name,
                        list.count,
                        issueRows(list.rows, data),
                        HomeWidgets.EMPTY_ISSUES,
                        WidgetLinks.board(board.id),
                    )
                }
            }
            HomeWidgetKind.MyIssues -> {
                val list = HomeWidgets.myIssuesWidget(
                    me, HomeWidgetPrefs.teams(context, id), data.statuses.values.map { it.status }, data.issues, max,
                )
                Content(
                    ExpIcons.byName(HomeWidgets.ICON_MY_ISSUES),
                    DesignTokens.Palette.Foreground,
                    HomeWidgets.MY_ISSUES,
                    list.count,
                    issueRows(list.rows, data),
                    HomeWidgets.EMPTY_ISSUES,
                    WidgetLinks.MY_ISSUES,
                )
            }
            HomeWidgetKind.NeedsYou -> {
                val list = HomeWidgets.needsYouWidget(data.needsYou, data.sealedRoots, max)
                Content(
                    ExpIcons.byName(HomeWidgets.ICON_NEEDS_YOU),
                    DesignTokens.Semantic.Yellow,
                    HomeWidgets.NEEDS_YOU,
                    list.count,
                    list.rows.map { row ->
                        val opens = com.exponential.app.domain.NeedsYou.Opens.entries.firstOrNull { it.wire == row.opens }
                            ?: com.exponential.app.domain.NeedsYou.Opens.Chat
                        Row(row.messageId, row.icon, DesignTokens.Semantic.Yellow, row.title, row.age, WidgetLinks.needsYouRow(row.rootId, row.messageId, opens))
                    },
                    HomeWidgets.EMPTY_NEEDS_YOU,
                    WidgetLinks.NEEDS_YOU,
                )
            }
        }
    }

    /** Sidebar order: team order (name), then the board's sortOrder, then name. */
    private fun orderedBoards(data: Data): List<BoardEntity> {
        val teamRank = data.teams.sortedBy { it.name.lowercase() }.withIndex().associate { (i, t) -> t.id to i }
        return data.boards.sortedWith(
            compareBy<BoardEntity> { teamRank[it.teamId] ?: Int.MAX_VALUE }.thenBy { it.sortOrder }.thenBy { it.name },
        )
    }

    private fun issueRows(rows: List<HomeWidgets.IssueRow>, data: Data): List<Row> {
        val statusOf = data.issues.associate { it.id to it.statusId }
        return rows.map { row ->
            Row(
                row.id,
                statusOf[row.id]?.let { data.statuses[it]?.icon }.orEmpty(),
                runCatching { parseColor(row.statusColor) }.getOrDefault(DesignTokens.Palette.MutedForeground),
                row.title,
                row.identifier,
                WidgetLinks.issue(row.id),
            )
        }
    }

    // ---- drawing (iOS HomeWidgetView) ----

    private fun views(kind: HomeWidgetKind, id: Int, data: Data?, manager: AppWidgetManager): RemoteViews {
        val options = manager.getAppWidgetOptions(id)
        val maxHeight = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, 0)
        val max = if (maxHeight >= LARGE_MIN_HEIGHT_DP) HomeWidgets.CAPACITY_LARGE else HomeWidgets.CAPACITY_MEDIUM
        val content = content(kind, id, data, max)
        val root = RemoteViews(context.packageName, R.layout.home_widget)
        root.setOnClickPendingIntent(R.id.home_widget_root, pending(id, -1, content.url))
        root.removeAllViews(R.id.home_widget_rows)
        if (content.title.isEmpty()) {
            root.setViewVisibility(R.id.home_widget_header, View.GONE)
            root.setViewVisibility(R.id.home_widget_rows, View.GONE)
            root.setViewVisibility(R.id.home_widget_empty, View.VISIBLE)
            root.setTextViewText(R.id.home_widget_empty, content.empty)
            return root
        }
        root.setViewVisibility(R.id.home_widget_header, View.VISIBLE)
        root.setOnClickPendingIntent(R.id.home_widget_header, pending(id, -1, content.url))
        root.setTextViewText(R.id.home_widget_title, content.title)
        root.setTextViewText(R.id.home_widget_count, if (content.count > 0) "${content.count}" else "")
        val glyphPx = px(HomeWidgets.LOOK_GLYPH)
        content.glyph?.let { root.setImageViewBitmap(R.id.home_widget_glyph, WidgetGlyphs.bitmap(it, content.glyphTint, glyphPx)) }
        root.setViewVisibility(R.id.home_widget_glyph, if (content.glyph != null) View.VISIBLE else View.INVISIBLE)
        if (content.rows.isEmpty()) {
            root.setViewVisibility(R.id.home_widget_rows, View.GONE)
            root.setViewVisibility(R.id.home_widget_empty, View.VISIBLE)
            root.setTextViewText(R.id.home_widget_empty, content.empty)
            return root
        }
        root.setViewVisibility(R.id.home_widget_empty, View.GONE)
        root.setViewVisibility(R.id.home_widget_rows, View.VISIBLE)
        content.rows.forEachIndexed { index, row ->
            val rowViews = RemoteViews(context.packageName, R.layout.home_widget_row)
            ExpIcons.byName(row.glyph)?.let {
                rowViews.setImageViewBitmap(R.id.home_widget_row_glyph, WidgetGlyphs.bitmap(it, row.tint, glyphPx))
            }
            rowViews.setTextViewText(R.id.home_widget_row_title, row.title)
            rowViews.setTextViewText(R.id.home_widget_row_trailing, row.trailing)
            rowViews.setOnClickPendingIntent(R.id.home_widget_row, pending(id, index, row.url))
            root.addView(R.id.home_widget_rows, rowViews)
        }
        return root
    }

    /** A tap = MainActivity on [url] by THE back stack rule; one request code per widget row. */
    private fun pending(widgetId: Int, row: Int, url: String): PendingIntent = PendingIntent.getActivity(
        context,
        widgetId * 100 + row + 1,
        MainActivity.widgetIntent(context, url),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    private fun px(dp: Int): Int =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, dp.toFloat(), context.resources.displayMetrics).toInt()

    private fun hex(color: Color): String = String.format("#%06X", color.toArgb() and 0xFFFFFF)

    private companion object {
        const val DEBOUNCE_MS = 1_500L
        /** A 4x4 widget (home-widgets.json `capacity.large`). */
        const val LARGE_MIN_HEIGHT_DP = 250
    }
}
