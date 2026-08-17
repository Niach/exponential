package com.exponential.app.navigation

import androidx.compose.animation.AnimatedContentTransitionScope.SlideDirection
import androidx.compose.foundation.layout.Box
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.exponential.app.AppConstants
import com.exponential.app.AppViewModel
import com.exponential.app.ExponentialApp
import com.exponential.app.data.TeamSelection
import androidx.browser.customtabs.CustomTabsIntent
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.push.WebLinkResolver
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.BottomNavBar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.invite.InviteAcceptScreen
import com.exponential.app.ui.issue.CreateIssueScreen
import com.exponential.app.ui.onboarding.OnboardingScreen
import com.exponential.app.ui.personal.PersonalScreen
import com.exponential.app.ui.reviews.ReviewsScreen
import com.exponential.app.ui.issue.IssueDetailScreen
import com.exponential.app.ui.issue.IssueListMode
import com.exponential.app.ui.issue.IssueListScreen
import com.exponential.app.ui.issue.ChangesScreen
import com.exponential.app.ui.actions.ActionsScreen
import com.exponential.app.ui.search.SearchScreen
import com.exponential.app.ui.session.AgentSessionScreen
import com.exponential.app.ui.session.AgentsScreen
import com.exponential.app.ui.settings.AboutScreen
import com.exponential.app.ui.settings.ServerDetailScreen
import com.exponential.app.ui.settings.SettingsScreen
import com.exponential.app.ui.settings.SyncDiagnosticsScreen
import com.exponential.app.ui.settings.TeamSettingsScreen
import com.exponential.app.ui.settings.ThirdPartyLicensesScreen
import com.exponential.app.ui.share.ShareTargetPickerViewModel
import com.exponential.app.ui.share.buildSharePrefill
import com.exponential.app.ui.support.SupportScreen
import com.exponential.app.ui.support.SupportThreadScreen
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.update.UpdateRequiredScreen
import dagger.hilt.android.EntryPointAccessors

/**
 * The single navigation surface, mirroring the iOS `AppNavigator`: a gradient
 * [AppBackground] behind one push-stack `NavHost`, with the floating bottom
 * pill (Issues · My Work · Support (helpdesk-gated, EXP-180) · Agents ·
 * Reviews · Search + compose FAB) overlaid
 * on the top-level routes. Replaces the inline graph + `MainScaffold` drawer
 * shell that used to live in MainActivity.
 */
