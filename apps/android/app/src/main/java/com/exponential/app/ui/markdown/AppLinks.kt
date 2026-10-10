package com.exponential.app.ui.markdown

import android.content.Context
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.TextLinkStyles
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.domain.AppLink
import com.exponential.app.domain.classifyAppLink
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.net.URI

/**
 * EXP-1188: links in agent prose (narration, Guide, thread) that point at THIS
 * instance open in the app. [origin] = the active account's instance origin
 * (`https://host[:port]`, no trailing slash); [onOpen] navigates an
 * [AppLink.Issue] / [AppLink.Session], given the link's ABSOLUTE url.
 */
class AppLinkHandler(
    val origin: String,
    val onOpen: (link: AppLink, url: String) -> Unit,
)

/**
 * Provided by the agent surfaces only (the session screen + the Guide face);
 * null (the default) keeps every markdown link a plain browser link, as issue
 * descriptions and comments have always rendered.
 */
val LocalAppLinks = compositionLocalOf<AppLinkHandler?> { null }

/**
 * The annotation a link [href] gets under [handler] (`fixtures/app-link.json`):
 * an in-app issue/run = a click that navigates, another instance page = the
 * absolute instance URL, external = the href as written, ignore = none. With
 * no handler the href opens as written, as before EXP-1188.
 */
internal fun appLinkAnnotation(
    href: String,
    handler: AppLinkHandler?,
    styles: TextLinkStyles,
): LinkAnnotation? {
    if (handler == null) return LinkAnnotation.Url(url = href, styles = styles)
    return when (val link = classifyAppLink(href, handler.origin)) {
        is AppLink.Issue, is AppLink.Session -> {
            val url = absoluteUrl(href, handler.origin)
            LinkAnnotation.Clickable(
                tag = url,
                styles = styles,
                linkInteractionListener = { handler.onOpen(link, url) },
            )
        }
        is AppLink.App -> LinkAnnotation.Url(url = handler.origin + link.path, styles = styles)
        is AppLink.External -> LinkAnnotation.Url(url = link.url, styles = styles)
        AppLink.Ignore -> null
    }
}

private fun absoluteUrl(href: String, origin: String): String {
    val trimmed = href.trim()
    return if (trimmed.startsWith("/")) origin + trimmed else trimmed
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
}

private fun appLinksEntryPoint(context: Context): AppLinksEntryPoint =
    EntryPointAccessors.fromApplication(context.applicationContext, AppLinksEntryPoint::class.java)

/**
 * The agent surfaces' handler: the active account's origin, and navigation
 * through the app's [DeepLinkBus] — a run opens its steer screen, an issue goes
 * the verified-App-Link path (slug + identifier resolved against the local DB
 * of the account on that host, a Custom Tab when it never syncs). Null while
 * no instance is known, which leaves links as plain browser links.
 */
@Composable
fun rememberAppLinkHandler(): AppLinkHandler? {
    val context = LocalContext.current
    val entryPoint = remember(context) { appLinksEntryPoint(context) }
    val instanceUrl by entryPoint.authRepository().instanceUrl.collectAsStateWithLifecycle()
    val origin = remember(instanceUrl) { instanceOrigin(instanceUrl) } ?: return null
    return remember(origin, entryPoint) {
        val bus = entryPoint.deepLinkBus()
        // The resolver matches accounts by `URI(instanceUrl).host`, so hand it
        // that exact host rather than the link's (which may differ in case).
        val host = runCatching { URI(origin).host }.getOrNull().orEmpty()
        AppLinkHandler(origin) { link, url ->
            when (link) {
                is AppLink.Session -> bus.openSession(link.sessionId)
                is AppLink.Issue -> {
                    bus.openWebIssueRef(
                        uri = Uri.parse(url),
                        host = host,
                        teamSlug = link.teamSlug,
                        identifier = link.identifier,
                    )
                }
                else -> Unit
            }
        }
    }
}
