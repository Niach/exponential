package com.exponential.app.navigation

import androidx.activity.compose.BackHandler
import android.net.Uri
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.AnimatedContentTransitionScope.SlideDirection
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.exponential.app.AppConstants
import io.sentry.android.navigation.SentryNavigationListener
import com.exponential.app.AppViewModel
import com.exponential.app.ExponentialApp
import com.exponential.app.data.ChatShare
import com.exponential.app.data.ShellState
import com.exponential.app.domain.Chats
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.SettingsNav
import com.exponential.app.data.electric.SyncHealth
import androidx.browser.customtabs.CustomTabsIntent
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.push.InAppNotifications
import com.exponential.app.domain.NotificationDelivery
import com.exponential.app.domain.ToastAction
import com.exponential.app.data.push.PushDeepLinks
import com.exponential.app.domain.InboxRoute
import com.exponential.app.domain.AppShortcuts
import com.exponential.app.domain.WidgetLinks
import com.exponential.app.data.push.WebLinkResolver
import com.exponential.app.domain.AGENT_ROUTE
import com.exponential.app.domain.AGENT_ROUTE_ARGS
import com.exponential.app.domain.AGENT_ROUTE_PATTERN
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.LINK_NO_ACCESS
import com.exponential.app.domain.LinkOpenOutcome
import com.exponential.app.domain.agentRoute
import com.exponential.app.domain.linkOpenOutcome
import com.exponential.app.ui.agent.AgentScreen
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.BottomBarSuppression
import com.exponential.app.ui.components.LocalPageHeaderActions
import com.exponential.app.ui.components.PageHeaderActions
import com.exponential.app.ui.components.LocalBottomBarSuppression
import com.exponential.app.ui.components.LocalOverlayToaster
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.PhoneDock
import com.exponential.app.ui.components.PhoneNewMenu
import com.exponential.app.ui.components.ToastHost
import com.exponential.app.ui.components.aboveDock
import com.exponential.app.ui.components.BlockingOverlayHost
import com.exponential.app.ui.components.Toaster
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.invite.InviteAcceptScreen
import com.exponential.app.ui.issue.IssueDraftScreen
import com.exponential.app.ui.onboarding.OnboardingScreen
import com.exponential.app.ui.personal.PersonalScreen
import com.exponential.app.ui.issue.IssueListScreen
import com.exponential.app.ui.search.SearchScreen
import com.exponential.app.ui.chat.ChatScreen
import com.exponential.app.ui.plan.LocalOpenPlanPage
import com.exponential.app.ui.plan.OpenPlanPage
import com.exponential.app.ui.chats.NeedsYouPage
import com.exponential.app.ui.chats.ChatsScreen
import com.exponential.app.ui.chats.ChatsViewModel
import com.exponential.app.ui.session.AgentSessionScreen
import com.exponential.app.ui.issue.IssueScreen
import com.exponential.app.ui.settings.AgentSettingsScreen
import com.exponential.app.ui.settings.AssistantSettingsRoute
import com.exponential.app.ui.settings.McpActivityScreen
import com.exponential.app.ui.settings.McpSettingsScreen
import com.exponential.app.ui.settings.PromptSettingsScreen
import com.exponential.app.ui.settings.PromptsSettingsScreen
import com.exponential.app.ui.settings.SkillSettingsScreen
import com.exponential.app.ui.settings.SkillsSettingsScreen
import com.exponential.app.ui.chat.AgentConversationScreen
import com.exponential.app.ui.apps.AppEditorScreen
import com.exponential.app.ui.apps.AppRunScreen
import com.exponential.app.ui.settings.AgentsSettingsScreen
import com.exponential.app.ui.session.AgentsScreen
import com.exponential.app.ui.settings.AboutScreen
import com.exponential.app.ui.settings.ServerDetailScreen
import com.exponential.app.ui.settings.VoiceSettingsScreen
import com.exponential.app.ui.settings.SecuritySettingsScreen
import com.exponential.app.ui.settings.SettingsScreen
import com.exponential.app.ui.settings.TeamSettingsRootScreen
import com.exponential.app.ui.teams.TeamsScreen
import com.exponential.app.ui.settings.SyncDiagnosticsScreen
import com.exponential.app.ui.settings.TeamSettingsScreen
import com.exponential.app.ui.settings.ThirdPartyLicensesScreen
import com.exponential.app.ui.share.ShareTargetPickerViewModel
import com.exponential.app.ui.share.ShareToScreen
import com.exponential.app.ui.share.buildSharePrefill
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.update.UpdateRequiredScreen
import dagger.hilt.android.EntryPointAccessors

/**
 * The single navigation surface, mirroring the iOS `AppNavigator`: a gradient
 * [AppBackground] behind one push-stack `NavHost`, with the docked bar (#EXP-2109:
 * Chats · Teams · Assistant · New · Settings) over the pages `shell.pages`
 * lists; the app lands on Chats. EXP-1210: the tabs are ROOT siblings
 * ([MainTabs], [selectTab]) — no back button, no slide between them. There is
 * no active team: a screen's team is derived from what it shows.
 */
