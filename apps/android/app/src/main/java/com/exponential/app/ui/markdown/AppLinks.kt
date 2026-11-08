package com.exponential.app.ui.markdown

import android.content.Context
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.TextLinkStyles
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.host.TextLinkSegment
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.domain.AppLink
import com.exponential.app.domain.ProseLinks
import com.exponential.app.domain.cardLinkOpens
import com.exponential.app.domain.classifyAppLink
import com.exponential.app.domain.expandTeamLink
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.net.URI
import kotlinx.coroutines.flow.first

/**
 * EXP-1188: links in agent prose (narration, Guide, chat) that point at THIS
 * instance open in the app. [origin] = the active account's instance origin
 * (`https://host[:port]`, no trailing slash); [team] = the slug `~/` links
 * expand against (#EXP-1674: the surface's team, else the active team's);
 * [onOpen] opens a classified link, given its ABSOLUTE url.
 */
class AppLinkHandler(
    val origin: String,
    val team: String? = null,
    /** The id of the team [team] names, null while unknown. */
    val teamId: String? = null,
    val onOpen: (link: AppLink, url: String) -> Unit,
)

/**
 * Provided by the agent surfaces only (the session screen, the chat screen);
 * null (the default) keeps every markdown link a plain browser link, as issue
 * descriptions and comments have always rendered.
 */
val LocalAppLinks = compositionLocalOf<AppLinkHandler?> { null }

/**
 * The annotation a link [href] gets under [handler] (`fixtures/app-link.json`):
 * an in-app link = a click that navigates, another instance page = the absolute
 * instance URL, external = the href as written, ignore = none. With no handler
 * the href opens as written, as before EXP-1188.
 */
internal fun appLinkAnnotation(
    href: String,
    handler: AppLinkHandler?,
    styles: TextLinkStyles,
): LinkAnnotation? {
    if (handler == null) {
        // #EXP-1674: `~/x` means nothing to a browser (and no app can open it).
        if (href.trim().startsWith("~")) return null
        return LinkAnnotation.Url(url = href, styles = styles)
    }
    return when (val link = classifyAppLink(href, handler.origin, handler.team)) {
        is AppLink.Issue, is AppLink.IssueRef, is AppLink.Board, is AppLink.Session, is AppLink.Plan,
        is AppLink.Chats, is AppLink.Agent, is AppLink.AgentHome, is AppLink.Inbox,
        is AppLink.Devices, is AppLink.Settings, is AppLink.AppHome, is AppLink.AppScreen,
        // EXP-1188: another instance page and a web source open in the
        // in-app browser (a Custom Tab), like iOS's Safari sheet; mailto
        // keeps the system handler.
        is AppLink.App -> {
            val url = linkUrl(href, link, handler)
            LinkAnnotation.Clickable(
                tag = url,
                styles = styles,
                linkInteractionListener = { handler.onOpen(link, url) },
            )
        }
        is AppLink.External ->
            if (link.url.startsWith("mailto:", ignoreCase = true)) {
                LinkAnnotation.Url(url = link.url, styles = styles)
            } else {
                LinkAnnotation.Clickable(
                    tag = link.url,
                    styles = styles,
                    linkInteractionListener = { handler.onOpen(link, link.url) },
                )
            }
        AppLink.Ignore -> null
    }
}

/** Opens [href] as a tapped link would (an app's `openUrl`, say). */
fun AppLinkHandler.openHref(href: String) {
    val link = classifyAppLink(href, origin, team)
    if (link != AppLink.Ignore) onOpen(link, linkUrl(href, link, this))
}

/** #EXP-1674 (plan-card.json): a chat card's link: in-app and https only. */
fun AppLinkHandler.openCardHref(href: String) {
    if (cardLinkOpens(href, origin, team)) openHref(href)
}

/** The absolute URL of a classified [href]. */
private fun linkUrl(href: String, link: AppLink, handler: AppLinkHandler): String {
    if (link is AppLink.External) return link.url
    if (link is AppLink.App) return handler.origin + link.path
    val trimmed = href.trim()
    val path = if (trimmed.startsWith("~")) expandTeamLink(trimmed, handler.team) else trimmed.takeIf { it.startsWith("/") }
    return if (path != null) handler.origin + path else trimmed
}

/** `https://host[:port]` of an instance URL, or null when it does not parse. */
internal fun instanceOrigin(instanceUrl: String?): String? {
    val uri = instanceUrl?.trim()?.let { runCatching { URI(it) }.getOrNull() } ?: return null
    val scheme = uri.scheme ?: return null
    val host = uri.host ?: return null
    return if (uri.port >= 0) "$scheme://$host:${uri.port}" else "$scheme://$host"
}

@EntryPoint
@InstallIn(SingletonComponent::class)
interface AppLinksEntryPoint {
    fun authRepository(): AuthRepository
    fun deepLinkBus(): DeepLinkBus
    fun teamSelection(): TeamSelection
    fun databaseHolder(): DatabaseHolder
}

private fun appLinksEntryPoint(context: Context): AppLinksEntryPoint =
    EntryPointAccessors.fromApplication(context.applicationContext, AppLinksEntryPoint::class.java)

/**
 * THE one opener (#EXP-1674): the active account's origin, the slug of [teamId]
 * (the surface's team; null = the active team's) for `~/` links, and every link
 * through the app's [DeepLinkBus] — AppNavHost resolves an in-app link against
 * the local DB of the account on that host and applies the one open rule. Null
 * while no instance is known, which leaves links as plain browser links.
 */
