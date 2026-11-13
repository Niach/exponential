package com.exponential.app.ui.apps

import android.content.Context
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.glassRow
import androidx.compose.material3.TextButton
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.PaddingValues
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.FilePickRequest
import at.exponential.ui.host.FunctionCallInfo
import at.exponential.ui.host.FunctionPolicy
import at.exponential.ui.host.HostExtension
import at.exponential.ui.host.HostFunction
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.MediaRule
import at.exponential.ui.host.PickedFile
import at.exponential.ui.host.NavigateTarget
import at.exponential.ui.host.RecordRow
import at.exponential.ui.host.RecordMigrations
import at.exponential.ui.host.RecordsOverlay
import at.exponential.ui.host.RecordsQuery
import at.exponential.ui.host.RecordsStore
import at.exponential.ui.host.Screens
import at.exponential.ui.host.SourceResolver
import at.exponential.ui.ffi.HostZone
import at.exponential.ui.host.A2UI_VERSION
import at.exponential.ui.host.TimeZoneOffset
import at.exponential.ui.host.UrlPolicy
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceSettings
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.images.resolveAccountForUrl
import com.exponential.app.data.api.AppsApi
import com.exponential.app.data.api.buildImageUploadBody
import com.exponential.app.data.db.AppEntity
import com.exponential.app.data.db.AppRecordEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AppActions
import com.exponential.app.domain.AppHostValues
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.TeamZone
import com.exponential.app.domain.ViaApp
import com.exponential.app.domain.ViaAppClaim
import com.exponential.app.domain.viaAppClaim
import com.exponential.app.domain.AppSources
import com.exponential.app.domain.AppUpstream
import com.exponential.app.domain.AppLink
import com.exponential.app.domain.Apps
import com.exponential.app.domain.LINK_NO_ACCESS
import com.exponential.app.domain.classifyAppLink
import com.exponential.app.domain.expandTeamLink
import com.exponential.app.ui.components.failedAction
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.CodeBlock
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import com.exponential.app.ui.markdown.openHref
import com.exponential.app.ui.markdown.rememberIssueTextLinks
import com.exponential.app.ui.markdown.rememberAppLinkHandler
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import io.ktor.client.HttpClient
import io.ktor.client.plugins.timeout
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.ByteArrayContent
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * VAPP-110: the Android app as an Exponential UI HOST, the twin of the web
 * app's `lib/exponential-ui-host.tsx` and the IDE's `exponential_ui_host.rs`.
 * Everything goes through the SDK's PUBLIC Compose host API
 * ([ExponentialHost] + [at.exponential.ui.host.HostSurface]), like a third
 * party's host would:
 *  - functions: `harness_toast` (the app's toast stack), `harness_mcp` (a
 *    `tools/call` on the instance's own `/api/mcp` as the signed-in account); a
 *    link goes through the URL policy (`~/` = the app's team) and THE opener;
 *  - bindings: the `exp:` scheme over Room (`exp:devices`, re-derived on the
 *    liveness tick), [AppHostValues]; VAPP-119 `exp:issues` / `boards` /
 *    `members` / `sessions` / `comments` / `attachments` over the scope
 *    team's synced rows, [AppSources]; VAPP-122 the format's
 *    `records:<kind>?…` over the hosting app's synced records (or the
 *    `app=<slug>` app of the same team), [AppRecords];
 *  - VAPP-230 `upstream:<server>/<tool>?…`: tRPC `apps.upstreamRead` as
 *    the viewer, re-read every [AppUpstream.CACHE_SECONDS] while bound; a
 *    refusal or offline emits nothing (the template's default stays), a
 *    disconnected server raises the "Connect <server>" banner
 *    ([AppUpstreamAnswers], [AppHostBanner]);
 *  - VAPP-122 `records.upsert {kind, data, id?}` and `records.delete {id}`:
 *    tRPC `apps.records.*` as the viewer, no consent card
 *    (the app's own rows), a refusal = the failed-action toast;
 *    VAPP-235 every write goes through the OFFLINE OUTBOX
 *    ([AppRecordOutbox]): persisted, laid over the synced rows, replayed in
 *    order; alone in the queue it awaits its own outcome (as online), else
 *    it answers from the entry at once;
 *  - VAPP-194 `records.batch {writes}`: a package action's writes as ONE
 *    `apps.records.batch`; a failed action (that or a false `require`) =
 *    `onActionFailed`'s failed-action toast; VAPP-215 a landed one toasts
 *    Undo for 10 s (`apps.records.undo`, via `app`);
 *  - extensions: the app extension (`packages/ui/exponential-ui/extension.json`)
 *    with Compose painters for its `IconDisc` and `LiveDot` natives (Exponential
 *    ships no built-in apps: only the team's packages are installed);
 *  - VAPP-120 typed actions: every `exponential_<noun>_<op>` of
 *    [AppActions] is the same `tools/call` as the viewer (`{op, …args}`; no
 *    op for the verb tool `exponential_sessions_send`);
 *  - policy: `harness_mcp` and `exponential_*` ASK first (the consent card,
 *    [AppConsentCard]); a typed action the hosting app's synced
 *    `granted_tools` names runs without the card;
 *    urls and media resolve against the instance (attachments with the
 *    account's bearer), every href through the URL policy;
 *  - VAPP-221 files: a FileUpload's `pickFiles` opens the system document
 *    picker; each picked file then POSTs (one at a time, in picked order) to
 *    `/api/apps/{appId}/files` as the viewer and its `url` lands in the
 *    FileUpload's bound list ([Apps.appUploadTarget] + [Apps.withUploadedUrl]);
 *    a failure = the failed-action toast `upload` `the file`; at app access
 *    `read` nothing uploads (Android has no Run as).
 */

@EntryPoint
@InstallIn(SingletonComponent::class)
interface AppHostEntryPoint {
    fun authRepository(): AuthRepository
    fun databaseHolder(): DatabaseHolder
    fun httpClient(): HttpClient
    fun appsApi(): AppsApi
    fun appRecordOutbox(): AppRecordOutbox
}

/** The package files every host installs (read from the shared assets). */
object AppHostAssets {
    @Volatile private var extension: String? = null

    /** The app extension catalog (`extension.json`). */
    fun extensionJson(context: Context): String =
        extension ?: read(context, "extension.json").also { extension = it }

    private fun read(context: Context, path: String): String =
        context.assets.open(path).bufferedReader().use { it.readText() }
}

/** The consent card's store: one pending `ask` call at a time, answered by
 *  the person (Deny / Allow). */
class AppConsentGate {
    private var pending: Pair<FunctionCallInfo, CompletableDeferred<Boolean>>? by mutableStateOf(null)

    /** The call waiting for an answer (null = none). */
    val current: FunctionCallInfo? get() = pending?.first

    suspend fun ask(call: FunctionCallInfo): Boolean {
        pending?.second?.complete(false)
        val answer = CompletableDeferred<Boolean>()
        pending = call to answer
        return answer.await()
    }

    fun answer(ok: Boolean) {
        pending?.second?.complete(ok)
        pending = null
    }
}

/** VAPP-107 (the consent prompt's `slot`): what the card shows of a call:
 *  the tool it names, its arguments as compact JSON (a `harness_mcp` call's
 *  MCP `arguments`, else the function's own args; an empty object = none)
 *  and whether it sounds destructive. The web `consentWords` and desktop
 *  `consent_words` derive the same three. */
data class AppConsentWords(
    val tool: String,
    val args: String?,
    val destructive: Boolean,
    /** VAPP-120: a typed action's own title ([AppActions.actionConsent]),
     *  replacing the generic prompt's; null = the generic copy. */
    val title: String? = null,
) {
    /** Past [PREVIEW_CHARS] characters the arguments show as a preview
     *  (desktop's rule; the web adds a Show all disclosure). */
    val argsPreview: String?
        get() = args?.let { if (it.length > PREVIEW_CHARS) it.take(PREVIEW_CHARS) + "…" else it }

    companion object {
        const val PREVIEW_CHARS = 160

        fun of(call: FunctionCallInfo): AppConsentWords {
            // VAPP-120: a typed action is worded for its operation (the contract's rows).
            AppActions.actionConsent(call.name, call.args)?.let { typed ->
                return AppConsentWords(tool = typed.tool, args = typed.args, destructive = typed.destructive, title = typed.title)
            }
            val mcp = call.name == "harness_mcp"
            val tool = if (mcp) call.args["tool"]?.string.orEmpty() else call.name
            val raw: JsonValue? = if (mcp) call.args["arguments"] else JsonValue.Obj(call.args)
            val json = (raw as? JsonValue.Obj)?.orderedJson
            val op = raw?.get("op")?.string
            return AppConsentWords(
                tool = tool,
                args = json?.takeIf { it != "{}" },
                destructive = Prompts.destructiveTool(tool, op),
            )
        }
    }
}

/**
 * Round 9 §1.4: a package screen onto a fresh surface: a minted id under
 * [base], the template on it (`data` = a screen's `{params}`), and only then
 * the [current] surface goes, so it paints until the new one exists. Surface
 * ids are unique for the host's lifetime, so the old id can never come back.
 * The new id; the caller paints it.
 */
internal fun ExponentialHost.reopenSurface(current: String?, base: String, packageId: String, templateId: String, data: JsonValue? = null): String {
    val surfaceId = mintSurfaceId(base)
    val apply = buildMap {
        put("surfaceId", JsonValue.Str(surfaceId))
        put("templateId", JsonValue.Str(templateId))
        put("packageId", JsonValue.Str(packageId))
        if (data != null) put("data", data)
    }
    receive(JsonValue.Obj(mapOf("version" to JsonValue.Str(A2UI_VERSION), "applyTemplate" to JsonValue.Obj(apply))), trusted = true)
    if (current != null) {
        receive(JsonValue.Obj(mapOf("version" to JsonValue.Str(A2UI_VERSION), "deleteSurface" to JsonValue.Obj(mapOf("surfaceId" to JsonValue.Str(current))))))
    }
    return surfaceId
}

/** The consent card: what a surface wants to run (the tool AND its
 *  arguments in the CodeBlock), Deny (focused) / Allow, worded by the
 *  contract's `exponential-ui-consent` prompt (its destructive entry when
 *  the call sounds destructive). */
@Composable
fun AppConsentCard(gate: AppConsentGate) {
    val call = gate.current ?: return
    val words = AppConsentWords.of(call)
    PromptAlert(
        prompt = Prompts.exponentialUiConsent(words.tool, words.destructive).let { copy ->
            words.title?.let { copy.copy(title = it) } ?: copy
        },
        onDismiss = { gate.answer(false) },
        handlers = mapOf("deny" to { gate.answer(false) }, "allow" to { gate.answer(true) }),
        testTags = mapOf("deny" to "exponential-ui-consent-deny", "allow" to "exponential-ui-consent-allow"),
        content = words.argsPreview?.let { args ->
            {
                CodeBlock(
                    code = args,
                    title = null,
                    modifier = Modifier.testTag("exponential-ui-consent-arguments"),
                )
            }
        },
    )
}

/** #VAPP-230: every live upstream binding's last answer, for the banners
 *  ([AppUpstream.banners]: one per server answering upstreamDisconnected;
 *  the web's `UpstreamAnswers`). */
class AppUpstreamAnswers {
    private val answers = HashMap<Int, AppUpstream.Answer>()
    private var nextId = 0

    /** The banners to show, sorted by server. */
    var banners: List<AppUpstream.Banner> by mutableStateOf(emptyList())
        private set

    /** A new binding's id. */
    fun bind(): Int = synchronized(answers) { ++nextId }

    /** Binding [id]'s latest answer (null = unbound). */
    fun set(id: Int, answer: AppUpstream.Answer?) {
        val next = synchronized(answers) {
            if (answer != null) answers[id] = answer else answers.remove(id)
            AppUpstream.hostBanners(answers.values)
        }
        if (next != banners) banners = next
    }
}

/** The banners above a host's surfaces: #VAPP-230 one "Connect <server>"
 *  per disconnected upstream ([upstreams]; its Connect = [onConnect],
 *  Settings › MCP), else the catalog-update banner (a local host has no
 *  transport, so no offline state). */
@Composable
fun AppHostBanner(
    host: ExponentialHost,
    modifier: Modifier = Modifier,
    upstreams: AppUpstreamAnswers? = null,
    onConnect: (() -> Unit)? = null,
) {
    val connect = upstreams?.banners.orEmpty()
    if (connect.isNotEmpty()) {
        Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            connect.forEach { banner ->
                // The amber notice of ×4 (upstream.json `binding`): the
                // OfflineBanner row: the notice rung, the yellow warning
                // icon, the text, one text action.
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassRow(opaque = true)
                        .padding(start = 14.dp, end = 6.dp, top = 4.dp, bottom = 4.dp)
                        .testTag("exponential-ui-upstream-connect"),
                ) {
                    Icon(
                        ExpIcons.uiWarning,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = DesignTokens.Semantic.Yellow,
                    )
                    Spacer(Modifier.width(10.dp))
                    Text(
                        banner.text,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                    )
                    if (onConnect != null && banner.connect) {
                        Spacer(Modifier.width(4.dp))
                        TextButton(onClick = onConnect, contentPadding = PaddingValues(horizontal = 12.dp)) {
                            Text(AppUpstream.CONNECT, style = MaterialTheme.typography.labelMedium)
                        }
                    }
                }
            }
        }
        return
    }
    if (host.unsupportedCatalog == null) return
    val shape = RoundedCornerShape(8.dp)
    Text(
        "This surface needs a newer Exponential. Update the app to show it.",
        style = MaterialTheme.typography.bodySmall,
        color = DesignTokens.Semantic.Yellow,
        modifier = modifier
            .fillMaxWidth()
            .background(DesignTokens.Semantic.Yellow.copy(alpha = 0.1f), shape)
            .border(1.dp, DesignTokens.Semantic.Yellow.copy(alpha = 0.4f), shape)
            .padding(horizontal = 12.dp, vertical = 8.dp)
            .testTag("exponential-ui-catalog-update"),
    )
}

/** The icon registry the painter reads: a concept (`nav-devices`) or a
 *  registry name (`monitor`), drawn in the current content colour.
 *  VAPP-221: a FileUpload's pick goes to [picks] (the system picker). */
private class AppHostPlugin(private val picks: AppFilePicks) : HostPlugin {
    override fun icon(name: String, size: Float): (@Composable () -> Unit)? {
        val glyph = ExpIcons.byConcept(name) ?: ExpIcons.byName(name) ?: return null
        return { Icon(glyph, contentDescription = null, modifier = Modifier.size(size.dp)) }
    }

    override fun pickFiles(request: FilePickRequest) = picks.ask(request)
}

/** VAPP-221: the FileUpload pick waiting on the system picker (one at a
 *  time: a new ask replaces it), answered with the picked uris. */
private class AppFilePicks {
    private var pending: FilePickRequest? = null

    /** Opens the system picker for a request (set by the composable). */
    var launch: ((FilePickRequest) -> Unit)? = null

    /** The current host's handler of a pick (set as the host is built). */
    var onPicked: ((FilePickRequest, List<Uri>) -> Unit)? = null

    fun ask(request: FilePickRequest) {
        val open = launch ?: return
        pending = request
        runCatching { open(request) }.onFailure { pending = null }
    }

    fun picked(uris: List<Uri>) {
        val request = pending ?: return
        pending = null
        if (uris.isNotEmpty()) onPicked?.invoke(request, uris)
    }
}

/** A FileUpload's `accept` (MIME types and `.ext` extensions, comma
 *  separated) → the document picker's MIME types (`*` + `/` + `*` = anything). */
private fun acceptMimeTypes(accept: String?): Array<String> {
    val types = accept.orEmpty().split(',').map { it.trim().lowercase() }.filter { it.isNotEmpty() }.mapNotNull { entry ->
        if ('/' in entry) entry else MimeTypeMap.getSingleton().getMimeTypeFromExtension(entry.removePrefix("."))
    }.distinct()
    return if (types.isEmpty()) arrayOf("*/*") else types.toTypedArray()
}

/** The value at a JSON pointer (`/a/0/b`; "" = the root), null when absent. */
private fun pointerValue(root: JsonValue, path: String): JsonValue? {
    if (path.isEmpty()) return root
    var current: JsonValue = root
    for (raw in path.removePrefix("/").split('/')) {
        val key = raw.replace("~1", "/").replace("~0", "~")
        current = when (val at = current) {
            is JsonValue.Obj -> at.v[key]
            is JsonValue.Arr -> key.toIntOrNull()?.let { at.v.getOrNull(it) }
            else -> null
        } ?: return null
    }
    return current
}

/** One picked file: what the surface hears + where its bytes are read. */
private class AppPickedUpload(val uri: Uri, val name: String, val type: String, val size: Long)

/** The disc tone → the theme colour key (the web's `DISC_TONE`). */
private fun discColorKey(tone: String?): String = when (tone) {
    "success" -> "success"
    "danger" -> "destructive"
    "muted" -> "mutedForeground"
    else -> "primary"
}

/** `IconDisc`: a 32dp circle in the tone colour at 15% with the icon centred
 *  in the tone colour (the web's `IconDiscPainter`). */
private object IconDiscPainter : ExtensionPainter {
    override fun measure(leaf: ExtensionLeaf, wrap: Float?): Size = Size(32f, 32f)

    @Composable
    override fun Paint(context: ExtensionContext) {
        val theme = context.theme
        val color = theme?.color(discColorKey(context.props["tone"]?.string), context.mode)
            ?: theme?.color("primary", context.mode)
            ?: DesignTokens.Semantic.Blue
        val name = context.props["icon"]?.string.orEmpty()
        val glyph = ExpIcons.byConcept(name) ?: ExpIcons.byName(name)
        Box(
            modifier = Modifier.size(32.dp).background(color.copy(alpha = 0.15f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (glyph != null) Icon(glyph, contentDescription = null, tint = color, modifier = Modifier.size(16.dp))
        }
    }
}

/** `LiveDot`: the 8dp status dot in the app's device / session dot colours. */
private object LiveDotPainter : ExtensionPainter {
    override fun measure(leaf: ExtensionLeaf, wrap: Float?): Size = Size(8f, 8f)

    @Composable
    override fun Paint(context: ExtensionContext) {
        val muted = context.theme?.color("mutedForeground", context.mode) ?: DesignTokens.Semantic.Neutral
        val tone: Color = when (context.props["tone"]?.string) {
            "live" -> DesignTokens.Semantic.Green
            "attention" -> DesignTokens.Semantic.Yellow
            "done" -> DesignTokens.Semantic.Blue
            "unread" -> context.theme?.color("primary", context.mode) ?: muted
            "idle" -> muted.copy(alpha = 0.4f)
            else -> muted
        }
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Box(Modifier.size(8.dp).background(tone, CircleShape))
        }
    }
}

/** VAPP-235: how often acked outbox entries re-check their grace while the shape catches up. */
private const val OUTBOX_PRUNE_TICK_MS = 5_000L

/** The theme every app surface paints with (the web's `DEFAULT_THEME_ID`). */
const val APP_THEME_ID = "exponential"

/**
 * #EXP-1674 (host-router.json `crossApp`): the href [link] opens when the app
 * [sourceAppId]'s surface opens it: its own screens as written, another app's
 * only through its `exposed` contract, with the params it declares. Null = the
 * toast [LINK_NO_ACCESS], the same for a missing app. A person's URL or an
 * agent's chat link never comes here.
 */
private suspend fun crossAppHref(link: AppLink, origin: String, sourceAppId: String, database: ExponentialDatabase?): String? {
    val teamSlug: String
    val appSlug: String
    val screen: String?
    val params: Map<String, String>?
    when (link) {
        is AppLink.AppHome -> { teamSlug = link.teamSlug; appSlug = link.appSlug; screen = null; params = null }
        is AppLink.AppScreen -> { teamSlug = link.teamSlug; appSlug = link.appSlug; screen = link.screen; params = link.params }
        else -> return null
    }
    val db = database ?: return null
    val team = db.teamDao().observeBySlug(teamSlug).first() ?: return null
    val app = db.appDao().observeByTeam(team.id).first().firstOrNull { it.slug == appSlug } ?: return null
    if (app.id == sourceAppId) return if (screen != null) origin + Apps.screenPath(teamSlug, appSlug, screen, params.orEmpty()) else origin + "/t/" + Uri.encode(teamSlug) + "/apps/" + Uri.encode(appSlug)
    val published = app.packageJson?.takeIf { (app.publishedVersion ?: 0) > 0 } ?: return null
    val target = Screens.crossAppTarget(JsonValue.parse(published), screen, params).getOrNull() ?: return null
    return origin + Apps.screenPath(teamSlug, appSlug, target.first, target.second)
}

/**
 * A host over [packages] (package JSON texts), alive while the caller is composed.
 */
@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
@Composable
fun rememberAppHost(
    packages: List<String>,
    consent: AppConsentGate,
    /** VAPP-122: the hosting app (its records; null = none: `records:<kind>` is null, writes fail). */
    appId: String? = null,
    /** The hosting app's team (an `app=<slug>` names an app of this team). */
    teamId: String? = null,
    /** The viewer's user id (`mine=1`). */
    me: String? = null,
    /** The package's `theme` (a built-in id); null = [APP_THEME_ID]. */
    themeId: String? = null,
    /** Release train 2026-10-25: the row's grants are about the HEAD package; false = an older version's preview, which runs with none. */
    grants: Boolean = true,
    /** VAPP-200: the viewer's app access is `read`: the app's own writing controls paint disabled. */
    readOnly: Boolean = false,
    /** #VAPP-230: the upstream reads' answers (the banners); null = untracked. */
    upstreams: AppUpstreamAnswers? = null,
    /** eApps R8 (apps.json `migrationCases`): a draft preview's published `version`; the hosting app's rows read through the package's migrations (copies, nothing written). */
    migrateFrom: String? = null,
    /** VAPP-218: `app_navigate` opens a screen (the preview's shell); null = no shell, refused. Kept stable by the caller. */
    navigate: ((NavigateTarget) -> Boolean)? = null,
): ExponentialHost {
    val context = LocalContext.current
    val toaster = LocalToaster.current
    // VAPP-215: the batch's Undo outlives the call that toasted it.
    val undoScope = rememberCoroutineScope()
    // The markdown links' rule for an allowed href (web + iOS alike): an
    // instance link opens in-app, anything else in the Custom Tab; no
    // instance known = the SDK's system opener.
    val links = rememberAppLinkHandler(teamId)
    // #EXP-1674: the opener and text links read the latest; `~/x`'s base is in
    // the host's URL policy, so the team's slug landing rebuilds it once.
    val latestLinks by rememberUpdatedState(links)
    val latestTextLinks by rememberUpdatedState(rememberIssueTextLinks(teamId, links?.team))
    // VAPP-221: a FileUpload's pick = the system document picker (several
    // when the FileUpload is `multiple`).
    val picks = remember { AppFilePicks() }
    val pickMany = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris: List<Uri> -> picks.picked(uris) }
    val pickOne = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? -> picks.picked(listOfNotNull(uri)) }
    SideEffect {
        picks.launch = { request ->
            val types = acceptMimeTypes(request.accept)
            if (request.multiple) pickMany.launch(types) else pickOne.launch(types)
        }
    }
    val host = remember(packages, consent, themeId, links?.origin, links?.team, appId, teamId, me, grants, readOnly, upstreams, migrateFrom, navigate) {
        val deps = EntryPointAccessors.fromApplication(context.applicationContext, AppHostEntryPoint::class.java)
        val auth = deps.authRepository()
        val db = accountDatabaseFlow(auth, deps.databaseHolder())
        val devices: Flow<List<DeviceEntity>> = db.scopedQuery(emptyList()) { it.deviceDao().observeAll() }
        val users: Flow<List<UserEntity>> = db.scopedQuery(emptyList()) { it.userDao().observeAll() }
        // VAPP-120: the hosting app's row, read at call time for its grants
        // (the web/desktop settings page may change them while the host lives).
        val hostingApp: Flow<AppEntity?> =
            if (appId == null || !grants) flowOf(null) else db.scopedQuery(null) { it.appDao().observeById(appId) }
        // VAPP-231 (via-app.json `claim`): what this host's MCP calls claim, read
        // off the synced row at send time; the server checks it.
        val ownApp: Flow<AppEntity?> =
            if (appId == null) flowOf(null) else db.scopedQuery(null) { it.appDao().observeById(appId) }
        suspend fun hostClaim(): ViaAppClaim? = viaAppClaim(appId, ownApp.first()?.publishedVersion)
        val teamApps: Flow<List<AppEntity>> =
            if (teamId == null) flowOf(emptyList()) else db.scopedQuery(emptyList()) { it.appDao().observeByTeam(teamId) }
        fun recordsOf(app: String?): Flow<List<AppRecordEntity>> =
            if (app == null) flowOf(emptyList()) else db.scopedQuery(emptyList()) { it.appRecordDao().observeByApp(app) }
        // VAPP-119: the scope team's rows for the other `exp:` sources (no
        // team = an empty store); each source combines only what it reads.
        fun <T> teamRows(query: (ExponentialDatabase, String) -> Flow<List<T>>): Flow<List<T>> =
            if (teamId == null) flowOf(emptyList()) else db.scopedQuery(emptyList()) { query(it, teamId) }
        val teamBoards = teamRows { d, t -> d.boardDao().observeByTeam(t) }.map { it.map(AppSources::board) }
        val teamIssues = teamRows { d, t -> d.issueDao().observeByTeamForApps(t) }.map { it.map(AppSources::issue) }
        val teamMembers = teamRows { d, t -> d.teamMemberDao().observeByTeam(t) }.map { it.map(AppSources::member) }
        val sourceUsers = users.map { it.map(AppSources::user) }
        val teamSessions = teamRows { d, t -> d.sessionDao().observeByTeam(t) }.map { it.map(AppSources::session) }
        val teamComments = teamRows { d, t -> d.commentDao().observeByTeam(t) }.map { it.map(AppSources::comment) }
        val teamAttachments = teamRows { d, t -> d.attachmentDao().observeByTeam(t) }.map { it.map(AppSources::attachment) }
        // FEED-243: `exp:issues?touched=` also reads the timeline, the session
        // links, the sessions and the comments (only when it asks).
        val teamIssueEvents = teamRows { d, t -> d.issueEventDao().observeByTeam(t) }.map { it.map(AppSources::issueEvent) }
        val teamSessionIssues = teamRows { d, t -> d.sessionIssueDao().observeByTeam(t) }.map { it.map(AppSources::sessionIssue) }
        fun sourceStore(name: String, params: Map<String, String> = emptyMap()): Flow<AppSources.Store> = when (name) {
            "issues" -> {
                val base = combine(teamBoards, teamIssues, sourceUsers) { b, i, u -> AppSources.Store(boards = b, issues = i, users = u) }
                if (params["touched"].isNullOrEmpty()) base
                else combine(base, teamIssueEvents, teamSessionIssues, teamSessions, teamComments) { s, e, l, ss, c ->
                    s.copy(issueEvents = e, sessionIssues = l, sessions = ss, comments = c)
                }
            }
            "boards" -> teamBoards.map { AppSources.Store(boards = it) }
            "members", "me" -> combine(teamMembers, sourceUsers) { m, u -> AppSources.Store(members = m, users = u) }
            "sessions" -> teamSessions.map { AppSources.Store(sessions = it) }
            "comments" -> combine(teamIssues, teamComments, sourceUsers) { i, c, u -> AppSources.Store(issues = i, comments = c, users = u) }
            "attachments" -> combine(teamIssues, teamAttachments, sourceUsers) { i, a, u -> AppSources.Store(issues = i, attachments = a, users = u) }
            else -> flowOf(AppSources.Store())
        }
        val origin = auth.instanceUrl.value?.trimEnd('/')
        val ownHost = origin?.let { runCatching { java.net.URI(it).host }.getOrNull() }?.let { listOf(it) }
        val token = origin?.let { resolveAccountForUrl(auth.accounts.value, auth.activeAccountId.value, it)?.token }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        // VAPP-201 (team-zone.json): dates in the TEAM zone, read live off
        // the synced team row; the device's without one.
        val zone = TeamHostZone()
        val teamZoneName: Flow<String?> =
            if (teamId == null) flowOf(null)
            else db.scopedQuery(null) { it.teamDao().observeById(teamId) }.map { it?.timeZone }.distinctUntilChanged()
        if (teamId != null) {
            scope.launch { teamZoneName.collect { zone.teamZone(it) } }
        }

        val outbox = deps.appRecordOutbox()
        // VAPP-122: the `records:<kind>` value over the parsed query (VAPP-189:
        // eUI's one evaluator, the SDK's `RecordsQuery` over the core).
        // VAPP-214: the hosting package's `records` (its kinds' `computed` fields apply on read).
        val packageRecords = packages.firstOrNull()?.let { JsonValue.parse(it)["records"] }
        // eApps R8: a draft preview's migrations (null = rows as stored).
        val migrations = packages.firstOrNull()?.let { JsonValue.parse(it)["migrations"] }?.takeIf { (it.array?.size ?: 0) > 0 && migrateFrom != null }
            ?.let { JsonValue.Obj(mapOf("migrations" to it)) }

        // VAPP-212: one store per app (a draft preview's migrated copies apart):
        // the rows stay in Rust and only what changed crosses the FFI.
        val recordStores = java.util.concurrent.ConcurrentHashMap<String, RecordsStore<AppRecordEntity>>()

        fun recordsValues(query: RecordsQuery): Flow<JsonValue?> {
            // The app whose records are read: the hosting app, or
            // (`app=`) another app of the same team by slug (it may
            // sync later, so the lookup follows the apps).
            val target: Flow<String?> = if (query.app == null) {
                flowOf(appId)
            } else {
                teamApps.map { rows -> rows.firstOrNull { it.slug == query.app }?.id }.distinctUntilChanged()
            }
            val rows = target.flatMapLatest { app -> recordsOf(app).map { app to it } }
            // VAPP-235: that app's outbox entries, laid over the synced rows
            // (re-emitting on every outbox change); while acked entries wait
            // for the shape, a tick re-checks their grace.
            val entries: Flow<List<AppRecords.OutboxEntry>> = target
                .flatMapLatest { app -> if (app == null) flowOf(emptyList()) else db.scopedQuery(emptyList()) { AppRecordOutbox.entries(it, app) } }
                .flatMapLatest { list ->
                    if (list.none { it.state == AppRecords.Outbox.ACKED_STATE }) {
                        flowOf(list)
                    } else {
                        kotlinx.coroutines.flow.flow {
                            while (true) {
                                emit(list)
                                delay(OUTBOX_PRUNE_TICK_MS)
                            }
                        }
                    }
                }
            // VAPP-219 (R5): the hosting app's refs; an expanded issue ref reads the team's issues.
            val refs = if (query.app == null) RecordsQuery.refs(packageRecords, query.from) else JsonValue.Null
            val expanded = (query.structured["expand"]?.array.orEmpty()).mapNotNull { it.string }
            val issueRefs = expanded.filter { refs[it]?.string == "issue" }
            val refIssues: Flow<List<IssueEntity>> = if (issueRefs.isEmpty()) flowOf(emptyList()) else teamRows { d, t -> d.issueDao().observeByTeamForApps(t) }
            // VAPP-222 (R6): the hosting app's state machine + my team role (an owner takes every edge).
            val transitions = if (query.app == null) RecordsQuery.transitions(packageRecords, query.from) else JsonValue.Null
            val role: Flow<String?> = if (transitions == JsonValue.Null) flowOf(null) else teamRows { d, t -> d.teamMemberDao().observeByTeam(t) }.map { rows -> rows.firstOrNull { it.userId == me }?.role }
            fun stored(row: AppRecords.Row) = RecordRow(
                id = row.id,
                kind = row.kind,
                data = JsonValue.Obj(row.data),
                createdBy = row.createdBy,
                authorId = row.authorId,
                createdAt = row.createdAt,
                updatedAt = row.updatedAt,
            )
            // eApps R8: a draft preview reads migrated copies of the hosting app's rows.
            fun shown(row: RecordRow): RecordRow {
                if (migrations == null || migrateFrom == null || query.app != null) return row
                val out = RecordMigrations.migrate(migrations, migrateFrom, row.kind, row.data, System.currentTimeMillis().toDouble())
                val kind = out["kind"]?.string
                val data = out["data"]
                return if (out["error"] != null || (out["applied"]?.number ?: 0.0) <= 0.0 || kind == null || data == null) row else row.copy(kind = kind, data = data)
            }
            val draft = migrations != null && migrateFrom != null && query.app == null
            return combine(rows, entries, users, refIssues, role) { (app, records), pending, people, teamIssueRows, myRole ->
                // VAPP-235: the outbox reads only the rows it wrote; acked entries
                // the shape reflects (or past the grace) leave.
                val written = AppRecords.outboxWrittenIds(pending)
                val touched = if (written.isEmpty()) emptyList() else records.filter { it.id in written }.map(AppRecords::row)
                val account = auth.activeAccountId.value
                if (account != null && pending.any { it.state == AppRecords.Outbox.ACKED_STATE }) outbox.reflect(account, pending.first().appId, touched)
                val laid = AppRecords.outboxOverlay(touched, pending, me)
                val overlay = RecordsOverlay(laid.upserts.map { shown(stored(it)) }, laid.deletes)
                // VAPP-187: `author` = the AUTHOR's name; VAPP-219: a member ref expands to {id, name}.
                val names = people.associate { it.id to it.name.orEmpty() }
                // VAPP-214: the hosting app's computed fields (another app's rows carry none).
                val computed = if (query.app == null) RecordsQuery.computed(packageRecords, query.from) else emptyMap()
                val issueSummaries = teamIssueRows.associate {
                    it.identifier to JsonValue.Obj(mapOf("id" to JsonValue.Str(it.id), "identifier" to JsonValue.Str(it.identifier), "title" to JsonValue.Str(it.title), "status" to JsonValue.Str(it.status)))
                }
                // Dogfood G: the month view's today + default month in the TEAM zone (the day today() reads).
                val nowMs = System.currentTimeMillis()
                val store = recordStores.getOrPut("${app.orEmpty()}${if (draft) ":draft" else ""}") { RecordsStore() }
                synchronized(store) {
                    store.sync(records, { it.id }, { shown(stored(AppRecords.row(it))) })
                    query.value(store, me, AppRecords.isoNow(nowMs), names, computed, today = TeamZone.day(nowMs, zone.offsetMinutes(nowMs.toDouble())), refs = refs, issues = issueSummaries, transitions = transitions, role = myRole, overlay = overlay)
                }
            }
        }

        val exp = SourceResolver { source, emit ->
            val values: Flow<JsonValue?> = when (source.name) {
                "devices" -> combine(devices, DeviceLiveness.ticker(AppHostValues.LIVENESS_TICK_MS)) { rows, now ->
                    AppHostValues.devices(rows, now)
                }
                // FEED-243: date-only `since` / `until` read in the team zone.
                in AppSources.SOURCES -> combine(sourceStore(source.name, source.params), teamZoneName) { store, tz ->
                    AppSources.sourceValue(source.name, source.params, store, me, if (readOnly) "read" else "write", tz)
                }
                else -> flowOf(null)
            }
            val job = scope.launch { values.distinctUntilChanged().collect { emit(it) } }
            return@SourceResolver { job.cancel() }
        }
        // VAPP-161: `records:<kind>?…` (the source's name = the kind).
        val records = SourceResolver { source, emit ->
            val query = RecordsQuery.parse(source)
            val values = if (query == null) flowOf(null) else recordsValues(query)
            val job = scope.launch { values.distinctUntilChanged().collect { emit(it) } }
            return@SourceResolver { job.cancel() }
        }

        // #VAPP-230: `upstream:<server>/<tool>?…` → the broker's answer for
        // the viewer, re-read every cacheSeconds while bound. A refusal or
        // offline emits nothing (the path keeps the template's data: the
        // binding's default); a disconnected one raises the banner.
        val upstream = SourceResolver { source, emit ->
            val id = upstreams?.bind()
            val job = scope.launch {
                while (isActive) {
                    val account = auth.activeAccountId.value
                    if (appId != null && account != null) {
                        try {
                            val json = deps.appsApi().upstreamRead(account, appId, source.uri)
                            val answer = AppUpstream.Answer.of(JsonValue.parse(json.toString()))
                            if (id != null) upstreams?.set(id, answer)
                            if (answer.ok) emit(answer.value)
                        } catch (e: Exception) {
                            if (e is kotlinx.coroutines.CancellationException) throw e
                            // Offline or refused: the binding keeps its default.
                        }
                    }
                    delay(AppUpstream.CACHE_SECONDS * 1000L)
                }
            }
            return@SourceResolver {
                job.cancel()
                if (id != null) upstreams?.set(id, null)
            }
        }

        val toast: HostFunction = { args, _ ->
            val text = args["message"]?.string.orEmpty()
            if (args["tone"]?.string == "error") toaster.error(text) else toaster.info(text)
        }
        val mcp: HostFunction = { args, _ ->
            callMcp(deps.httpClient(), origin, token, args["tool"]?.string.orEmpty(), args["arguments"], hostClaim())
        }
        // VAPP-122: the app's own rows (no consent card); a refusal (the
        // kind's schema, the caps, the plan quota) is a failed action.
        val api = deps.appsApi()
        // VAPP-235: every write is an outbox entry ([AppRecordOutbox.submit]):
        // landed = the online answer; refused = the failed-action toast and
        // the call fails; queued (offline, or behind pending entries) = the
        // answer from the entry (`{id}` / null / `{ids}`).
        suspend fun submit(function: String, args: Map<String, JsonValue>): AppRecordOutbox.Answer {
            val account = auth.activeAccountId.value ?: error("Not signed in")
            val app = appId ?: error("No app to keep records for")
            return outbox.submit(account, app, function, args)
        }
        // VAPP-245: a record form's `form {error, close}`: landed = the error
        // clears and the editor closes; invalid input (BAD_REQUEST) = the
        // message under the form, no toast. `author` = an owner's on-behalf
        // pick, online only (the outbox replays as the writer): straight to
        // the server.
        var formHost: ExponentialHost? = null
        val recordsUpsert: HostFunction = { args, call ->
            val errorPath = args["form"]?.get("error")?.string
            val closePath = args["form"]?.get("close")?.string
            val surface = { formHost?.surfaces?.get(call.surfaceId) }
            val landed = {
                errorPath?.let { surface()?.setData(it, JsonValue.Null) }
                closePath?.let { surface()?.setData(it, JsonValue.Bool(false)) }
            }
            var inline = false
            try {
                val author = args["author"]?.string
                if (author != null) {
                    val account = auth.activeAccountId.value ?: error("Not signed in")
                    val app = appId ?: error("No app to keep records for")
                    val data = kotlinx.serialization.json.Json.parseToJsonElement((args["data"] ?: JsonValue.Obj(emptyMap())).json) as? kotlinx.serialization.json.JsonObject
                    val result = try {
                        api.recordsUpsert(account, app, args["kind"]?.string.orEmpty(), data ?: kotlinx.serialization.json.JsonObject(emptyMap()), args["id"]?.string, author, viaApp = hostClaim())
                    } catch (e: com.exponential.app.data.api.TrpcException) {
                        if (e.code == "BAD_REQUEST" && errorPath != null) {
                            surface()?.setData(errorPath, JsonValue.Str(e.message.orEmpty()))
                            inline = true
                        }
                        throw e
                    }
                    landed()
                    JsonValue.Obj(mapOf("id" to JsonValue.Str(result.id)))
                } else {
                    when (val answer = submit("records_upsert", args)) {
                        is AppRecordOutbox.Answer.Refused -> {
                            if (answer.invalid && errorPath != null) {
                                surface()?.setData(errorPath, JsonValue.Str(answer.message.orEmpty()))
                                inline = true
                            }
                            error(answer.message ?: "Couldn't save the record")
                        }
                        is AppRecordOutbox.Answer.Done -> {
                            landed()
                            answer.answer
                        }
                        is AppRecordOutbox.Answer.Queued -> {
                            landed()
                            answer.answer
                        }
                    }
                }
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                if (!inline) toaster.failedAction("save", "the record", e.message)
                throw e
            }
        }
        val recordsDelete: HostFunction = { args, _ ->
            try {
                val answer = submit("records_delete", args)
                if (answer is AppRecordOutbox.Answer.Refused) error(answer.message ?: "Couldn't delete the record")
                null
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                toaster.failedAction("delete", "the record", e.message)
                throw e
            }
        }
        // VAPP-194: a package action's writes as ONE entry (one transaction
        // on the server, all or nothing); a refusal lands nothing and is the
        // action's failure: `onActionFailed` toasts it (not here, so it
        // shows once).
        val recordsBatch: HostFunction = { args, _ ->
            when (val answer = submit("records_batch", args)) {
                is AppRecordOutbox.Answer.Refused -> error(answer.message ?: "Couldn't save the record")
                is AppRecordOutbox.Answer.Done -> {
                    val account = auth.activeAccountId.value
                    val app = appId
                    val eventIds = answer.answer?.get("eventIds")?.array.orEmpty().mapNotNull { it.string }
                    // VAPP-215: a package action's writes toast Undo for 10 s (app-records.json `history`).
                    if (account != null && app != null) {
                        toaster.recordUndo("batch", eventIds) {
                            undoScope.launch {
                                try {
                                    api.recordsUndo(account, app, eventIds, via = "app")
                                } catch (e: Exception) {
                                    if (e is kotlinx.coroutines.CancellationException) throw e
                                    toaster.failedAction(AppRecords.HistoryCopy.UNDO_VERB, AppRecords.HistoryCopy.UNDO_THING, e.message)
                                }
                            }
                        }
                    }
                    JsonValue.Obj(mapOf("ids" to (answer.answer?.get("ids") ?: JsonValue.Arr(emptyList()))))
                }
                is AppRecordOutbox.Answer.Queued -> answer.answer
            }
        }
        // VAPP-120: every MCP operation as a typed function
        // (fixtures/app-actions.json): the same tools/call as the viewer,
        // `op` + the args (no op for a verb tool). EXP-1657: the aliases too,
        // so a package published before the `team_config` split keeps working.
        val targets = AppActions.TOOLS + AppActions.ALIASES.mapValues { (_, name) -> AppActions.TOOLS.getValue(name) }
        val actions: Map<String, HostFunction> = targets.mapValues { (_, target) ->
            val fn: HostFunction = { args, _ ->
                callMcp(deps.httpClient(), origin, token, target.tool, JsonValue.Obj(AppActions.arguments(target, args)), hostClaim())
            }
            fn
        }
        lateinit var built: ExponentialHost
        // VAPP-221: the picked files reach the FileUpload (`upload {files}`),
        // then upload one at a time in picked order; each answer's `url`
        // lands in the FileUpload's bound list. At app access `read` (or
        // without an app) nothing uploads.
        picks.onPicked = { request, uris ->
            scope.launch {
                val app = context.applicationContext
                val files = withContext(Dispatchers.IO) {
                    uris.map { uri ->
                        AppPickedUpload(
                            uri = uri,
                            name = MarkdownMediaUtils.guessFilename(app, uri),
                            type = MarkdownMediaUtils.guessMimeType(app, uri, fallback = "application/octet-stream"),
                            size = MarkdownMediaUtils.querySize(app, uri) ?: MarkdownMediaUtils.readBytes(app, uri)?.size?.toLong() ?: 0L,
                        )
                    }
                }
                request.done(files.map { PickedFile(it.name, it.size, it.type) })
                val hostingApp = appId
                if (readOnly || hostingApp == null) return@launch
                // The components as authored (the painted nodes carry the `files` binding resolved).
                val authored = built.surfaces[request.surfaceId]?.components.orEmpty()
                val upload = authored.firstOrNull { it["id"]?.string == request.componentId && it["component"]?.string == "FileUpload" }
                val target = upload?.let { Apps.appUploadTarget(authored, (it["name"] as? JsonValue.Str)?.v ?: request.componentId) }
                for (file in files) {
                    try {
                        val bytes = withContext(Dispatchers.IO) { MarkdownMediaUtils.readBytes(app, file.uri) } ?: error("That file could not be read")
                        val url = uploadAppFile(deps.httpClient(), origin, token, hostingApp, bytes, file.name, file.type)
                        val model = built.surfaces[request.surfaceId]
                        if (target != null && model != null) {
                            Apps.withUploadedUrl(pointerValue(model.data, target), file.name, url)?.let { model.setData(target, it) }
                        }
                    } catch (e: Exception) {
                        if (e is kotlinx.coroutines.CancellationException) throw e
                        toaster.failedAction(Apps.AppFiles.FAILED_VERB, Apps.AppFiles.FAILED_THING, e.message)
                    }
                }
            }
        }
        val functions = actions + mapOf(
            "records_upsert" to recordsUpsert,
            "records_delete" to recordsDelete,
            "records_batch" to recordsBatch,
            "harness_toast" to toast,
            "harness_mcp" to mcp,
        )

        built = ExponentialHost(
            HostOptions(
                functions = functions,
                sources = mapOf("exp" to exp, "records" to records, AppUpstream.SCHEME to upstream),
                extensions = listOf(
                    HostExtension(
                        AppHostAssets.extensionJson(context),
                        mapOf("IconDisc" to IconDiscPainter, "LiveDot" to LiveDotPainter),
                    ),
                ),
                packages = packages,
                policy = HostPolicy(
                    functions = FunctionPolicy(ask = listOf("harness_mcp", "exponential_*")),
                    // A granted typed action runs without the card; anything else asks.
                    onFunctionCall = { call ->
                        val granted = AppActions.parseActionName(call.name) != null &&
                            AppActions.grantAllows(AppActions.grantedList(hostingApp.first()?.grantedTools), call.name, call.args)
                        granted || consent.ask(call)
                    },
                    // VAPP-107: private hosts are denied unless opted in; the
                    // app's OWN instance is (the localhost dev setup), nothing
                    // else. A public origin listed here changes nothing.
                    // #EXP-1674: `~/x` = the app's team.
                    urls = UrlPolicy(baseUrl = origin, allowPrivate = ownHost, linkBase = expandTeamLink("~/", links?.team)),
                    // #EXP-1674: a link at another app's screen passes that app's `exposed` contract.
                    openUrl = links?.let { handler ->
                        { url: String ->
                            val current = latestLinks ?: handler
                            val link = classifyAppLink(url, current.origin)
                            val source = appId
                            if (source == null || (link !is AppLink.AppHome && link !is AppLink.AppScreen)) {
                                current.openHref(url)
                            } else {
                                undoScope.launch {
                                    val href = crossAppHref(link, current.origin, source, db.first())
                                    if (href != null) current.openHref(href) else toaster.error(LINK_NO_ACCESS)
                                }
                            }
                            Unit
                        }
                    },
                    // #EXP-1674: a Row subtitle's `#IDENT` refs that resolve in the app's team.
                    textLinks = { text -> latestTextLinks(text) },
                    media = MediaOptions(
                        baseUrl = origin,
                        allowPrivate = ownHost,
                        rules = if (origin != null && token != null) {
                            listOf(MediaRule("$origin/api/attachments/", mapOf("Authorization" to "Bearer $token")))
                        } else {
                            emptyList()
                        },
                        // VAPP-157: an app's media loads only a data: image /
                        // audio / video or the instance's own attachment, so
                        // bound team data never rides a url to another host.
                        allow = { src -> Apps.appMediaAllowed(src, origin.orEmpty()) },
                    ),
                ),
                theme = themeId?.let(ThemeHandle::builtin) ?: ThemeHandle.builtin(APP_THEME_ID),
                mode = Mode.Dark,
                // VAPP-200: at app access `read` every surface is read-only.
                settings = SurfaceSettings(readOnly = readOnly),
                plugin = AppHostPlugin(picks),
                // VAPP-194: a package action runs as the viewer; one that
                // landed nothing = the failed-action toast. VAPP-201: in the
                // team zone, else the device's.
                me = me,
                zone = zone,
                onActionFailed = { failure -> toaster.failedAction("save", "the record", failure.message) },
                navigate = navigate,
            ),
            scope,
        )
        formHost = built
        built
    }
    DisposableEffect(host) { onDispose { host.dispose() } }
    return host
}

/**
 * VAPP-221: one picked file's bytes as the ONE multipart part `file` on
 * `/api/apps/{appId}/files` as the signed-in account (the same quoted part
 * the issue and session uploads send); the answer's `url`. Throws with the
 * server's message on a refusal (caps, storage budget, read access).
 */
private suspend fun uploadAppFile(
    client: HttpClient,
    origin: String?,
    token: String?,
    appId: String,
    bytes: ByteArray,
    filename: String,
    contentType: String,
): String {
    requireNotNull(origin) { "No instance" }
    val (body, boundary) = buildImageUploadBody(bytes, filename, contentType)
    val response = client.post("$origin${Apps.appUploadPath(appId)}") {
        // A large file on a slow uplink needs the file route's budget.
        timeout { requestTimeoutMillis = 300_000 }
        if (token != null) header("Authorization", "Bearer $token")
        setBody(ByteArrayContent(bytes = body, contentType = ContentType.MultiPart.FormData.withParameter("boundary", boundary)))
    }
    val text = response.bodyAsText()
    val answer = runCatching { JsonValue.parse(text) }.getOrNull()
    if (!response.status.isSuccess()) {
        // 412 = the storage cap: its body carries billing copy (EXP-216).
        if (response.status == HttpStatusCode.PreconditionFailed) error("Team storage is full.")
        error(answer?.get("error")?.string ?: answer?.get("message")?.string ?: "HTTP ${response.status.value}")
    }
    return answer?.get("url")?.string ?: error("The upload answered no url")
}

private var mcpRequestId = 0

/**
 * A JSON-RPC `tools/call` on the instance's MCP endpoint as the signed-in
 * account; the consent card has already said yes. Resolves the answer's
 * `result` (a JSON body or the SSE message carrying our id); throws on an
 * HTTP failure, a JSON-RPC `error` or no answer.
 */
private suspend fun callMcp(client: HttpClient, origin: String?, token: String?, tool: String, arguments: JsonValue?, viaApp: ViaAppClaim? = null): Any? {
    requireNotNull(origin) { "MCP: no instance" }
    val id = ++mcpRequestId
    val body = JsonValue.Obj(
        mapOf(
            "jsonrpc" to JsonValue.Str("2.0"),
            "id" to JsonValue.Num(id.toDouble()),
            "method" to JsonValue.Str("tools/call"),
            "params" to JsonValue.Obj(mapOf("name" to JsonValue.Str(tool), "arguments" to (arguments ?: JsonValue.Obj(emptyMap())))),
        ),
    ).json
    val response = client.post("$origin/api/mcp") {
        contentType(ContentType.Application.Json)
        header("Accept", "application/json, text/event-stream")
        if (token != null) header("Authorization", "Bearer $token")
        if (viaApp != null) header(ViaApp.HEADER, viaApp.headerValue)
        setBody(body)
    }
    if (!response.status.isSuccess()) error("MCP ${response.status.value}")
    val text = response.bodyAsText()
    val messages = if (text.trimStart().startsWith("{")) {
        listOf(JsonValue.parse(text))
    } else {
        text.lineSequence().filter { it.startsWith("data:") }.map { JsonValue.parse(it.removePrefix("data:").trim()) }.toList()
    }
    val answer = messages.firstOrNull { it["id"]?.number?.toInt() == id } ?: error("MCP: no answer")
    answer["error"]?.takeIf { it != JsonValue.Null }?.let { error(it["message"]?.string ?: "MCP error") }
    return answer["result"]
}

/** VAPP-201: the host's zone, `TimeZoneOffset(the resolved team zone)` once
 *  the team row says so, else the device's. */
private class TeamHostZone : HostZone {
    @Volatile private var current: HostZone = TimeZoneOffset(java.util.TimeZone.getDefault())

    fun teamZone(name: String?) {
        current = TimeZoneOffset(java.util.TimeZone.getTimeZone(TeamZone.hostZoneId(name)))
    }

    override fun offsetMinutes(epochMs: Double): Int = current.offsetMinutes(epochMs)
}
