package com.exponential.app.ui.apps

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
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
import androidx.compose.runtime.getValue
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
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.FunctionCallInfo
import at.exponential.ui.host.FunctionPolicy
import at.exponential.ui.host.HostExtension
import at.exponential.ui.host.HostFunction
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.MediaRule
import at.exponential.ui.host.SourceResolver
import at.exponential.ui.host.UrlPolicy
import at.exponential.ui.json.JsonValue
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
import com.exponential.app.data.db.AppEntity
import com.exponential.app.data.db.AppRecordEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AppActions
import com.exponential.app.domain.AppHostValues
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.AppSources
import com.exponential.app.domain.Apps
import com.exponential.app.ui.components.failedAction
import com.exponential.app.domain.classifyAppLink
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.CodeBlock
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.markdown.rememberAppLinkHandler
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import io.ktor.client.HttpClient
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * VAPP-110: the Android app as an Exponential UI HOST, the twin of the web
 * app's `lib/exponential-ui-host.tsx` and the IDE's `exponential_ui_host.rs`.
 * Everything goes through the SDK's PUBLIC Compose host API
 * ([ExponentialHost] + [at.exponential.ui.host.HostSurface]), like a third
 * party's host would:
 *  - functions: `harness.toast` (the app's toast stack), `harness.openIssue`
 *    (the synced issue's page, else an error toast), `harness.openDevice` (a
 *    toast with its name, the web's default), `harness.mcp` (a `tools/call` on
 *    the instance's own `/api/mcp` as the signed-in account);
 *  - bindings: the `exp:` scheme over Room (`exp:devices`, re-derived on the
 *    liveness tick), [AppHostValues]; VAPP-119 `exp:issues` / `boards` /
 *    `members` / `sessions` / `comments` / `attachments` over the scope
 *    team's synced rows, [AppSources]; VAPP-122
 *    `exp:records?kind=…` over the hosting app's synced records (or the
 *    `app=<slug>` app of the same team), [AppRecords];
 *  - VAPP-122 `harness.records.upsert {kind, data, id?}` and
 *    `harness.records.delete {id}`: tRPC `apps.records.*` as the viewer, no
 *    consent card (the app's own rows), a refusal = the failed-action toast;
 *  - extensions: the app extension (`packages/ui/exponential-ui/extension.json`)
 *    with Compose painters for its `IconDisc` and `LiveDot` natives (Exponential
 *    ships no built-in apps: only the team's packages are installed);
 *  - VAPP-120 typed actions: every `exponential.<noun>.<op>` of
 *    [AppActions] is the same `tools/call` as the viewer (`{op, …args}`; no
 *    op for the verb tool `exponential.sessions.send`);
 *  - policy: `harness.mcp` and `exponential.*` ASK first (the consent card,
 *    [AppConsentCard]); a typed action the hosting app's synced
 *    `granted_tools` names runs without the card;
 *    urls and media resolve against the instance (attachments with the
 *    account's bearer), every href through the URL policy.
 */

@EntryPoint
@InstallIn(SingletonComponent::class)
interface AppHostEntryPoint {
    fun authRepository(): AuthRepository
    fun databaseHolder(): DatabaseHolder
    fun httpClient(): HttpClient
    fun appsApi(): AppsApi
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
 *  the tool it names, its arguments as compact JSON (a `harness.mcp` call's
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
            val mcp = call.name == "harness.mcp"
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

/** The catalog-update banner above a host's surfaces (a local host has no
 *  transport, so no offline state). */
@Composable
fun AppHostBanner(host: ExponentialHost, modifier: Modifier = Modifier) {
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
 *  registry name (`monitor`), drawn in the current content colour. */
private object AppHostIcons : HostPlugin {
    override fun icon(name: String, size: Float): (@Composable () -> Unit)? {
        val glyph = ExpIcons.byConcept(name) ?: ExpIcons.byName(name) ?: return null
        return { Icon(glyph, contentDescription = null, modifier = Modifier.size(size.dp)) }
    }
}

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

/** The theme every app surface paints with (the web's `DEFAULT_THEME_ID`). */
const val APP_THEME_ID = "exponential"

/**
 * A host over [packages] (package JSON texts), alive while the caller is composed. [onOpenIssue] =
 * `harness.openIssue` for a synced issue (by row id).
 */
@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
@Composable
fun rememberAppHost(
    packages: List<String>,
    consent: AppConsentGate,
    onOpenIssue: (issueId: String) -> Unit,
    /** VAPP-122: the hosting app (its records; null = none: `exp:records` is null, writes fail). */
    appId: String? = null,
    /** The hosting app's team (an `app=<slug>` names an app of this team). */
    teamId: String? = null,
    /** The viewer's user id (`mine=1`). */
    me: String? = null,
    /** The package's `theme` (a built-in id); null = [APP_THEME_ID]. */
    themeId: String? = null,
    /** Release train 2026-10-25: the row's grants are about the HEAD package; false = an older version's preview, which runs with none. */
    grants: Boolean = true,
): ExponentialHost {
    val context = LocalContext.current
    val toaster = LocalToaster.current
    val openIssue by rememberUpdatedState(onOpenIssue)
    // The markdown links' rule for an allowed href (web + iOS alike): an
    // instance link opens in-app, anything else in the Custom Tab; no
    // instance known = the SDK's system opener.
    val links = rememberAppLinkHandler()
    val host = remember(packages, consent, themeId, links, appId, teamId, me, grants) {
        val deps = EntryPointAccessors.fromApplication(context.applicationContext, AppHostEntryPoint::class.java)
        val auth = deps.authRepository()
        val db = accountDatabaseFlow(auth, deps.databaseHolder())
        val devices: Flow<List<DeviceEntity>> = db.scopedQuery(emptyList()) { it.deviceDao().observeAll() }
        val issues: Flow<List<IssueEntity>> = db.scopedQuery(emptyList()) { it.issueDao().observeAll() }
        val users: Flow<List<UserEntity>> = db.scopedQuery(emptyList()) { it.userDao().observeAll() }
        // VAPP-120: the hosting app's row, read at call time for its grants
        // (the web/desktop settings page may change them while the host lives).
        val hostingApp: Flow<AppEntity?> =
            if (appId == null || !grants) flowOf(null) else db.scopedQuery(null) { it.appDao().observeById(appId) }
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
        fun sourceStore(name: String): Flow<AppSources.Store> = when (name) {
            "issues" -> combine(teamBoards, teamIssues, sourceUsers) { b, i, u -> AppSources.Store(boards = b, issues = i, users = u) }
            "boards" -> teamBoards.map { AppSources.Store(boards = it) }
            "members" -> combine(teamMembers, sourceUsers) { m, u -> AppSources.Store(members = m, users = u) }
            "sessions" -> teamSessions.map { AppSources.Store(sessions = it) }
            "comments" -> combine(teamIssues, teamComments, sourceUsers) { i, c, u -> AppSources.Store(issues = i, comments = c, users = u) }
            "attachments" -> combine(teamIssues, teamAttachments, sourceUsers) { i, a, u -> AppSources.Store(issues = i, attachments = a, users = u) }
            else -> flowOf(AppSources.Store())
        }
        val origin = auth.instanceUrl.value?.trimEnd('/')
        val ownHost = origin?.let { runCatching { java.net.URI(it).host }.getOrNull() }?.let { listOf(it) }
        val token = origin?.let { resolveAccountForUrl(auth.accounts.value, auth.activeAccountId.value, it)?.token }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

        val exp = SourceResolver { source, emit ->
            val values: Flow<JsonValue?> = when (source.name) {
                "devices" -> combine(devices, DeviceLiveness.ticker(AppHostValues.LIVENESS_TICK_MS)) { rows, now ->
                    AppHostValues.devices(rows, now)
                }
                in AppSources.SOURCES -> sourceStore(source.name).map { store ->
                    AppSources.sourceValue(source.name, source.params, store, me)
                }
                "records" -> {
                    val query = AppRecords.parseRecordsQuery(source.params)
                    // The app whose records are read: the hosting app, or
                    // (`app=`) another app of the same team by slug (it may
                    // sync later, so the lookup follows the apps).
                    val target: Flow<String?> = if (query.app == null) {
                        flowOf(appId)
                    } else {
                        teamApps.map { rows -> rows.firstOrNull { it.slug == query.app }?.id }.distinctUntilChanged()
                    }
                    val rows = target.flatMapLatest { recordsOf(it) }
                    combine(rows, users) { records, people ->
                        val mapped = records.map(AppRecords::row)
                        val wanted = mapped.mapNotNullTo(HashSet()) { it.createdBy }
                        val names = people.filter { it.id in wanted }.associate { it.id to it.name.orEmpty() }
                        AppRecords.recordsValue(mapped, query, me, AppRecords.isoNow(), names)
                    }
                }
                else -> flowOf(null)
            }
            val job = scope.launch { values.distinctUntilChanged().collect { emit(it) } }
            return@SourceResolver { job.cancel() }
        }

        val toast: HostFunction = { args, _ ->
            val text = args["message"]?.string.orEmpty()
            if (args["tone"]?.string == "error") toaster.error(text) else toaster.info(text)
        }
        val openIssueFn: HostFunction = { args, _ ->
            val identifier = args["identifier"]?.string.orEmpty()
            val issue = issues.first().firstOrNull { it.identifier == identifier }
            if (issue != null) openIssue(issue.id) else toaster.error("$identifier is not synced here")
        }
        val openDevice: HostFunction = { args, _ -> toaster.info(args["label"]?.string ?: args["id"]?.string.orEmpty()) }
        val mcp: HostFunction = { args, _ ->
            callMcp(deps.httpClient(), origin, token, args["tool"]?.string.orEmpty(), args["arguments"])
        }
        // VAPP-122: the app's own rows (no consent card); a refusal (the
        // kind's schema, the caps, the plan quota) is a failed action.
        val api = deps.appsApi()
        val recordsUpsert: HostFunction = { args, _ ->
            try {
                val account = auth.activeAccountId.value ?: error("Not signed in")
                val app = appId ?: error("No app to keep records for")
                val data = (args["data"] as? JsonValue.Obj) ?: JsonValue.Obj(emptyMap())
                val result = api.recordsUpsert(
                    account,
                    app,
                    kind = args["kind"]?.string.orEmpty(),
                    data = kotlinx.serialization.json.Json.parseToJsonElement(data.orderedJson) as kotlinx.serialization.json.JsonObject,
                    recordId = args["id"]?.string,
                )
                JsonValue.Obj(mapOf("id" to JsonValue.Str(result.id)))
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                toaster.failedAction("save", "the record", e.message)
                throw e
            }
        }
        val recordsDelete: HostFunction = { args, _ ->
            try {
                val account = auth.activeAccountId.value ?: error("Not signed in")
                val app = appId ?: error("No app to keep records for")
                api.recordsDelete(account, app, args["id"]?.string.orEmpty())
                null
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                toaster.failedAction("delete", "the record", e.message)
                throw e
            }
        }
        // VAPP-120: every MCP operation as a typed function
        // (fixtures/app-actions.json): the same tools/call as the viewer,
        // `op` + the args (no op for a verb tool).
        val actions: Map<String, HostFunction> = AppActions.TOOLS.mapValues { (_, target) ->
            val fn: HostFunction = { args, _ ->
                callMcp(deps.httpClient(), origin, token, target.tool, JsonValue.Obj(AppActions.arguments(target, args)))
            }
            fn
        }
        val functions = actions + mapOf(
            "harness.records.upsert" to recordsUpsert,
            "harness.records.delete" to recordsDelete,
            "harness.toast" to toast,
            "harness.openIssue" to openIssueFn,
            "harness.openDevice" to openDevice,
            "harness.mcp" to mcp,
        )

        ExponentialHost(
            HostOptions(
                functions = functions,
                sources = mapOf("exp" to exp),
                extensions = listOf(
                    HostExtension(
                        AppHostAssets.extensionJson(context),
                        mapOf("IconDisc" to IconDiscPainter, "LiveDot" to LiveDotPainter),
                    ),
                ),
                packages = packages,
                policy = HostPolicy(
                    functions = FunctionPolicy(ask = listOf("harness.mcp", "exponential.*")),
                    // A granted typed action runs without the card; anything else asks.
                    onFunctionCall = { call ->
                        val granted = AppActions.parseActionName(call.name) != null &&
                            AppActions.grantAllows(AppActions.grantedList(hostingApp.first()?.grantedTools), call.name, call.args)
                        granted || consent.ask(call)
                    },
                    // VAPP-107: private hosts are denied unless opted in; the
                    // app's OWN instance is (the localhost dev setup), nothing
                    // else. A public origin listed here changes nothing.
                    urls = UrlPolicy(baseUrl = origin, allowPrivate = ownHost),
                    openUrl = links?.let { handler -> { url: String -> handler.onOpen(classifyAppLink(url, handler.origin), url) } },
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
                plugin = AppHostIcons,
            ),
            scope,
        )
    }
    DisposableEffect(host) { onDispose { host.dispose() } }
    return host
}

private var mcpRequestId = 0

/**
 * A JSON-RPC `tools/call` on the instance's MCP endpoint as the signed-in
 * account; the consent card has already said yes. Resolves the answer's
 * `result` (a JSON body or the SSE message carrying our id); throws on an
 * HTTP failure, a JSON-RPC `error` or no answer.
 */
private suspend fun callMcp(client: HttpClient, origin: String?, token: String?, tool: String, arguments: JsonValue?): Any? {
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
