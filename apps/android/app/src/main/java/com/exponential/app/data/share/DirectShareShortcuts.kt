package com.exponential.app.data.share

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.VectorGroup
import androidx.compose.ui.graphics.vector.VectorPath
import androidx.compose.ui.graphics.vector.toPath
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.core.app.Person
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import com.exponential.app.MainActivity
import com.exponential.app.R
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.PinEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AppShortcuts
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionTitle
import com.exponential.app.domain.ShareTargets
import com.exponential.app.ui.icons.ExpIcons
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * EXP-1635 (share-targets.json DIRECT SHARE): the OS-drawn rows of the system
 * share sheet. The active team's [ShareTargets.directShareEntries] as long-lived
 * sharing shortcuts (`res/xml/shortcuts.xml`); a share on one arrives at
 * MainActivity with `EXTRA_SHORTCUT_ID`. Debounced behind the sync's bursts, and
 * a row is only re-pushed when it changed.
 *
 * #EXP-1638 (app-shortcuts.json): beside them, the long-press menu's recent
 * chat ([AppShortcuts.appShortcuts] past the statics, which live in the same
 * XML) as a plain dynamic shortcut at rank 0, the sharing ones ranked after.
 * The same store and filter (never an agent's conversation, never an
 * encrypted chat), so it follows the active account and team; signed out =
 * none. A tap launches MainActivity with [MainActivity.EXTRA_APP_SHORTCUT].
 */