@Composable
fun rememberAppLinkHandler(teamId: String? = null): AppLinkHandler? {
    val context = LocalContext.current
    val entryPoint = remember(context) { appLinksEntryPoint(context) }
    val instanceUrl by entryPoint.authRepository().instanceUrl.collectAsStateWithLifecycle()
    val accountId by entryPoint.authRepository().activeAccountId.collectAsStateWithLifecycle()
    val selectedTeamId by entryPoint.teamSelection().selectedId.collectAsStateWithLifecycle()
    val slugTeamId = teamId?.takeIf { it.isNotEmpty() } ?: selectedTeamId
    val team by produceState<String?>(null, accountId, slugTeamId) {
        val account = accountId
        val id = slugTeamId
        value = null
        if (account != null && id != null) {
            // Collected: the team row may land after this surface does.
            runCatching {
                entryPoint.databaseHolder().database(account).teamDao().observeById(id).collect { value = it?.slug }
            }
        }
    }
    val origin = remember(instanceUrl) { instanceOrigin(instanceUrl) } ?: return null
    return remember(origin, entryPoint, team, slugTeamId) {
        val bus = entryPoint.deepLinkBus()
        // The resolver matches accounts by `URI(instanceUrl).host`, so hand it
        // that exact host rather than the link's (which may differ in case).
        val host = runCatching { URI(origin).host }.getOrNull().orEmpty()
        AppLinkHandler(origin, team, slugTeamId) { link, url ->
            when (link) {
                is AppLink.External -> bus.openWebUrl(Uri.parse(url))
                AppLink.Ignore -> Unit
                else -> bus.openLink(link, Uri.parse(url), host)
            }
        }
    }
}

/**
 * #EXP-1674 (prose-links.json): agent prose with its bare in-app URLs rewritten
 * to their chips, under the transcript's [LocalIssueRefBare] (display-only, like
 * the bare identifiers: descriptions, comments and every stored-text path keep
 * their bytes). The raw text until the synced rows have been read.
 */
@Composable
internal fun rememberChippedProse(text: String): String {
    val handler = LocalAppLinks.current
    val issueRefs = LocalIssueRefs.current
    val enabled = LocalIssueRefBare.current && handler != null && text.contains("://")
    val context = LocalContext.current
    val chipped by produceState(text, text, enabled, handler, issueRefs) {
        value = text
        val links = handler
        val teamId = links?.teamId
        if (!enabled || links == null || teamId == null) return@produceState
        // Here, not on composition: a render outside the app has no entry point.
        val entryPoint = appLinksEntryPoint(context)
        val account = entryPoint.authRepository().activeAccountId.value ?: return@produceState
        val asked = LinkedHashSet<Pair<ProseLinks.ChipKind, String>>()
        ProseLinks.chipBareAppLinks(text, links.origin, links.team) { kind, id ->
            asked.add(kind to id)
            null
        }
        if (asked.isEmpty()) return@produceState
        val db = runCatching { entryPoint.databaseHolder().database(account) }.getOrNull() ?: return@produceState
        val titles = asked.associateWith { (kind, id) ->
            runCatching {
                when (kind) {
                    ProseLinks.ChipKind.Issue -> issueRefs?.resolve(id)?.title
                    ProseLinks.ChipKind.Board -> db.boardDao().observeBySlug(teamId, id).first()?.name
                    ProseLinks.ChipKind.Session -> db.sessionDao().observeById(id).first()
                        ?.takeIf { it.teamId == teamId }
                        ?.let { row -> row.title?.takeIf { it.isNotBlank() } ?: "Session" }
                    ProseLinks.ChipKind.Agent -> db.agentDefinitionDao().observeById(id).first()
                        ?.takeIf { it.teamId == teamId }?.name
                }
            }.getOrNull()
        }
        value = ProseLinks.chipBareAppLinks(text, links.origin, links.team) { kind, id -> titles[kind to id] }
    }
    return if (enabled) chipped else text
}

/**
 * #EXP-1674: a host's `textLinks` for a `hostLinks` Text (the Row subtitle): the
 * `#IDENT` refs that resolve in [teamId], on the team's short issue route; an
 * unresolved ref stays text. Collected: issues sync in behind the surface.
 */
@Composable
fun rememberIssueTextLinks(teamId: String?, teamSlug: String?): (String) -> List<TextLinkSegment>? {
    val context = LocalContext.current
    val entryPoint = remember(context) { appLinksEntryPoint(context) }
    val accountId by entryPoint.authRepository().activeAccountId.collectAsStateWithLifecycle()
    val known by produceState(emptySet<String>(), accountId, teamId) {
        val account = accountId
        value = emptySet()
        if (account != null && teamId != null) {
            runCatching {
                entryPoint.databaseHolder().database(account).issueDao().observeByTeam(teamId).collect { issues ->
                    value = issues.mapTo(HashSet()) { it.identifier.uppercase() }
                }
            }
        }
    }
    return remember(known, teamSlug) { { text -> issueTextLinks(text, teamSlug, known) } }
}

/** The pure half of [rememberIssueTextLinks]: null = no link. */
internal fun issueTextLinks(text: String, team: String?, known: Set<String>): List<TextLinkSegment>? {
    if (team == null) return null
    val segments = mutableListOf<TextLinkSegment>()
    var at = 0
    for (match in IssueRefs.findAll(text)) {
        if (match.identifier.uppercase() !in known) continue
        val href = expandTeamLink("~/issues/${match.identifier}", team) ?: continue
        if (match.start > at) segments += TextLinkSegment(text.substring(at, match.start))
        segments += TextLinkSegment(text.substring(match.start, match.end), href)
        at = match.end
    }
    if (segments.isEmpty()) return null
    if (at < text.length) segments += TextLinkSegment(text.substring(at))
    return segments
}