@Composable
fun AppNavHost() {
    val viewModel: AppViewModel = hiltViewModel()
    val deepLinkBus = applicationDeepLinkBus()
    val shell = applicationShellState()
    val webLinkResolver = applicationWebLinkResolver()
    val state by viewModel.state.collectAsStateWithLifecycle()
    // No session yet (see below). #EXP-1694: each graph has its own controller:
    // handing the signed-in back stack to the signed-out graph crashed Sign out
    // ("a tab is not a direct child of this NavGraph").
    val needsAuth =
        state.accounts.isEmpty() || state.instanceUrl == null || state.token == null
    val navController = key(needsAuth) { rememberNavController() }
    // #LOGS-2: navigation breadcrumbs + transactions (inert while Sentry is off).
    DisposableEffect(navController) {
        val listener = SentryNavigationListener()
        navController.addOnDestinationChangedListener(listener)
        onDispose { navController.removeOnDestinationChangedListener(listener) }
    }
    val pendingTarget by deepLinkBus.target.collectAsStateWithLifecycle()
    val context = LocalContext.current
    // EXP-1212: the New issue page holds navigation while its draft has
    // content; a held navigation drops the page first, so Back never
    // returns to a draft that has already been created, kept or discarded.
    val leaveGuard = remember { LeaveGuard() }
    val guarded: (() -> Unit) -> Unit = { navigation ->
        leaveGuard.navigate(beforeHeld = { navController.popBackStack() }, navigation = navigation)
    }
    // EXP-1210: the tab switch in flight, drawn with no transition.
    val tabSwitch = remember { TabSwitchMarker() }

    val startDestination = when {
        state.instanceUrl == null -> "instance"
        state.token == null -> "login"
        else -> MainTabs.CHATS
    }

    // EXP-1031: the app's ONE toaster, above both nav graphs, drawn by ONE
    // ToastHost at the top (below the status bar), where no bottom sheet,
    // nav bar or keyboard ever covers it.
    val toaster = remember { Toaster() }
    // #EXP-1815: a push that lands while the app is up is THE toast (kind
    // info, its title and body, `Open` = the push tap's route); FcmService
    // posts nothing to the shade then.
    val inAppNotifications = applicationInAppNotifications()
    LaunchedEffect(inAppNotifications, toaster) {
        inAppNotifications.toasts.collect { push ->
            toaster.info(
                push.title,
                push.body,
                ToastAction(NotificationDelivery.Copy.TOAST_ACTION) {
                    runCatching { context.startActivity(push.open) }
                },
            )
        }
    }

    // EXP-980 / #EXP-1456: a session push tap or inbox row (a wall, an
    // agent's decision, an @mention) opens where that session lives
    // (fixtures/inbox-route.json): my agent conversation = its channel, a
    // chat root = the chat, else the session view. Same snapshot hazard
    // as the issue route — the screens read their id from SavedStateHandle
    // once. #EXP-1638: a recent-chat app shortcut rides the same open.
    suspend fun openSession(id: String, card: PushDeepLinks.Card? = null, onRoot: Boolean = false) {
        // #EXP-1662: a session with a waiting plan of its own = its
        // chat's plan tab, on that plan. waiting-cards.json DEEP LINKS: a
        // push's card (its `messageId`), else the session's waiting
        // question, lands its chat on that card.
        val target = webLinkResolver.sessionTarget(id)
        val root = webLinkResolver.planChat(id) ?: card?.rootId
        val landing = when {
            card != null -> InboxRoute.landing(target, root, card.messageId, card.plan)
            else -> webLinkResolver.planPageTarget(id)?.let { InboxRoute.landing(target, root, it, plan = true) }
                ?: InboxRoute.landing(target, root, webLinkResolver.waitingCard(id), plan = false)
        }
        val route = landingRoute(landing) ?: sessionTargetRoute(id, target)
        guarded {
            // #EXP-1830: a shortcut or widget lands by THE back stack rule.
            if (onRoot) navController.launchOnRoot(route, tabSwitch) else navController.navigateDeepLink(route, tabSwitch)
        }
    }

    LaunchedEffect(pendingTarget, state.token) {
        val target = pendingTarget ?: return@LaunchedEffect
        // Leave the target in the bus while unauthenticated so a share/deep-link
        // received before login resumes once the token lands (token is a key).
        if (state.token == null) return@LaunchedEffect
        // A cold start's push for the Assistant lands on its tab too.
        AssistantTab.ref = withTimeoutOrNull(ASSISTANT_WAIT_MS) { viewModel.assistantNow() }
        when (target) {
            // A real push, never launchSingleTop (EXP-528): single top REPLACES
            // the top entry and the replacement keeps its id, so its
            // ViewModelStore — and an IssueDetailViewModel that read issueId
            // once from SavedStateHandle — survived, and a tap that arrived
            // while ANOTHER issue was open re-showed that issue. navigateDeepLink
            // keeps the only thing single top bought us: a re-tap for what is
            // already on screen stays a no-op.
            is DeepLinkBus.Target.Issue -> {
                guarded {
                    if (target.rooted) {
                        navController.launchIssueOnRoot(target.id, tabSwitch, target.comment)
                    } else {
                        navController.navigateIssueDeepLink(target.id, target.comment)
                    }
                }
            }
            is DeepLinkBus.Target.Invite -> guarded {
                navController.navigateDeepLink("invite/${target.token}")
            }
            // An agent's message (EXP-801) renders in the inbox — chat
            // 58cb32df: a page pushed over the Chats tab, not a tab root.
            DeepLinkBus.Target.Inbox -> guarded {
                navController.selectTab(MainTabs.CHATS, tabSwitch, restore = false)
                navController.navigate(MainTabs.INBOX) { launchSingleTop = true }
            }
            // #ERR-27: a notification tap lands by THE back stack rule, so
            // chat after chat opened from the shade never stacks (each kept
            // its ChatViewModel, its tree and its transcript alive).
            is DeepLinkBus.Target.Session -> openSession(target.id, target.card, onRoot = target.rooted)
            // #EXP-1638 (app-shortcuts.json): the app icon's long-press menu,
            // onto the pages the app already has.
            // #EXP-1830 THE BACK STACK RULE: every entry lands on the Chats
            // root, its target pushed over it.
            is DeepLinkBus.Target.AppShortcut -> when (val shortcut = target.target) {
                // A chat that is gone (or another account's): the start screen.
                is AppShortcuts.Target.Chat ->
                    if (webLinkResolver.sessionTarget(shortcut.id) != null) {
                        openSession(shortcut.id, onRoot = true)
                    } else {
                        guarded { navController.selectTab(MainTabs.CHATS, tabSwitch, restore = false) }
                    }
                // #EXP-1830: THE agent's conversation (the caller's DM;
                // none yet = the empty DM, whose composer creates it).
                is AppShortcuts.Target.Agent -> guarded {
                    navController.launchOnRoot(Routes.agentDm(shortcut.id), tabSwitch)
                }
                // THE New issue page, on the last-used board (its picker
                // switches); a cold start waits a beat for the board to load.
                AppShortcuts.Target.NewIssue -> {
                    val boardId = viewModel.newIssueBoardId.value
                        ?: withTimeoutOrNull(BOARD_WAIT_MS) { viewModel.newIssueBoardId.filterNotNull().first() }
                    if (boardId != null) {
                        guarded {
                            navController.selectTab(MainTabs.CHATS, tabSwitch, restore = false)
                            navController.openIssueDraft(boardId)
                        }
                    }
                }
                // THE composer page, its default agent row.
                AppShortcuts.Target.NewChat -> guarded {
                    deepLinkBus.requestComposerFocus()
                    navController.launchOnRoot(AGENT_ROUTE, tabSwitch)
                }
                AppShortcuts.Target.Search -> guarded {
                    navController.launchOnRoot("search", tabSwitch)
                }
            }
            // #EXP-1830 (home-widgets.json TAPS): a home widget's header or
            // row, by THE back stack rule.
            is DeepLinkBus.Target.Widget -> when (val widget = target.target) {
                WidgetLinks.Target.Home -> Unit
                // My issues stands on a board's chips: the last-used board's.
                WidgetLinks.Target.MyIssues -> {
                    val boardId = viewModel.newIssueBoardId.value
                        ?: withTimeoutOrNull(BOARD_WAIT_MS) { viewModel.newIssueBoardId.filterNotNull().first() }
                    if (boardId != null) {
                        guarded {
                            deepLinkBus.requestMyIssues()
                            navController.launchOnRoot(Routes.board(boardId), tabSwitch)
                        }
                    }
                }
                WidgetLinks.Target.NeedsYou -> guarded {
                    navController.launchOnRoot(Routes.CHATS_NEEDS_YOU, tabSwitch)
                }
                is WidgetLinks.Target.Board -> guarded {
                    navController.launchOnRoot(Routes.board(widget.id), tabSwitch)
                }
                is WidgetLinks.Target.Issue -> guarded {
                    navController.launchIssueOnRoot(widget.id, tabSwitch)
                }
                is WidgetLinks.Target.NeedsYouRow -> guarded {
                    navController.launchOnRoot(
                        Routes.needsYou(widget.rootId, widget.messageId, widget.opens),
                        tabSwitch,
                    )
                }
            }
            // #EXP-1534: an agent_signed_out push tap or inbox row = the
            // Devices page with that device's settings open
            // (fixtures/inbox-route.json `device`).
            is DeepLinkBus.Target.Device -> guarded {
                if (target.rooted) {
                    navController.launchOnRoot(Routes.deviceSettings(target.deviceId), tabSwitch)
                } else {
                    navController.navigateDeepLink(Routes.deviceSettings(target.deviceId))
                }
            }
            // #EXP-1674: ONE LINK SYSTEM: a tapped link and a verified App Link
            // alike, by the one open rule (app-link.json `openCases`).
            is DeepLinkBus.Target.Link -> {
                val resolved = webLinkResolver.resolveLink(target.link, target.host)
                // #EXP-2109: every member team is in scope; only another
                // server's account is switched to.
                val screen = resolved.destination
                when (linkOpenOutcome(resolved.member, resolved.accountId == state.activeAccountId, resolved.found, resolved.webOnly)) {
                    LinkOpenOutcome.NoAccess -> {
                        toaster.error(LINK_NO_ACCESS)
                        guarded { navController.selectTab(MainTabs.CHATS, tabSwitch, restore = false) }
                    }
                    LinkOpenOutcome.Browser ->
                        CustomTabsIntent.Builder().build().launchUrl(context, target.uri)
                    // A screen with its own not-found state, else the toast.
                    LinkOpenOutcome.NotFound ->
                        if (screen != null) {
                            guarded { navController.openLinkDestination(screen, target.uri, tabSwitch) }
                        } else {
                            toaster.error(LINK_NO_ACCESS)
                        }
                    LinkOpenOutcome.SwitchTeam -> guarded {
                        resolved.accountId?.let(viewModel::switchAccount)
                        screen?.let { navController.openLinkDestination(it, target.uri, tabSwitch) }
                    }
                    LinkOpenOutcome.Open -> guarded {
                        screen?.let { navController.openLinkDestination(it, target.uri, tabSwitch) }
                    }
                }
            }
            // A web source: straight to the Custom Tab, which never re-triggers
            // App Links.
            is DeepLinkBus.Target.WebUrl ->
                CustomTabsIntent.Builder().build().launchUrl(context, target.uri)
            is DeepLinkBus.Target.ShareContent -> {
                // EXP-1635 (share-targets.json): a sharing shortcut skips Share
                // to; an id this viewer can no longer follow lands on it.
                val direct = target.directShareId?.let { viewModel.resolveDirectShare(it) }
                guarded {
                    when {
                        direct?.sessionId != null -> {
                            shell.setPendingChatShare(ChatShare(target, sessionId = direct.sessionId))
                            navController.navigateDeepLink(Routes.chat(direct.sessionId), tabSwitch)
                        }
                        direct?.pick != null -> {
                            shell.setPendingChatShare(ChatShare(target, pick = direct.pick, teamId = direct.teamId))
                            navController.navigateDeepLink(AGENT_ROUTE)
                        }
                        else -> {
                            // The pending share lives in the ShellState singleton.
                            shell.setPendingShare(target)
                            navController.navigate(Routes.SHARE_TO) { launchSingleTop = true }
                        }
                    }
                }
            }
        }
        deepLinkBus.consume()
    }

    val cloudAlreadyAdded = state.accounts.any { it.instanceUrl == AppConstants.PUBLIC_CLOUD_URL }

    // Show the unauthenticated flow whenever the active account has no usable
    // session: no accounts at all, no instance chosen yet, or an account that
    // exists but isn't logged in (just added, signed out, or a cleared/expired
    // token). Without this gate the home shell mounts and fires authed requests
    // with no Authorization header, which 401 immediately.

    // Gate the authenticated graph on onboarding: a brand-new user (the session
    // read at login explicitly reported no onboardingCompletedAt) starts in the
    // wizard. Persisted, so it resolves synchronously at startup; AuthenticatedNav
    // re-routes to the wizard if an account switch lands on a not-yet-onboarded
    // account. Accounts persisted before the flag existed never re-enter the
    // wizard (ServerAccount.needsOnboarding requires onboardingKnown).
    val activeAccount = state.accounts.firstOrNull { it.id == state.activeAccountId }
    val needsOnboarding = activeAccount?.needsOnboarding == true

    AppBackground {
        // Every screen floats on AppBackground (a Box, not a Material Surface), so
        // without this provider bare `Text`/`Icon` would inherit LocalContentColor's
        // black default and render near-invisible on the dark gradient. Anchor the
        // default to onSurface (light) app-wide; explicit colors still win.
        CompositionLocalProvider(
            LocalContentColor provides MaterialTheme.colorScheme.onSurface,
            LocalToaster provides toaster,
            // FEED-79: sheets and alerts mount their own host off it.
            LocalOverlayToaster provides toaster,
        ) {
        // VAPP-235: a later outbox replay's refusal, toasted wherever the member is.
        com.exponential.app.ui.apps.AppOutboxRefusals()
        val updateRequired = state.updateRequired
        if (updateRequired != null) {
            // Highest priority: the ACTIVE account's server has 426'd this
            // build (below its minimum version, EXP-104). Replace the whole
            // NavHost with the blocking update screen — no navigation, no
            // authed requests. Scoped per instance (REV2-18): a background
            // account's 426 gets the banner below instead of this screen.
            UpdateRequiredScreen(
                info = updateRequired,
                serverLabel = activeAccount?.displayName,
                onSignOutOfServer = { viewModel.signOutOfGatedServer() },
            )
        } else if (needsAuth) {
            UnauthenticatedNav(
                navController = navController,
                startDestination = startDestination,
                onInstanceSet = { url ->
                    viewModel.setInstanceUrl(url)
                    navController.navigate("login") { popUpTo("instance") { inclusive = true } }
                },
                // #EXP-1597: no navigation here. This graph has no Agent
                // route: the token landing flips `needsAuth`, and the
                // authenticated graph mounts on its own start (Agent or the
                // onboarding wizard). AppViewModel.state is collected off the
                // main thread (#1540), so the login screen's success can
                // land a frame BEFORE the token does, and navigating to Agent
                // from this graph threw IllegalArgumentException.
                onLogin = {},
                onChangeInstance = {
                    viewModel.clearInstance()
                    navController.navigate("instance") { popUpTo("login") { inclusive = true } }
                },
                instanceUrl = state.instanceUrl ?: "",
                cloudAlreadyAdded = cloudAlreadyAdded,
            )
        } else {
            // Feature ViewModels scope to the active account reactively
            // (accountDatabaseFlow + flatMapLatest), so an account switch
            // re-scopes every live screen in place — no key(activeAccountId)
            // rebuild, no pending-handoff flags.
            val unreadCount by viewModel.unreadCount.collectAsStateWithLifecycle()
            val newIssueBoardId by viewModel.newIssueBoardId.collectAsStateWithLifecycle()
            val assistant by viewModel.assistant.collectAsStateWithLifecycle()
            LaunchedEffect(assistant) { AssistantTab.ref = assistant }
            val gatedOtherServers by viewModel.gatedOtherServers.collectAsStateWithLifecycle()
            val syncHealth by viewModel.syncHealth.collectAsStateWithLifecycle()
            AuthenticatedNav(
                navController = navController,
                leaveGuard = leaveGuard,
                guarded = guarded,
                tabSwitch = tabSwitch,
                cloudAlreadyAdded = cloudAlreadyAdded,
                activeAccountId = state.activeAccountId,
                gatedOtherServers = gatedOtherServers,
                syncHealth = syncHealth,
                needsOnboarding = needsOnboarding,
                unreadCount = unreadCount,
                newIssueBoardId = newIssueBoardId,
                assistant = viewModel.assistant,
                onSetInstanceUrl = { viewModel.setInstanceUrl(it) },
                onRetrySync = { viewModel.retrySync() },
            )
            // NEW-69: a running voice call, full screen over every screen.
            // FEED-272: its Chat tab opens the agent's chat when none shows.
            com.exponential.app.ui.call.VoiceCallHost(onOpenChat = { id -> navController.open(Routes.agentDm(id), tabSwitch) })
        }
        // Drawn last, so it overlays every screen's header (sonner
        // top-center does the same); only the cards take touches.
        ToastHost(toaster, Modifier.align(Alignment.TopCenter))
        // EXP-1275 (D8.5): the window's ONE blocking overlay — switching
        // account, signing out — over everything, toasts included.
        BlockingOverlayHost()
        }
    }
}

