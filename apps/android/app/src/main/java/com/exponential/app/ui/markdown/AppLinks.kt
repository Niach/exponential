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
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.domain.AppLink
import com.exponential.app.domain.classifyAppLink
import com.exponential.app.domain.expandTeamLink
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.net.URI

/**
 * EXP-1188: links in agent prose (narration, Guide, thread) that point at THIS
 * instance open in the app. [origin] = the active account's instance origin
 * (`https://host[:port]`, no trailing slash); [team] = the slug `~/` links
 * expand against (#EXP-1674: the surface's team, else the active team's);
 * [onOpen] opens a classified link, given its ABSOLUTE url.
 */
class AppLinkHandler(
    val origin: String,
    val team: String? = null,
    val onOpen: (link: AppLink, url: String) -> Unit,
)

/**
 * Provided by the agent surfaces only (the session screen, the thread screen);
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
    if (handler == null) return LinkAnnotation.Url(url = href, styles = styles)
    return when (val link = classifyAppLink(href, handler.origin, handler.team)) {
        is AppLink.Issue, is AppLink.IssueRef, is AppLink.Board, is AppLink.Session, is AppLink.Plan,
        is AppLink.Thread, is AppLink.Threads, is AppLink.Agent, is AppLink.AgentHome, is AppLink.Inbox,
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
    return remember(origin, entryPoint, team) {
        val bus = entryPoint.deepLinkBus()
        // The resolver matches accounts by `URI(instanceUrl).host`, so hand it
        // that exact host rather than the link's (which may differ in case).
        val host = runCatching { URI(origin).host }.getOrNull().orEmpty()
        AppLinkHandler(origin, team) { link, url ->
            when (link) {
                is AppLink.External -> bus.openWebUrl(Uri.parse(url))
                AppLink.Ignore -> Unit
                else -> bus.openLink(link, Uri.parse(url), host)
            }
        }
    }
}