@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@Singleton
class DirectShareShortcuts @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val selection: TeamSelection,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** [recent] = a long-press chat entry (#EXP-1830: the Assistant, an
     *  agent, a pinned or recent chat), not a sharing shortcut. */
    private data class Row(
        val entry: ShareTargets.Entry,
        val rank: Int,
        val agent: SessionTree.AgentRow?,
        val recent: Boolean = false,
    )

    /** #EXP-1830: the long-press chat entries' inputs (pins in sidebar order). */
    private data class ShortcutChats(val pins: List<String>, val sessions: List<SessionEntity>)

    fun start() {
        scope.launch {
            val chats = selection.selectedId.flatMapLatest { team ->
                if (team == null) {
                    flowOf(emptyList())
                } else {
                    dbFlow.scopedQuery(emptyList<SessionEntity>()) {
                        it.sessionDao().observeRecentChats(team, ShareTargets.MAX_ENTRIES)
                    }
                }
            }
            val shortcutChats = combine(selection.selectedId, auth.userId) { team, me -> team to me }
                .distinctUntilChanged()
                .flatMapLatest { (team, me) ->
                    if (team == null || me == null) {
                        flowOf(ShortcutChats(emptyList(), emptyList()))
                    } else {
                        dbFlow.scopedQuery(emptyList<PinEntity>()) { it.pinDao().observeSessionPins(team) }
                            .map { pins -> pins.mapNotNull { it.sessionId } }
                            .distinctUntilChanged()
                            .flatMapLatest { pins ->
                                dbFlow.scopedQuery(emptyList<SessionEntity>()) {
                                    it.sessionDao().observeShortcutChats(team, me, pins, SHORTCUT_CHAT_LIMIT)
                                }.map { ShortcutChats(pins, it) }
                            }
                    }
                }
            combine(
                dbFlow.scopedQuery(emptyList<AgentDefinitionEntity>()) { it.agentDefinitionDao().observeAll() },
                chats,
                shortcutChats,
                selection.selectedId,
                combine(auth.userId, auth.token.map { it != null }.distinctUntilChanged()) { me, signedIn -> me to signedIn },
            ) { agents, sessions, shortcut, team, (me, signedIn) -> rows(agents, sessions, shortcut, team, me, signedIn) }
                .debounce(DEBOUNCE_MS)
                .distinctUntilChanged()
                .collect(::publish)
        }
    }

    private fun rows(
        agents: List<AgentDefinitionEntity>,
        sessions: List<SessionEntity>,
        shortcut: ShortcutChats,
        team: String?,
        me: String?,
        signedIn: Boolean,
    ): List<Row> {
        if (team == null) return emptyList()
        val listed = SessionTree.listAgents(agents.map(SessionTree::agentDefinition), emptyList(), team, me)
        val order = listed.withIndex().associate { (index, row) -> row.agentId to index }
        val chats = sessions.map(::chat)
        val entries = ShareTargets.directShareEntries(listed, chats, me) { a ->
            ShareTargets.Agent(
                id = a.agentId,
                name = a.name,
                sortOrder = order.getValue(a.agentId),
                role = if (a.assistant) DomainContract.agentRoleAssistant else DomainContract.agentRoleAgent,
                ownerId = if (a.assistant) me else null,
            )
        }
        val byId = listed.associateBy { "${ShareTargets.AGENT_PREFIX}${it.agentId}" }
        // #EXP-1830 (app-shortcuts.json): the chat entries past the statics.
        val recent = if (signedIn) {
            val assistant = listed.firstOrNull { it.assistant }
            val input = AppShortcuts.Input(
                assistant = assistant?.let { AppShortcuts.Assistant(it.agentId, it.name) },
                agents = listed.map { AppShortcuts.Agent(it.agentId, it.name, it.icon.ifEmpty { null }, it.color.ifEmpty { null }) },
                pins = shortcut.pins,
                chats = shortcut.sessions.map(::chat),
            )
            val agentById = listed.associateBy { "${AppShortcuts.AGENT_PREFIX}${it.agentId}" }
            AppShortcuts.appShortcuts(input)
                .filter { it.id !in AppShortcuts.STATICS }
                .mapIndexed { rank, it -> Row(ShareTargets.Entry(it.id, it.title), rank, agentById[it.id], recent = true) }
        } else {
            emptyList()
        }
        return recent + entries.mapIndexed { rank, entry -> Row(entry, recent.size + rank, byId[entry.id]) }
    }

    private fun chat(it: SessionEntity) = ShareTargets.Chat(
        it.id,
        it.title?.trim()?.takeIf(String::isNotEmpty) ?: SessionTitle.UNTITLED,
        it.updatedAt,
        it.agentId,
        it.sealPublicKey != null || it.sealedTitle != null,
    )

    /** This process's last pushes, for what the shortcut service cannot report back (an agent's look). */
    private var published: Map<String, Row> = emptyMap()

    private fun publish(rows: List<Row>) {
        runCatching {
            val next = rows.associateBy { it.entry.id }
            val live = ShortcutManagerCompat.getDynamicShortcuts(context).associateBy { it.id }
            val stale = live.keys.filter { it !in next }
            if (stale.isNotEmpty()) ShortcutManagerCompat.removeLongLivedShortcuts(context, stale)
            // #EXP-1638: a recent chat is a plain dynamic shortcut, not long-lived.
            val staleRecent = stale.filter {
                it.startsWith(AppShortcuts.RECENT_CHAT_PREFIX) || it.startsWith(AppShortcuts.AGENT_PREFIX)
            }
            if (staleRecent.isNotEmpty()) ShortcutManagerCompat.removeDynamicShortcuts(context, staleRecent)
            // #EXP-1830: once per intent version every row is re-pushed, so a
            // shortcut already pinned to the home screen gets the launcher
            // intent that names its chat (pushDynamicShortcut updates pinned
            // ones in place).
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val repushAll = prefs.getInt(PREF_INTENT_VERSION, 0) < INTENT_VERSION
            rows.filter { row ->
                val held = live[row.entry.id]
                repushAll || held == null || held.shortLabel != row.entry.title || held.rank != row.rank ||
                    published[row.entry.id]?.let { it != row } == true
            }.forEach { row -> ShortcutManagerCompat.pushDynamicShortcut(context, shortcut(row)) }
            if (repushAll && rows.isNotEmpty()) prefs.edit().putInt(PREF_INTENT_VERSION, INTENT_VERSION).apply()
            published = next
        }
    }

    private fun shortcut(row: Row): ShortcutInfoCompat {
        if (row.recent) return recentChatShortcut(row)
        val icon = row.agent?.takeUnless { it.assistant }?.let(::agentIcon)
            ?: IconCompat.createWithResource(context, R.mipmap.ic_launcher)
        val person = Person.Builder()
            .setKey(row.entry.id)
            .setName(row.entry.title)
            .setIcon(icon)
            .setBot(row.agent != null)
            .build()
        return ShortcutInfoCompat.Builder(context, row.entry.id)
            .setShortLabel(row.entry.title)
            .setLongLived(true)
            .setPerson(person)
            .setIcon(icon)
            .setRank(row.rank)
            .setCategories(setOf(CATEGORY))
            // #EXP-1830: tapped from a launcher (a pinned one on the home
            // screen too), not the share sheet: ITS chat (launcherShortcutId).
            .setIntent(launcherIntent(AppShortcuts.launcherShortcutId(row.entry.id)))
            .build()
    }

    /** #EXP-1638 / #EXP-1830: a chat entry on the long-press menu (the
     *  Assistant's brand mark, an agent's glyph in its colour, a chat's glyph),
     *  never in the share sheet. */
    private fun recentChatShortcut(row: Row): ShortcutInfoCompat {
        val icon = when {
            row.agent?.assistant == true -> IconCompat.createWithResource(context, R.mipmap.ic_launcher)
            row.agent != null -> agentIcon(row.agent) ?: IconCompat.createWithResource(context, R.drawable.ic_shortcut_chat)
            else -> IconCompat.createWithResource(context, R.drawable.ic_shortcut_chat)
        }
        return ShortcutInfoCompat.Builder(context, row.entry.id)
            .setShortLabel(row.entry.title)
            .setIcon(icon)
            .setRank(row.rank)
            .setIntent(launcherIntent(row.entry.id))
            .build()
    }

    /** MainActivity on [shortcutId] (null = just the app). */
    private fun launcherIntent(shortcutId: String?): Intent =
        Intent(context, MainActivity::class.java)
            .setAction(Intent.ACTION_VIEW)
            .apply { if (shortcutId != null) putExtra(MainActivity.EXTRA_APP_SHORTCUT, shortcutId) }

    /** The agent's glyph in its colour on the bubble's fill, an adaptive bitmap. */
    private fun agentIcon(agent: SessionTree.AgentRow): IconCompat? = runCatching {
        val vector = ExpIcons.byName(agent.icon)
            ?: ExpIcons.byName(SessionTree.ListCopy.AGENT_DEFAULT_ICON)
            ?: return null
        val tint = Color(android.graphics.Color.parseColor(agent.color))
        val bitmap = Bitmap.createBitmap(ICON_PX, ICON_PX, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap.asImageBitmap())
        val size = Size(ICON_PX.toFloat(), ICON_PX.toFloat())
        CanvasDrawScope().draw(Density(1f), LayoutDirection.Ltr, canvas, size) {
            drawRect(ICON_FILL)
            val glyph = ICON_PX * GLYPH_FRACTION
            val inset = (ICON_PX - glyph) / 2
            val factor = glyph / vector.viewportWidth
            translate(inset, inset) {
                scale(factor, factor, pivot = androidx.compose.ui.geometry.Offset.Zero) { drawGroup(vector.root, tint) }
            }
        }
        IconCompat.createWithAdaptiveBitmap(bitmap)
    }.getOrNull()

    /** A generated icon's paths, in [tint] (the shortcut service takes a bitmap). */
    private fun DrawScope.drawGroup(group: VectorGroup, tint: Color) {
        for (node in group) {
            when (node) {
                is VectorGroup -> drawGroup(node, tint)
                is VectorPath -> {
                    val path = node.pathData.toPath()
                    if (node.fill != null) drawPath(path, SolidColor(tint))
                    if (node.stroke != null) {
                        drawPath(path, SolidColor(tint), style = Stroke(node.strokeLineWidth, cap = node.strokeLineCap, join = node.strokeLineJoin))
                    }
                }
            }
        }
    }

    private companion object {
        const val CATEGORY = "com.exponential.app.category.SHARE_TARGET"
        const val DEBOUNCE_MS = 2_000L
        /** The long-press candidates read per team (pins ride along). */
        const val SHORTCUT_CHAT_LIMIT = 12
        const val PREFS = "direct_share_shortcuts"
        const val PREF_INTENT_VERSION = "intent_version"
        /** #EXP-1830: bump to re-push every shortcut (a pinned one's intent changed). */
        const val INTENT_VERSION = 1
        const val ICON_PX = 216
        const val GLYPH_FRACTION = 0.4f
        val ICON_FILL = Color(0xFF27272A)
    }
}