@Composable
private fun UnauthenticatedNav(
    navController: NavHostController,
    startDestination: String,
    instanceUrl: String,
    onInstanceSet: (String) -> Unit,
    onLogin: () -> Unit,
    onChangeInstance: () -> Unit,
    cloudAlreadyAdded: Boolean,
) {
    NavHost(navController = navController, startDestination = startDestination) {
        composable("instance") {
            InstanceScreen(
                onContinue = onInstanceSet,
                showCancel = false,
                onCancel = null,
                cloudAlreadyAdded = cloudAlreadyAdded,
            )
        }
        composable("login") {
            LoginScreen(
                instanceUrl = instanceUrl,
                onLoggedIn = onLogin,
                onChangeInstance = onChangeInstance,
            )
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AuthenticatedNav(
    navController: NavHostController,
    leaveGuard: LeaveGuard,
    guarded: (() -> Unit) -> Unit,
    tabSwitch: TabSwitchMarker,
    cloudAlreadyAdded: Boolean,
    activeAccountId: String?,
    gatedOtherServers: List<String>,
    syncHealth: SyncHealth,
    needsOnboarding: Boolean,
    unreadCount: Int,
    newIssueBoardId: String?,
    assistant: kotlinx.coroutines.flow.StateFlow<AssistantRef?>,
    onSetInstanceUrl: (String) -> Unit,
    onRetrySync: () -> Unit,
) {
    val shell = applicationShellState()
    val planLinks = applicationWebLinkResolver()
    val planScope = rememberCoroutineScope()
    val openPlanTab = remember(navController) {
        OpenPlanPage { sessionId, messageId ->
            planScope.launch {
                val root = planLinks.planChat(sessionId)
                navController.open(root?.let { Routes.chatPlan(it, messageId) } ?: Routes.session(sessionId), tabSwitch)
            }
        }
    }

    // NavHost only evaluates startDestination once, so an account switch onto a
    // not-yet-onboarded account (possible when a login was killed mid-wizard)
    // must re-route explicitly. launchSingleTop makes this a no-op when the
    // wizard is already showing (e.g. right after a fresh login).
    LaunchedEffect(needsOnboarding) {
        if (needsOnboarding) {
            navController.navigate("onboarding") {
                popUpTo(0) { inclusive = true }
                launchSingleTop = true
            }
        }
    }

    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = backStackEntry?.destination?.route
    // #EXP-1815: the screen on top's thread, for FcmService's presentation.
    val inAppNotifications = applicationInAppNotifications()
    DisposableEffect(backStackEntry) {
        inAppNotifications.setViewing(
            ViewingThreads.of(currentRoute) { name ->
                if (currentRoute == MainTabs.ASSISTANT) assistant.value?.sessionId.takeIf { name == "sessionId" }
                else backStackEntry?.arguments?.getString(name)
            },
        )
        onDispose { }
    }
    DisposableEffect(Unit) { onDispose { inAppNotifications.setViewing(null) } }
    // #EXP-2109 (`shell.pages`): the tab the page lights and whether the bar
    // shows; a page not listed is a detail: no bar.
    val page = Chats.Shell.page(MainTabs.page(currentRoute))
    val barVisible = !needsOnboarding && page.bar
    // EXP-698 r5 (Mechanism A): a screen may claim the tab bar's slot for a
    // bar of its own — today the issue list's multi-select bar. The switch is
    // provided to the whole NavHost; the chrome below reads it directly.
    val barSuppression = remember { BottomBarSuppression() }
    // The keyboard takes the bar's place (the Assistant's composer).
    val barShown = barVisible && !barSuppression.suppressed && !WindowInsets.isImeVisible

    // #FEED-232: the Chats tab's count badge (`chatsBadge`), over every team.
    val chatsVm: ChatsViewModel = hiltViewModel()
    val chatsBadge by chatsVm.chatsBadge.collectAsStateWithLifecycle()
    // EXP-825: every launcher entry point is NAVIGATION onto the composer page
    // with a preselection seed — every play button with what it acts on.
    // The seed rides the route string, so the concrete route differs per seed:
    // a tap reuses the deep-link rule — a no-op when that exact route is
    // already on top, else a NEW entry whose ViewModel reads the new seed
    // (single top would keep the old entry's ViewModel, which read its seed
    // once — EXP-528).
    val openAgent: (AgentComposerSeed) -> Unit = { seed -> navController.navigateDeepLink(agentRoute(seed)) }
    // EXP-1121: the "Ready to code?" fixes: the issue's team's settings page
    // and Devices, PUSHED over the issue, so Back returns to Start coding.
    val openReadinessTeamSettings: (String, String) -> Unit = { teamId, section ->
        navController.navigate(Routes.teamSettings(teamId, section)) { launchSingleTop = true }
    }
    val openReadinessDevices: () -> Unit = {
        navController.navigate(Routes.DEVICES) { launchSingleTop = true }
    }

    // New › Issue (`shell.newMenu`): the board on screen, else the last-used
    // one, else the first board of the first team.
    val composeBoardId = when (currentRoute) {
        Routes.BOARD_PATTERN -> backStackEntry?.arguments?.getString("boardId")
        else -> newIssueBoardId
    }
    var newOpen by remember { mutableStateOf(false) }
    // Leaving the page (or the bar hiding) closes the popup.
    LaunchedEffect(backStackEntry?.id, barShown) { newOpen = false }
    // The page header's circles (`shell.header.buttons`), the same on every
    // page: search pushes the phone search, the inbox the inbox page.
    val pageHeaderActions = PageHeaderActions(
        onSearch = { navController.navigate("search") { launchSingleTop = true } },
        onInbox = { navController.navigate(MainTabs.INBOX) { launchSingleTop = true } },
        inboxUnread = unreadCount,
    )

    // EXP-523: the four transitions below are plain lambdas, not composable
    // ones, so the reduce-motion flag is read here and captured. `Motion.slow`
    // is the shared 280ms token these were already hand-set to — the only
    // behaviour change is that a user who turned animations off now gets none.
    val reduceMotion = LocalReduceMotion.current
    val pushSpec = Motion.slow<IntOffset>(reduceMotion)

    // EXP-920: the ONE navigator an entity-preview sheet's Open rides — every
    // kind's detail surface behind one local, so the transcript never chats
    // a lambda per kind.
    val entityNavigator = remember(navController, tabSwitch) {
        EntityNavigator { target -> guarded {
            when (target) {
                is EntityTarget.Issue -> navController.navigate(issueRoute(target.id, commentId = target.commentId))
                is EntityTarget.Board -> navController.navigate(Routes.board(target.id))
                is EntityTarget.Session -> navController.navigate(Routes.session(target.id))
                // NEW-7: a `prompt` ref opens its Settings page.
                is EntityTarget.Prompt -> navController.navigate(Routes.promptSettings(target.id))
                is EntityTarget.Prompts -> navController.openTeamSettings(target.teamId, SettingsNav.PROMPTS, tabSwitch)
                // NEW-7: an `agent` ref opens my conversation with it.
                is EntityTarget.Agent -> navController.open(Routes.agentDm(target.id), tabSwitch)
                EntityTarget.Devices -> navController.navigate(Routes.DEVICES) { launchSingleTop = true }
                is EntityTarget.TeamSettings -> navController.openTeamSettings(target.teamId, target.section, tabSwitch)
                is EntityTarget.Team -> target.teamId?.let { navController.openTeam(it, tabSwitch) }
                    ?: navController.selectTab(MainTabs.TEAMS, tabSwitch)
                // Session 58cb32df: the inbox is a page, pushed.
                EntityTarget.Inbox -> navController.navigate(MainTabs.INBOX) { launchSingleTop = true }
            }
        } }
    }

    // The NavHost builder below must capture nothing that changes: a new
    // builder lambda rebuilds the whole route graph, and this shell
    // recomposes on every navigation (Back included) and sync tick.
    val activeAccountIdLatest = rememberUpdatedState(activeAccountId)
    val cloudAlreadyAddedLatest = rememberUpdatedState(cloudAlreadyAdded)
    val unreadCountLatest = rememberUpdatedState(unreadCount)
    val onSetInstanceUrlLatest = rememberUpdatedState(onSetInstanceUrl)
    Box(modifier = Modifier.fillMaxSize()) {
    CompositionLocalProvider(
        LocalBottomBarSuppression provides barSuppression,
        LocalEntityNavigator provides entityNavigator,
        LocalLeaveGuard provides leaveGuard,
        LocalPageHeaderActions provides pageHeaderActions,
        // #EXP-1662: every way in from outside a chat opens that chat's
        // plan tab (an unsynced session = its own route, web's plan route).
        LocalOpenPlanPage provides openPlanTab,
    ) {
    NavHost(
        navController = navController,
        // Chats is the stack ROOT (where the app lands); every other tab is
        // one entry above it, so Back from a tab returns to Chats.
        startDestination = if (needsOnboarding) "onboarding" else MainTabs.CHATS,
        // iOS-style horizontal push/pop transitions — except a tab switch
        // (EXP-1210): tab roots are siblings, so moving between them (and
        // Back from a tab root to the Chats root) swaps with no transition.
        enterTransition = {
            if (isTabSwap(tabSwitch)) EnterTransition.None
            else slideIntoContainer(SlideDirection.Start, pushSpec)
        },
        exitTransition = {
            if (isTabSwap(tabSwitch)) ExitTransition.None
            else slideOutOfContainer(SlideDirection.Start, pushSpec)
        },
        popEnterTransition = { popSlideIn(tabSwitch, pushSpec) },
        popExitTransition = { popSlideOut(tabSwitch, pushSpec) },
        // FEED-265: the back GESTURE (predictive back, on by default at
        // targetSdk 36) does not use popEnter/popExit — navigation-compose
        // gives it its own pair, defaulting to a fade-in under a scale-out.
        // Pin it to the same reverse slide, which the gesture then scrubs.
        predictivePopEnterTransition = { popSlideIn(tabSwitch, pushSpec) },
        predictivePopExitTransition = { popSlideOut(tabSwitch, pushSpec) },
    ) {
        composable("onboarding") {
            OnboardingScreen(
                onDone = {
                    navController.navigate(MainTabs.CHATS) { popUpTo("onboarding") { inclusive = true } }
                },
            )
        }
        composable(MainTabs.ASSISTANT) {
            AssistantTabScreen(
                ref = assistant,
                onOpenSession = { id -> navController.navigate(Routes.session(id)) },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                onNewIssue = { boardId, rootId -> navController.openIssueDraft(boardId, chatId = rootId) },
                onOpenApp = { id -> navController.navigate(Routes.app(id)) },
            )
        }
        composable(MainTabs.TEAMS) {
            TeamsScreen(
                onOpenBoard = { id -> navController.navigate(Routes.board(id)) },
                onOpenAgent = { id, root -> navController.open(root?.let { Routes.chat(it) } ?: Routes.agentDm(id), tabSwitch) },
                onOpenAgentSettings = { id, root -> navController.open(Routes.agentChatSettings(id, root), tabSwitch) },
                onRunApp = { id -> navController.navigate(Routes.appRun(id)) },
                onEditApp = { id, settings -> navController.navigate(if (settings) Routes.appSettings(id) else Routes.app(id)) },
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                onNewChat = { teamId -> navController.navigateDeepLink(agentRoute(AgentComposerSeed(teamId = teamId))) },
                onNewAgent = { teamId -> navController.navigate(Routes.agentNew(teamId)) },
                onNewIssue = { boardId -> navController.openIssueDraft(boardId) },
                onOpenTeamSettings = { teamId, section ->
                    navController.navigate(section?.let { Routes.teamSettings(teamId, it) } ?: Routes.teamSettingsRoot(teamId))
                },
                onOpenComposer = { navController.navigateDeepLink(AGENT_ROUTE) },
                chats = chatsVm,
            )
        }
        composable(Routes.TEAM_SETTINGS_ROOT_PATTERN) {
            Docked {
                TeamSettingsRootScreen(
                    onBack = { navController.popBackStack() },
                    onOpenSection = { teamId, section -> navController.navigate(Routes.teamSettings(teamId, section)) },
                )
            }
        }
        composable(
            Routes.DEVICES_PATTERN,
            arguments = listOf(
                // #EXP-1534: the device whose settings open.
                navArgument("device") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) { entry ->
            AgentsScreen(
                onBack = { navController.popBackStack() },
                openDeviceRowId = entry.arguments?.getString("device"),
            )
        }
        composable("search") {
            // EXP-686: a pushed detail route now (not a tab), so it carries a
            // back button and the bar yields the full height.
            SearchScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
                // #EDE-99 (search-list.json A PICK): closes the search, opens
                // the hit's session (a root = its chat, a child = its view)
                // with find in chat seeded on the query at the hit.
                onOpenChatHit = { pick, isRoot ->
                    com.exponential.app.ui.chat.FindInChatSeeds.put(pick)
                    navController.popBackStack("search", inclusive = true)
                    navController.open(if (isRoot) Routes.chat(pick.sessionId) else Routes.session(pick.sessionId), tabSwitch)
                },
            )
        }
        composable(
            // EXP-825: THE composer page — a pushed detail (New › Chat, every play
            // button) whose nullable query args carry the preselection seed
            // (`AgentComposerSeed.fromArgs` reads them off the ViewModel's
            // SavedStateHandle). The pattern is generated with the args so a
            // navigate() with fewer of them still matches.
            AGENT_ROUTE_PATTERN,
            arguments = AGENT_ROUTE_ARGS.map { name ->
                navArgument(name) {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                }
            },
        ) {
            val composerFocusBus = applicationDeepLinkBus()
            AgentScreen(
                onBack = { navController.popBackStack() },
                // The New chat shortcut: the composer focused.
                pendingComposerFocus = composerFocusBus.pendingComposerFocus.collectAsStateWithLifecycle().value,
                onComposerFocused = composerFocusBus::consumeComposerFocus,
                onOpenSession = { sessionId -> navController.navigate(Routes.session(sessionId)) },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                // The composer gives way to the chat it started (the Assistant's = its tab).
                onLandInChat = { id ->
                    if (AssistantTab.landingOf(Routes.chat(id), assistant.value) != null) {
                        navController.popBackStack()
                        navController.open(Routes.chat(id), tabSwitch)
                    } else {
                        navController.landOnChat(id, replaceCurrent = true)
                    }
                },
            )
        }
        composable(MainTabs.INBOX) {
            // Session 58cb32df: the INBOX page — pushed from the Chats
            // header's inbox circle (no segments: My issues and Drafts moved
            // to the board's chips).
            val inboxDeepLinks = applicationDeepLinkBus()
            PersonalScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
                // EXP-980 / #EXP-1456: a row naming a session (a wall, an
                // agent's decision, an @mention) opens it by the push's rule.
                onOpenSession = { sessionId -> inboxDeepLinks.openSession(sessionId) },
                // #EXP-1534: an agent_signed_out row opens its device's settings.
                onOpenDevice = { deviceId -> inboxDeepLinks.openDevice(deviceId) },
                // EXP-1291: an agent message's issue row opens the issue.
                onOpenIssueGuide = { id -> navController.navigate("issue/$id") },
            )
        }
        composable(Routes.CHATS) {
            // The Chats tab: a row opens its chat, an agent row its channel
            // (the same screen), the Assistant's row its tab.
            ChatsScreen(
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                // NEW-7: an agent with no conversation of mine yet opens its
                // empty DM (the first send creates it).
                onOpenAgent = { id -> navController.open(Routes.agentDm(id), tabSwitch) },
                // #EXP-1733: an app row opens the app's chat (its editor).
                onOpenApp = { id -> navController.navigate(Routes.app(id)) },
                onOpenSession = { id -> navController.navigate(Routes.session(id)) },
                // Session 58cb32df: the header's inbox circle pushes the inbox.
                onOpenInbox = { navController.navigate(MainTabs.INBOX) { launchSingleTop = true } },
                // #FEED-239: an agent row's `…` › Agent settings opens that
                // agent's chat with its settings item open.
                onOpenAgentSettings = { id, rootId -> navController.open(Routes.agentChatSettings(id, rootId), tabSwitch) },
                inboxUnread = unreadCountLatest.value,
                // #EXP-1388: the Archived entry pushes the Archived page.
                onOpenArchived = { navController.navigate(Routes.CHATS_ARCHIVED) { launchSingleTop = true } },
                // #FEED-191: the Needs you entry pushes THE list.
                onOpenNeedsYou = { navController.navigate(Routes.CHATS_NEEDS_YOU) { launchSingleTop = true } },
            )
        }
        composable(Routes.CHATS_NEEDS_YOU) {
            // #FEED-191 (needs-you.json): pushed, the page header with back; a
            // row opens the chat on the right tab.
            NeedsYouPage(
                onBack = { navController.popBackStack() },
                onOpen = { row -> navController.open(Routes.needsYou(row), tabSwitch) },
                onOpenInbox = { navController.navigate(MainTabs.INBOX) { launchSingleTop = true } },
                inboxUnread = unreadCountLatest.value,
            )
        }
        composable(Routes.CHATS_ARCHIVED) {
            // #EXP-1388: the viewer's archived chats (the page header with
            // back; the selection bar unarchives).
            ChatsScreen(
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                onOpenAgent = { id -> navController.navigate(Routes.agentDm(id)) },
                onOpenInbox = { navController.navigate(MainTabs.INBOX) { launchSingleTop = true } },
                inboxUnread = unreadCountLatest.value,
                view = com.exponential.app.domain.Chats.ChatListView.Archived,
                onBack = { navController.popBackStack() },
            )
        }
        composable(MainTabs.SETTINGS) {
            SettingsScreen(
                onOpenAccount = { accountId -> navController.navigate("server/$accountId") },
                onOpenAssistant = { navController.navigate(Routes.ASSISTANT_SETTINGS) },
                onOpenNotifications = { navController.navigate(Routes.NOTIFICATION_SETTINGS) },
                onOpenSecurity = { navController.navigate(Routes.SECURITY) },
                onOpenDevices = { navController.navigate(Routes.DEVICES) },
                onOpenAbout = { navController.navigate(Routes.ABOUT) },
                onOpenSyncDiagnostics = { navController.navigate(Routes.SYNC_DIAGNOSTICS) },
                onAddServer = { navController.navigate("add-server") },
                onOpenComposer = { navController.navigateDeepLink(AGENT_ROUTE) },
            )
        }
        composable(
            Routes.APP_RUN_PATTERN,
            arguments = Routes.APP_RUN_ARGS.map { name ->
                navArgument(name) {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                }
            },
        ) {
            // VAPP-242 (apps.json THE RUN VIEW): the published app, full screen.
            AppRunScreen(
                onBack = { navController.popBackStack() },
                onEdit = { id -> navController.navigate(Routes.app(id)) },
                onOpenTeamSettings = openReadinessTeamSettings,
            )
        }
        composable(
            Routes.APP_PATTERN,
            arguments = Routes.APP_ARGS.map { name ->
                navArgument(name) {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                }
            },
        ) {
            AppEditorScreen(
                onBack = { navController.popBackStack() },
                onOpenSession = { id -> navController.navigate(Routes.session(id)) },
                // #VAPP-230: Connect <server> opens the app team's Settings › MCP.
                onOpenTeamSettings = openReadinessTeamSettings,
                // VAPP-234: a template picked in a draft replaces the draft's editor.
                onOpenApp = { id ->
                    navController.navigate(Routes.app(id)) { popUpTo(Routes.APP_PATTERN) { inclusive = true } }
                },
            )
        }
        composable(Routes.SECURITY) {
            Docked { SecuritySettingsScreen(onBack = { navController.popBackStack() }) }
        }
        composable(Routes.NOTIFICATION_SETTINGS) {
            Docked { com.exponential.app.ui.settings.NotificationSettingsScreen(onBack = { navController.popBackStack() }) }
        }
        composable(Routes.SYNC_DIAGNOSTICS) {
            Docked { SyncDiagnosticsScreen(onBack = { navController.popBackStack() }) }
        }
        composable(Routes.ABOUT) {
            Docked {
                AboutScreen(
                    onOpenThirdPartyLicenses = { navController.navigate(Routes.LICENSES) },
                    onBack = { navController.popBackStack() },
                )
            }
        }
        composable(Routes.LICENSES) {
            Docked { ThirdPartyLicensesScreen(onBack = { navController.popBackStack() }) }
        }
        composable("add-server") {
            InstanceScreen(
                onContinue = { url ->
                    onSetInstanceUrlLatest.value(url)
                    navController.navigate("add-server-login") {
                        popUpTo("add-server") { inclusive = true }
                    }
                },
                showCancel = true,
                onCancel = { navController.popBackStack() },
                cloudAlreadyAdded = cloudAlreadyAddedLatest.value,
            )
        }
        composable("add-server-login") {
            LoginScreen(
                instanceUrl = "",
                onLoggedIn = {
                    navController.resetToRoot()
                },
                onChangeInstance = { navController.popBackStack() },
            )
        }
        composable(Routes.SERVER_PATTERN) { entry ->
            val accountId = entry.arguments?.getString("accountId").orEmpty()
            Docked {
                ServerDetailScreen(
                    accountId = accountId,
                    onBack = { navController.popBackStack() },
                    onOpenVoice = { navController.navigate("server/$accountId/voice") },
                )
            }
        }
        // NEW-67: the account's Settings → Voice page (`voice.json`); the
        // ViewModel reads `accountId` off the SavedStateHandle.
        composable(Routes.SERVER_VOICE_PATTERN) {
            Docked { VoiceSettingsScreen(onBack = { navController.popBackStack() }) }
        }
        composable(
            // #EXP-1381 (settings-nav.json): ONE team settings category per
            // page, its label the title; the TeamSettingsViewModel reads
            // `section` off the SavedStateHandle and observes only its data.
            Routes.TEAM_SETTINGS_PATTERN,
        ) { entry ->
            val back: () -> Unit = { navController.popBackStack() }
            val teamId = entry.arguments?.getString("teamId").orEmpty()
            Docked { Box(Modifier.fillMaxSize().testTag(SettingsNav.PAGE_TEST_ID)) {
            when (entry.arguments?.getString("section")) {
                SettingsNav.AGENTS -> AgentsSettingsScreen(
                    onBack = back,
                    onOpenAgent = { id -> navController.navigate(Routes.agentSettings(id)) },
                    onNewAgent = { navController.navigate(Routes.agentNew(teamId)) },
                )
                SettingsNav.PROMPTS -> PromptsSettingsScreen(
                    onBack = back,
                    onOpenPrompt = { id -> navController.navigate(Routes.promptSettings(id)) },
                    onNewPrompt = { navController.navigate(Routes.promptNew(teamId)) },
                )
                // #VAPP-185: Settings › MCP (settings-mcp.json); Activity is its own page.
                SettingsNav.MCP -> McpSettingsScreen(
                    onBack = back,
                    onOpenActivity = { navController.navigate(Routes.mcpActivity(teamId)) },
                )
                // #EXP-1661: Settings › Skills, read only on phones.
                SettingsNav.SKILLS -> SkillsSettingsScreen(
                    onBack = back,
                    onOpenSkill = { id -> navController.navigate(Routes.skillSettings(id)) },
                )
                else -> TeamSettingsScreen(
                    onBack = back,
                    onOpenRepo = { id -> navController.navigate(Routes.repo(teamId, id)) },
                )
            }
            } }
        }
        composable(Routes.SHARE_TO) {
            // EXP-1635 (share-targets.json STEP 1): where the share goes.
            val pendingShare by shell.pendingShare.collectAsStateWithLifecycle()
            val shareVm: ShareTargetPickerViewModel = hiltViewModel()
            val shareTo by shareVm.shareTo.collectAsStateWithLifecycle()
            // New issue discarded its share: nothing left to place.
            val share = pendingShare
            LaunchedEffect(share == null) {
                if (share == null && navController.currentBackStackEntry?.destination?.route == Routes.SHARE_TO) {
                    navController.popBackStack()
                }
            }
            if (share != null) {
                ShareToScreen(
                    state = shareTo,
                    imageUris = share.imageUris,
                    onBack = {
                        shell.consumePendingShare()
                    },
                    onSelectTeam = shareVm::selectTeam,
                    onNewIssue = { navController.navigate(Routes.SHARE_COMPOSE) { launchSingleTop = true } },
                    onPick = { pick ->
                        shell.consumePendingShare()?.let {
                            shell.setPendingChatShare(ChatShare(it, pick = pick, teamId = shareTo.teamId))
                        }
                        navController.popBackStack(Routes.SHARE_TO, inclusive = true)
                        navController.navigateDeepLink(AGENT_ROUTE)
                    },
                )
            }
        }
        composable(Routes.SHARE_COMPOSE) {
            // Single-screen share composer: the prefilled create form with the
            // "Share to" destination selector on top (EXP-60). The pending
            // share lives in the ShellState singleton (not route
            // state) so backing out and re-entering re-fills the form; it's
            // consumed exactly once — on a successful create or an explicit
            // discard.
            val pendingShare by shell.pendingShare.collectAsStateWithLifecycle()
            val sharePrefill = remember(pendingShare) { pendingShare?.let { buildSharePrefill(it) } }
            val shareVm: ShareTargetPickerViewModel = hiltViewModel()
            val shareState by shareVm.state.collectAsStateWithLifecycle()
            // EXP-1170: the New issue page in share mode (no draft row).
            IssueDraftScreen(
                onBack = { navController.popBackStack() },
                // The share is placed: Share to goes with the form.
                onCreated = { issueId ->
                    navController.navigate("issue/$issueId") {
                        popUpTo(Routes.SHARE_TO) { inclusive = true }
                    }
                },
                sharePrefill = sharePrefill,
                onSharePrefillConsumed = { shell.consumePendingShare() },
                shareMode = true,
                shareGroups = shareState.groups,
                shareRecentBoardId = shareState.recentBoardId,
                shareGroupsLoading = shareState.isLoading,
            )
        }
        composable(Routes.BOARD_PATTERN) { entry ->
            val boardId = entry.arguments?.getString("boardId").orEmpty()
            // Remembering the opened board drives the share picker's default.
            LaunchedEffect(boardId) {
                val accountId = activeAccountIdLatest.value
                if (boardId.isNotBlank() && accountId != null) {
                    shell.rememberLastBoard(accountId, boardId)
                }
            }
            val myIssuesBus = applicationDeepLinkBus()
            IssueListScreen(
                boardId = boardId,
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
                onOpenAgent = openAgent,
                onOpenComposer = { navController.navigateDeepLink(AGENT_ROUTE) },
                onNewIssue = { navController.openIssueDraft(boardId) },
                onOpenTeamSettings = openReadinessTeamSettings,
                // Session 58cb32df: the board's Drafts chip reopens a draft.
                onOpenDraft = { draftBoard, draftId -> navController.openIssueDraft(draftBoard, draftId = draftId) },
                // #EXP-1830: the My issues widget's header.
                pendingMyIssues = myIssuesBus.pendingMyIssues.collectAsStateWithLifecycle().value,
                onMyIssuesShown = myIssuesBus::consumeMyIssues,
            )
        }
        composable(
            // EXP-1170: the New issue PAGE. Every opener mints the draft id at
            // tap time; `board` is the target, `status` a preset, `parent`
            // files it as a sub-issue (no draft row then, EXP-1130).
            ISSUE_DRAFT_ROUTE,
            arguments = listOf(
                navArgument("board") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
                navArgument("status") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
                navArgument("parent") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
                // EXP-1284: "New issue in this chat" — the create links it.
                navArgument("chat") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) {
            IssueDraftScreen(
                onBack = { navController.popBackStack() },
                onCreated = { issueId -> navController.openCreatedIssue(issueId) },
                // T11: a chat draft (opened from the chat, or reopened
                // from Drafts) lands on the chat with its new issue's tab.
                onCreatedInChat = { _, chatId ->
                    navController.landOnChat(chatId, replaceCurrent = true)
                },
            )
        }
        composable(
            ISSUE_ROUTE,
            arguments = listOf(
                // EXP-1241: a comment call's card lands scrolled to its comment.
                navArgument("comment") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) { entry ->
            val issueId = entry.arguments?.getString("issueId").orEmpty()
            IssueScreen(
                issueId = issueId,
                focusCommentId = entry.arguments?.getString("comment"),
                onBack = { navController.popBackStack() },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenAgent = openAgent,
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                onOpenTeamSettings = openReadinessTeamSettings,
                onOpenDevices = openReadinessDevices,
                onCreateSubIssue = { boardId, parentId ->
                    // Single-top: a double-tap on `+` pushes ONE page.
                    navController.openIssueDraft(boardId, parentId = parentId, singleTop = true)
                },
            )
        }
        composable(Routes.SESSION_PATTERN) { entry ->
            // EXP-1291: the RAW session view (the live transcript).
            AgentSessionScreen(
                sessionId = entry.arguments?.getString("sessionId").orEmpty(),
                onBack = { navController.popBackStack() },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                // S-3: an inter-session link pushes that session.
                onOpenSession = { id -> navController.navigate(Routes.session(id)) },
                onOpenAgentHome = { navController.selectTab(MainTabs.ASSISTANT, tabSwitch, restore = false) },
            )
        }
        composable(
            Routes.CHAT_PATTERN,
            arguments = listOf(
                navArgument("tab") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
                // #EXP-1662: the plan card the plan tab opens on.
                navArgument("plan") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
                // #FEED-191: the waiting card the Conversation centres.
                navArgument("card") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) {
            // EXP-1291: a chat root OR an agent root — ONE conversation screen.
            ChatScreen(
                onBack = { navController.popBackStack() },
                onOpenSession = { id -> navController.navigate(Routes.session(id)) },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenChat = { id -> navController.open(Routes.chat(id), tabSwitch) },
                // The "+"'s New issue: filed + linked to this root on create.
                onNewIssue = { boardId, rootId -> navController.openIssueDraft(boardId, chatId = rootId) },
                // #EXP-1849: an app's root opens as the app, replacing this entry.
                onOpenApp = { id -> navController.replaceWith(Routes.app(id)) },
                onOpenAgentHome = { navController.selectTab(MainTabs.ASSISTANT, tabSwitch, restore = false) },
            )
        }
        // #EXP-1987: Settings › Assistant (the account page).
        composable(Routes.ASSISTANT_SETTINGS) {
            Docked { AssistantSettingsRoute(onBack = { navController.popBackStack() }) }
        }
        composable(Routes.AGENT_SETTINGS_PATTERN) {
            AgentSettingsScreen(
                onBack = { navController.popBackStack() },
                onCreated = {},
                onDeleted = { navController.popBackStack() },
            )
        }
        composable(Routes.AGENT_NEW_PATTERN) {
            AgentSettingsScreen(
                onBack = { navController.popBackStack() },
                // A created agent replaces the `new` page with its own.
                onCreated = { id ->
                    val newId = navController.currentBackStackEntry?.destination?.id
                    navController.navigate(Routes.agentSettings(id)) {
                        if (newId != null) popUpTo(newId) { inclusive = true }
                    }
                },
                onDeleted = { navController.popBackStack() },
            )
        }
        composable(
            Routes.AGENT_DM_PATTERN,
            arguments = listOf(
                // #FEED-239: `settings` = open on the agent's settings item.
                navArgument("open") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) { entry ->
            // NEW-7: my DM with an agent; once it exists it IS the chat
            // screen's agent channel, which replaces this entry.
            AgentConversationScreen(
                openSettings = entry.arguments?.getString("open") == "settings",
                onBack = { navController.popBackStack() },
                // #EXP-1849: an app's agent opens as its app, replacing this entry.
                onOpenApp = { id -> navController.replaceWith(Routes.app(id)) },
                onOpenConversation = { id ->
                    val here = navController.currentBackStackEntry?.destination?.id
                    navController.navigate(Routes.chat(id)) {
                        if (here != null) popUpTo(here) { inclusive = true }
                    }
                },
            )
        }
        composable(Routes.PROMPT_SETTINGS_PATTERN) {
            PromptSettingsScreen(
                onBack = { navController.popBackStack() },
                onCreated = {},
                onDeleted = { navController.popBackStack() },
            )
        }
        composable(Routes.SKILL_SETTINGS_PATTERN) {
            SkillSettingsScreen(onBack = { navController.popBackStack() })
        }
        composable(
            Routes.CI_RUN_PATTERN,
            arguments = listOf(
                navArgument("job") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) {
            // #EXP-1804 (ci-run.json): THE run page of our own CI; Re-run
            // failed opens the new run in its place.
            com.exponential.app.ui.ci.CiRunScreen(
                onBack = { navController.popBackStack() },
                onOpenRun = { id ->
                    navController.navigate(Routes.ciRun(id)) { popUpTo(Routes.CI_RUN_PATTERN) { inclusive = true } }
                },
            )
        }
        // #EXP-1616 (repo-page.json): the repo page; the ViewModel reads
        // `repositoryId` + `tab` off the SavedStateHandle.
        composable(
            Routes.REPO_PATTERN,
            arguments = listOf(
                navArgument("repositoryId") { type = NavType.StringType },
                navArgument("tab") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) {
            com.exponential.app.ui.repo.RepoPageScreen(
                onBack = { navController.popBackStack() },
                // #EXP-1617 (repo-ci.json): a run row opens THE run page.
                onOpenRun = { runId -> navController.navigate(Routes.ciRun(runId)) },
            )
        }
        composable(Routes.MCP_ACTIVITY_PATTERN) {
            McpActivityScreen(onBack = { navController.popBackStack() })
        }
        composable(Routes.PROMPT_NEW_PATTERN) {
            PromptSettingsScreen(
                onBack = { navController.popBackStack() },
                // A created prompt replaces the `new` page with its own.
                onCreated = { id ->
                    val newId = navController.currentBackStackEntry?.destination?.id
                    navController.navigate(Routes.promptSettings(id)) {
                        if (newId != null) popUpTo(newId) { inclusive = true }
                    }
                },
                onDeleted = { navController.popBackStack() },
            )
        }
        composable("invite/{token}") { entry ->
            val token = entry.arguments?.getString("token").orEmpty()
            InviteAcceptScreen(
                token = token,
                onBack = { navController.popBackStack() },
                onAccepted = {
                    navController.resetToRoot()
                },
            )
        }
    }
    }

    // A background server that 426'd this build (REV2-18): only the ACTIVE
    // account's gate blocks the app, but that account's sync IS stopped, so
    // say so rather than let it read as frozen sync. Dismissable for this app
    // run; sign-out lives on the server's row in Settings.
    var gatedBannerDismissed by remember(gatedOtherServers) { mutableStateOf(false) }
    val showsGatedBanner =
        gatedOtherServers.isNotEmpty() && !gatedBannerDismissed && !needsOnboarding
    // EXP-533: the active account can't reach its server. Unlike iOS (whose
    // navigator owns a root inset slot) this floats at the BOTTOM: a top strip
    // here would cover or reflow every screen's own header. Never during
    // onboarding, which has no cached data to explain.
    val showsOfflineBanner = syncHealth == SyncHealth.Offline && !needsOnboarding
    if (showsGatedBanner || showsOfflineBanner) {
        // One stack, so the two never overlap. The gated banner stays last —
        // its position is unchanged from when it was the only one.
        Column(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                // Route-level, NOT barShown: while a selection suppresses
                // the nav bar, the screen's own 52dp bar is standing in that
                // exact slot — dropping to 0 would land the banner on it.
                .padding(bottom = if (barVisible) BottomBarInset else 0.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (showsOfflineBanner) OfflineBanner(onRetry = onRetrySync)
            if (showsGatedBanner) {
                GatedServersBanner(
                    servers = gatedOtherServers,
                    onDismiss = { gatedBannerDismissed = true },
                )
            }
        }
    }

    // After the NavHost, so the popup's back wins over the NavHost's pop.
    BackHandler(enabled = newOpen) { newOpen = false }
    AnimatedVisibility(
        visible = newOpen && barShown,
        enter = fadeIn(Motion.standard(reduceMotion)),
        exit = fadeOut(Motion.standard(reduceMotion)),
    ) {
        PhoneNewMenu(
            issueEnabled = composeBoardId != null,
            onPick = { item ->
                newOpen = false
                when (item) {
                    "issue" -> composeBoardId?.let { navController.openIssueDraft(it) }
                    else -> navController.navigateDeepLink(AGENT_ROUTE)
                }
            },
            onDismiss = { newOpen = false },
        )
    }

    AnimatedVisibility(
        visible = barShown,
        enter = slideInVertically(Motion.standard(reduceMotion)) { it } +
            fadeIn(Motion.standard(reduceMotion)),
        exit = slideOutVertically(Motion.standard(reduceMotion)) { it } +
            fadeOut(Motion.standard(reduceMotion)),
        modifier = Modifier.align(Alignment.BottomCenter),
    ) {
        // The lit tab keeps through the bar's exit (a detail lights none).
        val lit = remember { mutableStateOf(page.tab) }
        if (page.tab != null) lit.value = page.tab
        PhoneDock(
            selected = lit.value,
            newOpen = newOpen,
            badges = dockBadges(chatsVm, assistant, chatsBadge),
            onSelect = { tab ->
                if (tab == "new") {
                    newOpen = !newOpen
                } else {
                    newOpen = false
                    // EXP-1210: every tab SWITCHES to a sibling root (a re-tap
                    // pops back to it).
                    navController.selectTab(tab, tabSwitch)
                }
            },
        )
    }
    }
}

/** The dock's badges by tab; read only while the bar shows. */
@Composable
private fun dockBadges(
    chats: ChatsViewModel,
    assistant: kotlinx.coroutines.flow.StateFlow<AssistantRef?>,
    chatsBadge: NavBadges.Badge,
): Map<String, NavBadges.Badge> {
    val ref by assistant.collectAsStateWithLifecycle()
    val rows by chats.rowBadges.collectAsStateWithLifecycle()
    return mapOf(
        "chats" to chatsBadge,
        "assistant" to (rows[ref?.sessionId] ?: NavBadges.NONE),
    )
}

/** A page the docked bar stands on, its content kept above the bar. */
@Composable
private fun Docked(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().aboveDock()) { content() }
}

/** A team settings category, or the Teams page while the team is unknown. */
private fun NavHostController.openTeamSettings(teamId: String?, section: String, marker: TabSwitchMarker?) {
    if (teamId != null) navigate(Routes.teamSettings(teamId, section)) else selectTab(MainTabs.TEAMS, marker)
}

/** EXP-1210: a move the NavHost draws with no transition (see [MainTabs]). */
private fun AnimatedContentTransitionScope<NavBackStackEntry>.isTabSwap(
    tabSwitch: TabSwitchMarker,
): Boolean =
    MainTabs.isRootSwap(initialState.destination.route, targetState.destination.route) ||
        tabSwitch.matches(initialState, targetState)

/** FEED-265: going back = the push slide reversed (the previous page returns
 *  from the start edge, the leaving one slides out to the end). One pair for
 *  every way back: the back button, the toolbar arrow and the back gesture. */
private fun AnimatedContentTransitionScope<NavBackStackEntry>.popSlideIn(
    tabSwitch: TabSwitchMarker,
    spec: FiniteAnimationSpec<IntOffset>,
): EnterTransition =
    if (isTabSwap(tabSwitch)) EnterTransition.None
    else slideIntoContainer(SlideDirection.End, spec)

private fun AnimatedContentTransitionScope<NavBackStackEntry>.popSlideOut(
    tabSwitch: TabSwitchMarker,
    spec: FiniteAnimationSpec<IntOffset>,
): ExitTransition =
    if (isTabSwap(tabSwitch)) ExitTransition.None
    else slideOutOfContainer(SlideDirection.End, spec)

/** EXP-1170: the New issue page — `drafts/{draftId}?board=&status=&parent=`. */
private const val ISSUE_DRAFT_ROUTE =
    "drafts/{draftId}?board={board}&status={status}&parent={parent}&chat={chat}"

/**
 * EXP-1291: land on the CHAT [chatId] (a root session). The page that
 * started it is replaced when [replaceCurrent]; when that page sat on this
 * very chat, the old chat entry goes too, so Back never returns to a
 * stale copy of it.
 */
private fun NavHostController.landOnChat(chatId: String, replaceCurrent: Boolean) {
    val current = currentBackStackEntry
    val previous = previousBackStackEntry
    val overSameChat = previous?.destination?.route == Routes.CHAT_PATTERN &&
        previous.arguments?.getString("sessionId") == chatId
    navigate(Routes.chat(chatId)) {
        when {
            overSameChat && previous != null -> popUpTo(previous.destination.id) { inclusive = true }
            replaceCurrent && current != null -> popUpTo(current.destination.id) { inclusive = true }
        }
    }
}

/**
 * Open the New issue page. The draft id is minted HERE, at tap time, so the
 * page and its autosave own one stable id from the first frame (EXP-1170).
 */
/** #EXP-1638: how long a New issue app shortcut waits for a cold start's board. */
private const val BOARD_WAIT_MS = 3_000L

/** How long a deep link waits for the Assistant's local rows. */
private const val ASSISTANT_WAIT_MS = 1_000L

private fun NavHostController.openIssueDraft(
    boardId: String,
    statusId: String? = null,
    parentId: String? = null,
    draftId: String = java.util.UUID.randomUUID().toString(),
    singleTop: Boolean = false,
    chatId: String? = null,
) {
    val query = listOfNotNull(
        "board=${Uri.encode(boardId)}",
        statusId?.let { "status=${Uri.encode(it)}" },
        parentId?.let { "parent=${Uri.encode(it)}" },
        chatId?.let { "chat=${Uri.encode(it)}" },
    ).joinToString("&")
    navigate("drafts/$draftId?$query") {
        if (singleTop) launchSingleTop = true
    }
}

/**
 * Land on a just-created issue (EXP-596). The create form is REPLACED rather
 * than left under the issue, so Back from it returns to wherever the compose
 * started (the board list, the home shell for a share) instead of re-showing an
 * empty form.
 */
private fun NavHostController.openCreatedIssue(issueId: String) {
    // Popped by destination id, so each caller's route pattern stays its own
    // business, and in the SAME transaction as the push — a pop-then-navigate
    // pair puts the form back on screen for the frame in between.
    val formId = currentBackStackEntry?.destination?.id
    navigate("issue/$issueId") {
        if (formId != null) popUpTo(formId) { inclusive = true }
    }
}

/** The floating "this server needs a newer app" notice (REV2-18). */
@Composable
private fun GatedServersBanner(
    servers: List<String>,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        // navigationBarsPadding lives on the banner stack in AuthenticatedNav
        // (EXP-533) — applying it per banner would inset each one again.
        modifier = modifier
            .padding(horizontal = 16.dp, vertical = 8.dp)
            // EXP-698: the ROW rung, not the capsule one. This banner wraps to
            // two lines and carries a dismiss button, so it is a notice
            // surface; the capsule recipe belongs to GlassPill alone.
            .glassRow(opaque = true)
            .padding(start = 14.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            ExpIcons.uiWarning,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = DesignTokens.Semantic.Yellow,
        )
        Spacer(Modifier.width(10.dp))
        val subject = if (servers.size == 1) {
            "${servers.first()} needs"
        } else {
            "${servers.size} servers need"
        }
        Text(
            "$subject a newer app version. Sync there is paused.",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f, fill = false),
        )
        IconButton(onClick = onDismiss, modifier = Modifier.size(28.dp)) {
            Icon(
                ExpIcons.uiClose,
                contentDescription = "Dismiss",
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/**
 * The floating "can't reach the server" notice (EXP-533). Copy and icon are
 * the iOS/desktop banner's, byte-for-byte; only the placement differs (see
 * AuthenticatedNav). No dismiss: it disappears the moment a poll succeeds, and
 * a dismissed one would leave stale boards looking live.
 */
@Composable
private fun OfflineBanner(
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .padding(horizontal = 16.dp, vertical = 8.dp)
            // Same notice rung as the gated-servers banner above.
            .glassRow(opaque = true)
            .padding(start = 14.dp, end = 6.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            ExpIcons.uiOffline,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = DesignTokens.Semantic.Yellow,
        )
        Spacer(Modifier.width(10.dp))
        Text(
            "Can't reach the server, showing cached data",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f, fill = false),
        )
        Spacer(Modifier.width(4.dp))
        TextButton(onClick = onRetry, contentPadding = PaddingValues(horizontal = 12.dp)) {
            Text("Retry", style = MaterialTheme.typography.labelMedium)
        }
    }
}

// --- Hilt EntryPoint accessors for app-singletons consumed inside composables.

@Composable
private fun applicationDeepLinkBus(): DeepLinkBus {
    val app = LocalContext.current.applicationContext as ExponentialApp
    return EntryPointAccessors.fromApplication(app, DeepLinkEntryPoint::class.java).deepLinkBus()
}

@Composable
private fun applicationInAppNotifications(): InAppNotifications {
    val app = LocalContext.current.applicationContext as ExponentialApp
    return EntryPointAccessors.fromApplication(app, InAppNotificationsEntryPoint::class.java).inAppNotifications()
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface InAppNotificationsEntryPoint {
    fun inAppNotifications(): InAppNotifications
}

@Composable
private fun applicationShellState(): ShellState {
    val app = LocalContext.current.applicationContext as ExponentialApp
    return EntryPointAccessors
        .fromApplication(app, ShellStateEntryPoint::class.java)
        .shellState()
}

@Composable
private fun applicationWebLinkResolver(): WebLinkResolver {
    val app = LocalContext.current.applicationContext as ExponentialApp
    return EntryPointAccessors
        .fromApplication(app, WebLinkResolverEntryPoint::class.java)
        .webLinkResolver()
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface DeepLinkEntryPoint {
    fun deepLinkBus(): DeepLinkBus
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface ShellStateEntryPoint {
    fun shellState(): ShellState
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface WebLinkResolverEntryPoint {
    fun webLinkResolver(): WebLinkResolver
}

/** #EXP-1849: open [route] in place of the current entry (a redirect). */
private fun androidx.navigation.NavHostController.replaceWith(route: String) {
    val here = currentBackStackEntry?.destination?.id
    navigate(route) {
        if (here != null) popUpTo(here) { inclusive = true }
    }
}