@Composable
fun AppNavHost() {
    val viewModel: AppViewModel = hiltViewModel()
    val deepLinkBus = applicationDeepLinkBus()
    val teamSelection = applicationTeamSelection()
    val webLinkResolver = applicationWebLinkResolver()
    val state by viewModel.state.collectAsStateWithLifecycle()
    val navController = rememberNavController()
    val pendingTarget by deepLinkBus.target.collectAsStateWithLifecycle()
    val context = LocalContext.current

    val startDestination = when {
        state.instanceUrl == null -> "instance"
        state.token == null -> "login"
        else -> "home"
    }

    LaunchedEffect(pendingTarget, state.token) {
        val target = pendingTarget ?: return@LaunchedEffect
        // Leave the target in the bus while unauthenticated so a share/deep-link
        // received before login resumes once the token lands (token is a key).
        if (state.token == null) return@LaunchedEffect
        when (target) {
            is DeepLinkBus.Target.Issue ->
                navController.navigate("issue/${target.id}") { launchSingleTop = true }
            is DeepLinkBus.Target.Invite ->
                navController.navigate("invite/${target.token}") { launchSingleTop = true }
            is DeepLinkBus.Target.SupportThread ->
                navController.navigate("support/${target.id}") { launchSingleTop = true }
            is DeepLinkBus.Target.WebIssueRef ->
                // Verified App Link (EXP-92): resolve slug+identifier against
                // the local DB of the account matching the link's host (brief
                // poll while sync lands fresh rows). Anything unresolvable
                // opens in a Custom Tab — which never re-triggers App Links,
                // so it can't loop back here.
                when (val resolution = webLinkResolver.resolve(target)) {
                    is WebLinkResolver.Resolution.Found -> {
                        if (resolution.accountId != state.activeAccountId) {
                            // The issue lives under another signed-in account:
                            // switch first; IssueDetail re-scopes reactively.
                            viewModel.switchAccount(resolution.accountId)
                        }
                        navController.navigate("issue/${resolution.issueId}") {
                            launchSingleTop = true
                        }
                    }
                    WebLinkResolver.Resolution.NotFound ->
                        CustomTabsIntent.Builder().build().launchUrl(context, target.uri)
                }
            is DeepLinkBus.Target.ShareContent -> {
                // Stash the shared content for the single-screen share composer
                // to consume (it carries its own inline board selector).
                teamSelection.setPendingShare(target)
                navController.navigate("share-compose") { launchSingleTop = true }
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
    val needsAuth =
        state.accounts.isEmpty() || state.instanceUrl == null || state.token == null

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
        CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.onSurface) {
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
                onLogin = {
                    navController.navigate("home") { popUpTo("login") { inclusive = true } }
                },
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
            val agentsRunning by viewModel.agentsRunning.collectAsStateWithLifecycle()
            val agentsNeedInput by viewModel.agentsNeedInput.collectAsStateWithLifecycle()
            val reviewsOpen by viewModel.reviewsOpen.collectAsStateWithLifecycle()
            val helpdeskEnabled by viewModel.helpdeskEnabled.collectAsStateWithLifecycle()
            val supportUnread by viewModel.supportUnread.collectAsStateWithLifecycle()
            val currentBoardId by viewModel.currentBoardId.collectAsStateWithLifecycle()
            val gatedOtherServers by viewModel.gatedOtherServers.collectAsStateWithLifecycle()
            AuthenticatedNav(
                navController = navController,
                cloudAlreadyAdded = cloudAlreadyAdded,
                activeAccountId = state.activeAccountId,
                gatedOtherServers = gatedOtherServers,
                needsOnboarding = needsOnboarding,
                unreadCount = unreadCount,
                agentsRunning = agentsRunning,
                agentsNeedInput = agentsNeedInput,
                reviewsOpen = reviewsOpen,
                helpdeskEnabled = helpdeskEnabled,
                supportUnread = supportUnread,
                currentBoardId = currentBoardId,
                onSetInstanceUrl = { viewModel.setInstanceUrl(it) },
            )
        }
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

@Composable
private fun AuthenticatedNav(
    navController: NavHostController,
    cloudAlreadyAdded: Boolean,
    activeAccountId: String?,
    gatedOtherServers: List<String>,
    needsOnboarding: Boolean,
    unreadCount: Int,
    agentsRunning: Boolean,
    agentsNeedInput: Boolean,
    reviewsOpen: Boolean,
    helpdeskEnabled: Boolean,
    supportUnread: Boolean,
    currentBoardId: String?,
    onSetInstanceUrl: (String) -> Unit,
) {
    val teamSelection = applicationTeamSelection()

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

    // (Fresh starts need no auto-push anymore: the Issues tab root IS the
    // last-opened board — its current-board resolution starts there.)

    // Linear-style floating bottom bar over the top-level routes only; detail
    // and settings screens get the full height back.
    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = backStackEntry?.destination?.route
    val barVisible = !needsOnboarding &&
        currentRoute in setOf(
            "home", "search", "agents", "personal", "reviews", "support-inbox", "board/{boardId}",
        )

    // The Support tab exists only while the flag is on — if it flips off
    // (team switch, feature disabled) while the Support surface is up, drop
    // it from the stack instead of stranding a tab-less screen. Pop ONLY on
    // a true→false TRANSITION of the flag (iOS AppNavigator's `.onChange`
    // parity, REV2-2): an inbox Support-group tap selects the group's team
    // right before navigating here, but helpdeskEnabled recomputes through a
    // fresh Room flow that can never emit synchronously — a guard re-run on
    // the route change alone would read the PREVIOUS team's stale false and
    // bounce the tap straight back to Issues.
    var hadHelpdesk by remember { mutableStateOf(helpdeskEnabled) }
    LaunchedEffect(helpdeskEnabled) {
        val flippedOff = hadHelpdesk && !helpdeskEnabled
        hadHelpdesk = helpdeskEnabled
        if (flippedOff) {
            // No-op when Support isn't on the back stack; also pops any
            // support thread pushed above the inbox.
            navController.popBackStack("support-inbox", inclusive = true)
        }
    }
    // The single add-issue affordance: the FAB shows while a board is in
    // view — the Issues tab root (its resolved current board) or a pushed
    // board route — so it always targets the board on screen.
    val composeBoardId = when (currentRoute) {
        "board/{boardId}" -> backStackEntry?.arguments?.getString("boardId")
        "home" -> currentBoardId
        else -> null
    }

    // EXP-523: the four transitions below are plain lambdas, not composable
    // ones, so the reduce-motion flag is read here and captured. `Motion.slow`
    // is the shared 280ms token these were already hand-set to — the only
    // behaviour change is that a user who turned animations off now gets none.
    val reduceMotion = LocalReduceMotion.current
    val pushSpec = Motion.slow<IntOffset>(reduceMotion)

    Box(modifier = Modifier.fillMaxSize()) {
    NavHost(
        navController = navController,
        startDestination = if (needsOnboarding) "onboarding" else "home",
        // iOS-style horizontal push/pop transitions.
        enterTransition = { slideIntoContainer(SlideDirection.Start, pushSpec) },
        exitTransition = { slideOutOfContainer(SlideDirection.Start, pushSpec) },
        popEnterTransition = { slideIntoContainer(SlideDirection.End, pushSpec) },
        popExitTransition = { slideOutOfContainer(SlideDirection.End, pushSpec) },
    ) {
        composable("onboarding") {
            OnboardingScreen(
                onDone = {
                    navController.navigate("home") { popUpTo("onboarding") { inclusive = true } }
                },
            )
        }
        composable("home") {
            // The Issues tab root: the current board's list with the inline
            // switcher; picking another board swaps it in place (no push).
            IssueListScreen(
                boardId = currentBoardId,
                mode = IssueListMode.Root,
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenSettings = { navController.navigate("settings") },
                // Zero-team empty state's "Join team" (EXP-188) reuses the
                // deep-link invite accept route.
                onOpenInvite = { token ->
                    navController.navigate("invite/$token") { launchSingleTop = true }
                },
            )
        }
        composable("search") {
            SearchScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
            )
        }
        composable("agents") {
            AgentsScreen(
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenActions = { navController.navigate("actions") { launchSingleTop = true } },
            )
        }
        composable("actions") {
            // Team actions (EXP-253, view + run only) — pushed from the Agents
            // tab's header entry; NOT helpdesk-gated. A detail-style route
            // (not in the bottom-bar set), so the bar yields the full height.
            ActionsScreen(
                onBack = { navController.popBackStack() },
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
            )
        }
        composable("personal") {
            // "My Work" — Inbox + My Issues merged into one board-independent
            // personal tab (EXP-58). Notification taps never land here directly
            // (pushes deep-link straight to issue/{id}), so renaming the old
            // "inbox" route is safe.
            PersonalScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                // Support-group taps land on the Support tab (the inbox
                // ViewModel has already selected the group's team).
                onOpenSupport = {
                    navController.navigate("support-inbox") {
                        launchSingleTop = true
                        popUpTo("home")
                    }
                },
            )
        }
        composable("support-inbox") {
            // Support — the team helpdesk inbox, its own bottom-bar
            // destination (EXP-180); the tab shows only while the active
            // team's synced helpdesk flag is on.
            SupportScreen(
                onOpenThread = { id -> navController.navigate("support/$id") },
            )
        }
        composable("reviews") {
            // Reviews — its own bottom-bar destination beside My Work
            // (EXP-147; it used to be a PersonalScreen segment). Rows open the
            // Review detail (EXP-168); the long-press sheet keeps issue access.
            ReviewsScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenChanges = { id -> navController.navigate("issue/$id/changes") },
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
            )
        }
        composable("settings") {
            SettingsScreen(
                onOpenServerDetail = { accountId -> navController.navigate("server/$accountId") },
                onOpenTeamSettings = { navController.navigate("team-settings") },
                onOpenSyncDiagnostics = { navController.navigate("sync-diagnostics") },
                onOpenAbout = { navController.navigate("about") },
                onAddServer = { navController.navigate("add-server") },
                onBack = { navController.popBackStack() },
            )
        }
        composable("sync-diagnostics") {
            SyncDiagnosticsScreen(onBack = { navController.popBackStack() })
        }
        composable("about") {
            AboutScreen(
                onOpenThirdPartyLicenses = { navController.navigate("third-party-licenses") },
                onBack = { navController.popBackStack() },
            )
        }
        composable("third-party-licenses") {
            ThirdPartyLicensesScreen(onBack = { navController.popBackStack() })
        }
        composable("add-server") {
            InstanceScreen(
                onContinue = { url ->
                    onSetInstanceUrl(url)
                    navController.navigate("add-server-login") {
                        popUpTo("add-server") { inclusive = true }
                    }
                },
                showCancel = true,
                onCancel = { navController.popBackStack() },
                cloudAlreadyAdded = cloudAlreadyAdded,
            )
        }
        composable("add-server-login") {
            LoginScreen(
                instanceUrl = "",
                onLoggedIn = {
                    navController.navigate("home") { popUpTo("home") { inclusive = true } }
                },
                onChangeInstance = { navController.popBackStack() },
            )
        }
        composable("server/{accountId}") { entry ->
            val accountId = entry.arguments?.getString("accountId").orEmpty()
            ServerDetailScreen(accountId = accountId, onBack = { navController.popBackStack() })
        }
        composable("team-settings") {
            TeamSettingsScreen(onBack = { navController.popBackStack() })
        }
        composable("share-compose") {
            // Single-screen share composer: the prefilled create form with the
            // "Share to" destination selector on top (EXP-60). The pending
            // share lives in the TeamSelection singleton (not route
            // state) so backing out and re-entering re-fills the form; it's
            // consumed exactly once — on a successful create or an explicit
            // discard.
            val pendingShare by teamSelection.pendingShare.collectAsStateWithLifecycle()
            val sharePrefill = remember(pendingShare) { pendingShare?.let { buildSharePrefill(it) } }
            val shareVm: ShareTargetPickerViewModel = hiltViewModel()
            val shareState by shareVm.state.collectAsStateWithLifecycle()
            CreateIssueScreen(
                onBack = { navController.popBackStack() },
                sharePrefill = sharePrefill,
                onSharePrefillConsumed = { teamSelection.consumePendingShare() },
                shareMode = true,
                shareGroups = shareState.groups,
                shareRecentBoardId = shareState.recentBoardId,
                shareGroupsLoading = shareState.isLoading,
            )
        }
        composable("board/{boardId}") { entry ->
            val boardId = entry.arguments?.getString("boardId").orEmpty()
            // Remembering the opened board drives the share picker's default.
            LaunchedEffect(boardId) {
                if (boardId.isNotBlank() && activeAccountId != null) {
                    teamSelection.rememberLastBoard(activeAccountId, boardId)
                }
            }
            IssueListScreen(
                boardId = boardId,
                mode = IssueListMode.Pushed,
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
            )
        }
        composable("board/{boardId}/new") {
            // The pending share lives in the TeamSelection singleton (not
            // route state), so backing out of this screen and re-entering
            // re-fills the form. The screen consumes it exactly once — on a
            // successful create or an explicit discard.
            val pendingShare by teamSelection.pendingShare.collectAsStateWithLifecycle()
            val sharePrefill = remember(pendingShare) { pendingShare?.let { buildSharePrefill(it) } }
            CreateIssueScreen(
                onBack = { navController.popBackStack() },
                sharePrefill = sharePrefill,
                onSharePrefillConsumed = { teamSelection.consumePendingShare() },
            )
        }
        composable("support/{threadId}") {
            // A support ticket's conversation (EXP-180) — reached from the
            // Support tab's inbox or a support_reply push tap. The
            // ViewModel reads threadId from its SavedStateHandle like the
            // issue-detail route.
            SupportThreadScreen(
                onBack = { navController.popBackStack() },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
            )
        }
        composable("issue/{issueId}") { entry ->
            val issueId = entry.arguments?.getString("issueId").orEmpty()
            IssueDetailScreen(
                issueId = issueId,
                onBack = { navController.popBackStack() },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
                onOpenChanges = { navController.navigate("issue/$issueId/changes") },
            )
        }
        composable("issue/{issueId}/changes") {
            // Dedicated diff page (EXP-34): PR/branch changes with per-file
            // expandable unified patches.
            ChangesScreen(
                onBack = { navController.popBackStack() },
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
            )
        }
        composable("steer/{codingSessionId}") {
            // The chat-style agent session viewer (EXP-32) — replaced the old
            // live-terminal mirror; the route string is unchanged.
            AgentSessionScreen(onBack = { navController.popBackStack() })
        }
        composable("invite/{token}") { entry ->
            val token = entry.arguments?.getString("token").orEmpty()
            InviteAcceptScreen(
                token = token,
                onBack = { navController.popBackStack() },
                onAccepted = {
                    navController.navigate("home") { popUpTo("home") { inclusive = true } }
                },
            )
        }
    }

    // A background server that 426'd this build (REV2-18): only the ACTIVE
    // account's gate blocks the app, but that account's sync IS stopped, so
    // say so rather than let it read as frozen sync. Dismissable for this app
    // run; sign-out lives on the server's row in Settings.
    var gatedBannerDismissed by remember(gatedOtherServers) { mutableStateOf(false) }
    if (gatedOtherServers.isNotEmpty() && !gatedBannerDismissed && !needsOnboarding) {
        GatedServersBanner(
            servers = gatedOtherServers,
            onDismiss = { gatedBannerDismissed = true },
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = if (barVisible) BottomBarInset else 0.dp),
        )
    }

    if (barVisible) {
        BottomNavBar(
            issuesActive = currentRoute == "home",
            searchActive = currentRoute == "search",
            agentsActive = currentRoute == "agents",
            personalActive = currentRoute == "personal",
            reviewsActive = currentRoute == "reviews",
            supportActive = currentRoute == "support-inbox",
            unreadCount = unreadCount,
            agentsRunning = agentsRunning,
            agentsNeedInput = agentsNeedInput,
            reviewsOpen = reviewsOpen,
            showsSupport = helpdeskEnabled,
            supportUnread = supportUnread,
            showsCompose = composeBoardId != null,
            onIssues = { navController.popBackStack("home", inclusive = false) },
            onSearch = {
                if (currentRoute != "search") {
                    navController.navigate("search") {
                        launchSingleTop = true
                        popUpTo("home")
                    }
                }
            },
            onAgents = {
                if (currentRoute != "agents") {
                    navController.navigate("agents") {
                        launchSingleTop = true
                        popUpTo("home")
                    }
                }
            },
            onPersonal = {
                if (currentRoute != "personal") {
                    navController.navigate("personal") {
                        launchSingleTop = true
                        popUpTo("home")
                    }
                }
            },
            onReviews = {
                if (currentRoute != "reviews") {
                    navController.navigate("reviews") {
                        launchSingleTop = true
                        popUpTo("home")
                    }
                }
            },
            onSupport = {
                if (currentRoute != "support-inbox") {
                    navController.navigate("support-inbox") {
                        launchSingleTop = true
                        popUpTo("home")
                    }
                }
            },
            onCompose = {
                composeBoardId?.let { navController.navigate("board/$it/new") }
            },
            modifier = Modifier.align(Alignment.BottomCenter),
        )
    }
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
        modifier = modifier
            .navigationBarsPadding()
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .glassButton(opaque = true)
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

// --- Hilt EntryPoint accessors for app-singletons consumed inside composables.

@Composable
private fun applicationDeepLinkBus(): DeepLinkBus {
    val app = LocalContext.current.applicationContext as ExponentialApp
    return EntryPointAccessors.fromApplication(app, DeepLinkEntryPoint::class.java).deepLinkBus()
}

@Composable
private fun applicationTeamSelection(): TeamSelection {
    val app = LocalContext.current.applicationContext as ExponentialApp
    return EntryPointAccessors
        .fromApplication(app, TeamSelectionEntryPoint::class.java)
        .teamSelection()
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
private interface TeamSelectionEntryPoint {
    fun teamSelection(): TeamSelection
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface WebLinkResolverEntryPoint {
    fun webLinkResolver(): WebLinkResolver
}
