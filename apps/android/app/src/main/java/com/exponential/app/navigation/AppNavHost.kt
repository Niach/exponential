package com.exponential.app.navigation

import android.net.Uri
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.AnimatedContentTransitionScope.SlideDirection
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Box
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
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.exponential.app.AppConstants
import com.exponential.app.AppViewModel
import com.exponential.app.ExponentialApp
import com.exponential.app.data.TeamSelection
import com.exponential.app.domain.TeamLiveRuns
import com.exponential.app.domain.workFaceFromParam
import com.exponential.app.data.electric.SyncHealth
import androidx.browser.customtabs.CustomTabsIntent
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.push.WebLinkResolver
import com.exponential.app.domain.AGENT_ROUTE
import com.exponential.app.domain.AGENT_ROUTE_ARGS
import com.exponential.app.domain.AGENT_ROUTE_PATTERN
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.agentRoute
import com.exponential.app.ui.agent.AgentScreen
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.BottomBarSuppression
import com.exponential.app.ui.components.BottomNavBar
import com.exponential.app.ui.components.LocalBottomBarSuppression
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.ToastHost
import com.exponential.app.ui.components.Toaster
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.invite.InviteAcceptScreen
import com.exponential.app.ui.issue.IssueDraftScreen
import com.exponential.app.ui.onboarding.OnboardingScreen
import com.exponential.app.ui.personal.PersonalScreen
import com.exponential.app.ui.reviews.ReviewsScreen
import com.exponential.app.domain.ReviewsNav
import com.exponential.app.ui.issue.IssueListMode
import com.exponential.app.ui.issue.IssueListScreen
import com.exponential.app.ui.actions.ActionDetailScreen
import com.exponential.app.ui.actions.ActionsScreen
import com.exponential.app.ui.search.SearchScreen
import com.exponential.app.ui.work.RunChangesScreen
import com.exponential.app.ui.work.WorkScreen
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.ui.work.WorkSubject
import com.exponential.app.ui.session.AgentsScreen
import com.exponential.app.ui.settings.AboutScreen
import com.exponential.app.ui.settings.ServerDetailScreen
import com.exponential.app.ui.settings.SettingsScreen
import com.exponential.app.ui.settings.SyncDiagnosticsScreen
import com.exponential.app.ui.settings.TeamSettingsScreen
import com.exponential.app.ui.settings.ThirdPartyLicensesScreen
import com.exponential.app.ui.share.ShareTargetPickerViewModel
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
 * [AppBackground] behind one push-stack `NavHost`, with the floating bottom
 * pill (Issues · Inbox · Devices · Reviews · Actions + the Chat | New issue
 * capsule) overlaid on the top-level routes; the app lands on the Agent page,
 * which the Chat arm selects like a tab. EXP-1210: the tabs are ROOT siblings
 * ([MainTabs], [selectTab]) — no back button, no slide between them. Replaces
 * the inline graph + `MainScaffold` drawer shell that used to live in
 * MainActivity.
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
        else -> AGENT_TAB_ROUTE
    }

    LaunchedEffect(pendingTarget, state.token) {
        val target = pendingTarget ?: return@LaunchedEffect
        // Leave the target in the bus while unauthenticated so a share/deep-link
        // received before login resumes once the token lands (token is a key).
        if (state.token == null) return@LaunchedEffect
        when (target) {
            // A real push, never launchSingleTop (EXP-528): single top REPLACES
            // the top entry and the replacement keeps its id, so its
            // ViewModelStore — and an IssueDetailViewModel that read issueId
            // once from SavedStateHandle — survived, and a tap that arrived
            // while ANOTHER issue was open re-showed that issue. navigateDeepLink
            // keeps the only thing single top bought us: a re-tap for what is
            // already on screen stays a no-op.
            is DeepLinkBus.Target.Issue -> guarded {
                navController.navigateIssueDeepLink(target.id, target.face)
            }
            is DeepLinkBus.Target.Invite -> guarded {
                navController.navigateDeepLink("invite/${target.token}")
            }
            // An agent's message (EXP-801) renders in My Work's inbox — the
            // segment the screen opens on unless the user last left it on
            // My Issues.
            DeepLinkBus.Target.Inbox -> guarded {
                navController.selectTab(MainTabs.INBOX, tabSwitch, restore = false)
            }
            // EXP-980: a blocked run's push tap lands on the run itself. Same
            // snapshot hazard as the issue route — the session screen reads
            // its id from SavedStateHandle once.
            is DeepLinkBus.Target.Session -> guarded {
                navController.navigateDeepLink("steer/${target.id}")
            }
            // EXP-825: the web's `/t/{team}/agent` — the composer, empty,
            // which IS the Agent tab.
            DeepLinkBus.Target.Agent -> guarded {
                navController.selectTab(MainTabs.AGENT, tabSwitch, restore = false)
            }
            is DeepLinkBus.Target.WebIssueRef ->
                // Verified App Link (EXP-92): resolve slug+identifier against
                // the local DB of the account matching the link's host (brief
                // poll while sync lands fresh rows). Anything unresolvable
                // opens in a Custom Tab — which never re-triggers App Links,
                // so it can't loop back here.
                when (val resolution = webLinkResolver.resolve(target)) {
                    is WebLinkResolver.Resolution.Found -> guarded {
                        if (resolution.accountId != state.activeAccountId) {
                            // The issue lives under another signed-in account:
                            // switch first; IssueDetail re-scopes reactively.
                            viewModel.switchAccount(resolution.accountId)
                        }
                        navController.navigateIssueDeepLink(resolution.issueId, rawFace = null)
                    }
                    WebLinkResolver.Resolution.NotFound ->
                        CustomTabsIntent.Builder().build().launchUrl(context, target.uri)
                }
            // A claimed-but-unrenderable https link (MainActivity's null
            // parse): straight to the Custom Tab, which never re-triggers
            // App Links.
            is DeepLinkBus.Target.WebUrl ->
                CustomTabsIntent.Builder().build().launchUrl(context, target.uri)
            is DeepLinkBus.Target.ShareContent -> {
                // Stash the shared content for the single-screen share composer
                // to consume (it carries its own inline board selector).
                guarded {
                    teamSelection.setPendingShare(target)
                    navController.navigate("share-compose") { launchSingleTop = true }
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

    // EXP-1031: the app's ONE toaster, above both nav graphs, drawn by ONE
    // ToastHost at the top (below the status bar), where no bottom sheet,
    // nav bar or keyboard ever covers it.
    val toaster = remember { Toaster() }

    AppBackground {
        // Every screen floats on AppBackground (a Box, not a Material Surface), so
        // without this provider bare `Text`/`Icon` would inherit LocalContentColor's
        // black default and render near-invisible on the dark gradient. Anchor the
        // default to onSurface (light) app-wide; explicit colors still win.
        CompositionLocalProvider(
            LocalContentColor provides MaterialTheme.colorScheme.onSurface,
            LocalToaster provides toaster,
        ) {
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
                    navController.navigate(AGENT_TAB_ROUTE) { popUpTo("login") { inclusive = true } }
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
            // EXP-1075: the board switcher sheet's per-team live-run dots.
            val liveRunsByTeam by viewModel.liveRunsByTeam.collectAsStateWithLifecycle()
            val reviewsNav by viewModel.reviewsNav.collectAsStateWithLifecycle()
            val currentBoardId by viewModel.currentBoardId.collectAsStateWithLifecycle()
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
                agentsRunning = agentsRunning,
                agentsNeedInput = agentsNeedInput,
                liveRunsByTeam = liveRunsByTeam,
                reviewsNav = reviewsNav,
                currentBoardId = currentBoardId,
                onSetInstanceUrl = { viewModel.setInstanceUrl(it) },
                onRetrySync = { viewModel.retrySync() },
            )
        }
        // Drawn last, so it overlays every screen's header (sonner
        // top-center does the same); only the cards take touches.
        ToastHost(toaster, Modifier.align(Alignment.TopCenter))
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
    leaveGuard: LeaveGuard,
    guarded: (() -> Unit) -> Unit,
    tabSwitch: TabSwitchMarker,
    cloudAlreadyAdded: Boolean,
    activeAccountId: String?,
    gatedOtherServers: List<String>,
    syncHealth: SyncHealth,
    needsOnboarding: Boolean,
    unreadCount: Int,
    agentsRunning: Boolean,
    agentsNeedInput: Boolean,
    liveRunsByTeam: Map<String, TeamLiveRuns>,
    reviewsNav: ReviewsNav,
    currentBoardId: String?,
    onSetInstanceUrl: (String) -> Unit,
    onRetrySync: () -> Unit,
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
    // EXP-1210: the tab whose stack is up — a detail pushed inside a tab
    // still lights that tab.
    val currentTab = remember(backStackEntry) { navController.currentTab() }
    val barVisible = !needsOnboarding &&
        (currentRoute in MainTabs.routes || currentRoute == "board/{boardId}")
    // EXP-698 r5 (Mechanism A): a screen may claim the tab bar's slot for a
    // bar of its own — today the issue list's multi-select bar. The switch is
    // provided to the whole NavHost; the chrome below reads it directly.
    val barSuppression = remember { BottomBarSuppression() }
    val barShown = barVisible && !barSuppression.suppressed

    // EXP-1105/EXP-1244: yolo mode hides the Reviews tab unless the Reviews
    // queue is non-empty (in yolo mode an open PR = a failed auto-merge, which
    // must still surface) — `ReviewsQueue.nav` over every member team. If it
    // flips off while the Reviews tab is up, switch to Issues instead of
    // stranding a tab-less screen, and drop its saved stack — ONLY on a
    // true→false TRANSITION of the flag (iOS AppNavigator's `.onChange` parity, REV2-2):
    // the flag recomputes through a fresh Room flow that can never emit
    // synchronously, so a guard re-run on the route change alone would read
    // the PREVIOUS team's stale value and bounce a tap straight back to Issues.
    val showsReviews = reviewsNav.shows
    var hadReviews by remember { mutableStateOf(showsReviews) }
    LaunchedEffect(showsReviews) {
        val flippedOff = hadReviews && !showsReviews
        hadReviews = showsReviews
        if (flippedOff) {
            val onReviews = navController.currentTab() == MainTabs.REVIEWS
            if (onReviews) navController.selectTab(MainTabs.ISSUES, tabSwitch)
            navController.clearBackStack(MainTabs.REVIEWS)
        }
    }
    // EXP-825: every launcher entry point is NAVIGATION onto the Agent page
    // with a preselection seed — every play button with what it acts on.
    // The seed rides the route string, so the concrete route differs per seed:
    // an EMPTY seed IS the Agent tab (the bar's chat arm), and a seeded tap reuses the
    // deep-link rule — a no-op when that exact seeded route is already on top,
    // else a NEW entry whose ViewModel reads the new seed (single top would
    // keep the old entry's ViewModel, which read its seed once — EXP-528).
    val openAgent: (AgentComposerSeed) -> Unit = { seed ->
        val route = agentRoute(seed)
        if (route == AGENT_ROUTE) {
            navController.selectTab(MainTabs.AGENT, tabSwitch)
        } else {
            navController.navigateDeepLink(route)
        }
    }
    // EXP-1121: the "Ready to code?" fixes. Team settings shows the SELECTED
    // team, so the issue's team is selected first (a no-op when it already
    // is); it is PUSHED over the issue, so Back returns to Start coding.
    // Devices is a tab (EXP-1210): a switch; an issue opened inside a
    // non-Agent tab comes back with that tab's restored stack.
    val openReadinessTeamSettings: (String) -> Unit = { teamId ->
        if (teamSelection.selectedId.value != teamId) teamSelection.select(teamId)
        navController.navigate("team-settings") { launchSingleTop = true }
    }
    val openReadinessDevices: () -> Unit = {
        navController.selectTab(MainTabs.DEVICES, tabSwitch)
    }

    // The single add-issue affordance. EXP-973: it rides EVERY tab, not just
    // the board ones — a pushed board route still wins (the reader is looking
    // at that board), anything else files onto the team's current board.
    val composeBoardId = when (currentRoute) {
        "board/{boardId}" -> backStackEntry?.arguments?.getString("boardId")
        else -> currentBoardId
    }

    // EXP-523: the four transitions below are plain lambdas, not composable
    // ones, so the reduce-motion flag is read here and captured. `Motion.slow`
    // is the shared 280ms token these were already hand-set to — the only
    // behaviour change is that a user who turned animations off now gets none.
    val reduceMotion = LocalReduceMotion.current
    val pushSpec = Motion.slow<IntOffset>(reduceMotion)

    // EXP-920: the ONE navigator an entity-preview sheet's Open rides — every
    // kind's detail surface behind one local, so the transcript never threads
    // a lambda per kind.
    val entityNavigator = remember(navController, tabSwitch) {
        EntityNavigator { target -> guarded {
            when (target) {
                is EntityTarget.Issue -> navController.navigate("issue/${target.id}")
                is EntityTarget.Board -> navController.navigate("board/${target.id}")
                is EntityTarget.Session -> navController.navigate("steer/${target.id}")
                // SLOP-2: an `action` ref opens that action's page.
                is EntityTarget.Action -> navController.navigate("action/${target.id}")
                // EXP-1210: a tab is never pushed — its ref switches to it.
                EntityTarget.Actions -> navController.selectTab(MainTabs.ACTIONS, tabSwitch)
                EntityTarget.Devices -> navController.selectTab(MainTabs.DEVICES, tabSwitch)
                EntityTarget.TeamSettings -> navController.navigate("team-settings")
                EntityTarget.Inbox -> navController.selectTab(MainTabs.INBOX, tabSwitch)
            }
        } }
    }

    Box(modifier = Modifier.fillMaxSize()) {
    CompositionLocalProvider(
        LocalBottomBarSuppression provides barSuppression,
        LocalEntityNavigator provides entityNavigator,
        LocalLeaveGuard provides leaveGuard,
    ) {
    NavHost(
        navController = navController,
        // The Agent tab is the stack ROOT (where the app lands); every other
        // tab is one entry above it, so Back from a tab returns to Agent.
        startDestination = if (needsOnboarding) "onboarding" else AGENT_TAB_ROUTE,
        // iOS-style horizontal push/pop transitions — except a tab switch
        // (EXP-1210): tab roots are siblings, so moving between them (and
        // Back from a tab root to the Agent root) swaps with no transition.
        enterTransition = {
            if (isTabSwap(tabSwitch)) EnterTransition.None
            else slideIntoContainer(SlideDirection.Start, pushSpec)
        },
        exitTransition = {
            if (isTabSwap(tabSwitch)) ExitTransition.None
            else slideOutOfContainer(SlideDirection.Start, pushSpec)
        },
        popEnterTransition = {
            if (isTabSwap(tabSwitch)) EnterTransition.None
            else slideIntoContainer(SlideDirection.End, pushSpec)
        },
        popExitTransition = {
            if (isTabSwap(tabSwitch)) ExitTransition.None
            else slideOutOfContainer(SlideDirection.End, pushSpec)
        },
    ) {
        composable("onboarding") {
            OnboardingScreen(
                onDone = {
                    navController.navigate(AGENT_TAB_ROUTE) { popUpTo("onboarding") { inclusive = true } }
                },
            )
        }
        composable(AGENT_TAB_ROUTE) {
            // The Agent TAB: the composer with an EMPTY seed (its ViewModel
            // finds no args on this route), no back button, the bar over it.
            AgentScreen(
                onBack = null,
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
            )
        }
        composable("home") {
            // The Issues tab root: the current board's list with the inline
            // switcher; picking another board swaps it in place (no push).
            val selectedTeamId by teamSelection.selectedId.collectAsStateWithLifecycle()
            IssueListScreen(
                boardId = currentBoardId,
                mode = IssueListMode.Root,
                // EXP-1075: the switcher SHEET marks the teams holding MY
                // live runs (EXP-1210: the pill wears no dot).
                liveRunsByTeam = liveRunsByTeam,
                selectedTeamId = selectedTeamId,
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenSettings = { navController.navigate("settings") },
                onOpenAgent = openAgent,
                // EXP-686: search left the bottom bar — the board header's
                // button pushes it instead.
                onOpenSearch = { navController.navigate("search") { launchSingleTop = true } },
                // EXP-698 r5: the getting-started cards under the empty
                // states — each step's own surface, plus the compose form the
                // hidden FAB would have opened.
                onOpenTeamSettings = { navController.navigate("team-settings") },
                onOpenDevices = { navController.selectTab(MainTabs.DEVICES, tabSwitch) },
                onOpenActions = { navController.selectTab(MainTabs.ACTIONS, tabSwitch) },
                onNewIssue = {
                    currentBoardId?.let { navController.openIssueDraft(it) }
                },
            )
        }
        composable("search") {
            // EXP-686: a pushed detail route now (not a tab), so it carries a
            // back button and the bar yields the full height.
            SearchScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
            )
        }
        composable("agents") {
            // Devices — machines only since EXP-825, and since the EXP-909
            // follow-up they carry no launcher at all: a row's ONE control is
            // the settings gear, and runs start from the Agent page composer.
            AgentsScreen()
        }
        composable("actions") {
            // Team actions (EXP-253) — its own bottom-bar tab since EXP-686.
            ActionsScreen(
                onOpenAgent = openAgent,
                onOpenAction = { id -> navController.navigate("action/$id") },
            )
        }
        composable("action/{actionId}") {
            // SLOP-2: one action — Prompt | Triggers | Runs on a pager. The
            // ViewModel reads actionId from its SavedStateHandle like the
            // issue-detail route does; a run opened from Runs pops back here.
            ActionDetailScreen(
                onBack = { navController.popBackStack() },
                onOpenAgent = openAgent,
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
            )
        }
        composable(
            // EXP-825: the SEEDED Agent page — a pushed detail over any tab
            // (the empty one is AGENT_TAB_ROUTE) whose six nullable query args carry the preselection seed
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
            AgentScreen(
                onBack = { navController.popBackStack() },
                onOpenSteer = { sessionId -> navController.navigate("steer/$sessionId") },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
            )
        }
        composable("personal") {
            // "Inbox" (SLOP-5; "My Work" until then) — Inbox + My Issues
            // merged into one board-independent personal tab (EXP-58). Notification taps never land here directly
            // (pushes deep-link straight to issue/{id}), so renaming the old
            // "inbox" route is safe.
            PersonalScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                // EXP-878/1170: a draft row reopens the New issue page.
                onOpenDraft = { boardId, draftId ->
                    navController.openIssueDraft(boardId, draftId = draftId)
                },
                // EXP-980: a blocked-run row opens the run it is about.
                onOpenSession = { sessionId -> navController.navigate("steer/$sessionId") },
                // EXP-933: an agent message's issue row → its Guide face.
                onOpenIssueGuide = { id -> navController.navigate("issue/$id?face=guide") },
            )
        }
        composable("reviews") {
            // Reviews — its own bottom-bar destination beside My Work
            // (EXP-147; it used to be a PersonalScreen segment). EXP-1154: rows
            // open the issue's Work screen on its Guide face (the review IS
            // the issue); rows carry no long-press.
            ReviewsScreen(
                onOpenChanges = { id -> navController.navigate("issue/$id?face=guide") },
                // EXP-1194: an Agent runs row opens the run's own PR in OUR diff UI.
                onOpenRunChanges = { id -> navController.navigate("runChanges/$id") },
            )
        }
        composable("runChanges/{sessionId}") { entry ->
            // EXP-1194: the Guide of a run's own issue-less PR.
            val sessionId = entry.arguments?.getString("sessionId").orEmpty()
            RunChangesScreen(
                sessionId = sessionId,
                onBack = { navController.popBackStack() },
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
                    navController.resetToAgentRoot()
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
            // EXP-1170: the New issue page in share mode (no draft row).
            IssueDraftScreen(
                onBack = { navController.popBackStack() },
                onCreated = { issueId -> navController.openCreatedIssue(issueId) },
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
                onOpenAgent = openAgent,
                onOpenSearch = { navController.navigate("search") { launchSingleTop = true } },
                onNewIssue = { navController.openIssueDraft(boardId) },
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
            ),
        ) {
            IssueDraftScreen(
                onBack = { navController.popBackStack() },
                onCreated = { issueId -> navController.openCreatedIssue(issueId) },
            )
        }
        composable(
            ISSUE_ROUTE,
            arguments = listOf(
                navArgument("face") {
                    type = NavType.StringType
                    nullable = true
                    defaultValue = null
                },
            ),
        ) { entry ->
            // EXP-893: the Work screen on its Issue face — the run and the
            // diff are FACES of the same screen, never routes. `?face=guide`
            // (an agent message's inbox row or push, a Reviews row) opens it on
            // the Guide; the old words results/changes/diff land there too.
            val issueId = entry.arguments?.getString("issueId").orEmpty()
            val initialFace = entry.arguments?.getString("face")?.let { face ->
                workFaceFromParam(face)
            }
            WorkScreen(
                subject = WorkSubject.Issue(issueId),
                initialFace = initialFace,
                onBack = { navController.popBackStack() },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenIssueChanges = { id -> navController.navigate("issue/$id?face=guide") },
                onOpenAgent = openAgent,
                onOpenTeamSettings = openReadinessTeamSettings,
                onOpenDevices = openReadinessDevices,
                onCreateSubIssue = { boardId, parentId ->
                    // Single-top: a double-tap on `+` pushes ONE page.
                    navController.openIssueDraft(boardId, parentId = parentId, singleTop = true)
                },
            )
        }
        composable("steer/{codingSessionId}") { entry ->
            // EXP-893: the Work screen on its Run face (EXP-32's viewer);
            // the route string is unchanged, push taps and lists land here.
            val sessionId = entry.arguments?.getString("codingSessionId").orEmpty()
            WorkScreen(
                subject = WorkSubject.Session(sessionId),
                onBack = { navController.popBackStack() },
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenIssueChanges = { id -> navController.navigate("issue/$id?face=guide") },
                onOpenAgent = openAgent,
                onOpenTeamSettings = openReadinessTeamSettings,
                onOpenDevices = openReadinessDevices,
                onCreateSubIssue = { boardId, parentId ->
                    // Single-top: a double-tap on `+` pushes ONE page.
                    navController.openIssueDraft(boardId, parentId = parentId, singleTop = true)
                },
            )
        }
        composable("invite/{token}") { entry ->
            val token = entry.arguments?.getString("token").orEmpty()
            InviteAcceptScreen(
                token = token,
                onBack = { navController.popBackStack() },
                onAccepted = {
                    navController.resetToAgentRoot()
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

    AnimatedVisibility(
        visible = barShown,
        enter = slideInVertically(Motion.standard(reduceMotion)) { it } +
            fadeIn(Motion.standard(reduceMotion)),
        exit = slideOutVertically(Motion.standard(reduceMotion)) { it } +
            fadeOut(Motion.standard(reduceMotion)),
        modifier = Modifier.align(Alignment.BottomCenter),
    ) {
        BottomNavBar(
            agentActive = currentTab == MainTabs.AGENT,
            issuesActive = currentTab == MainTabs.ISSUES,
            devicesActive = currentTab == MainTabs.DEVICES,
            actionsActive = currentTab == MainTabs.ACTIONS,
            personalActive = currentTab == MainTabs.INBOX,
            reviewsActive = currentTab == MainTabs.REVIEWS,
            unreadCount = unreadCount,
            agentsRunning = agentsRunning,
            agentsNeedInput = agentsNeedInput,
            reviewsOpen = reviewsNav.dot,
            showsReviews = showsReviews,
            // The Chat arm (the Agent page, with its sessions list and live
            // dot) rides every top-level surface, with New issue beside it in
            // one capsule (EXP-827/EXP-973) — dimmed while the team has no
            // board to file onto. The arm SWITCHES to the Agent page like a
            // tab (it is the start destination, never a push) and reads
            // selected while it is up.
            composeEnabled = composeBoardId != null,
            // EXP-1210: every entry SWITCHES to a sibling root (a re-tap pops
            // back to it); EXP-1187: Actions is a top-level tab (Settings = the
            // Issues header's gear).
            onChat = { navController.selectTab(MainTabs.AGENT, tabSwitch) },
            onIssues = { navController.selectTab(MainTabs.ISSUES, tabSwitch) },
            onDevices = { navController.selectTab(MainTabs.DEVICES, tabSwitch) },
            onActions = { navController.selectTab(MainTabs.ACTIONS, tabSwitch) },
            onPersonal = { navController.selectTab(MainTabs.INBOX, tabSwitch) },
            onReviews = { navController.selectTab(MainTabs.REVIEWS, tabSwitch) },
            onCompose = {
                composeBoardId?.let { navController.openIssueDraft(it) }
            },
        )
    }
    }
}

/**
 * The Agent TAB — the stack root and launch landing. A distinct route from the
 * seeded `agent?…` page (AGENT_ROUTE_PATTERN), so tab pops never land on a
 * seeded entry.
 */
private const val AGENT_TAB_ROUTE = MainTabs.AGENT

/** EXP-1210: a move the NavHost draws with no transition (see [MainTabs]). */
private fun AnimatedContentTransitionScope<NavBackStackEntry>.isTabSwap(
    tabSwitch: TabSwitchMarker,
): Boolean =
    MainTabs.isRootSwap(initialState.destination.route, targetState.destination.route) ||
        tabSwitch.matches(initialState, targetState)

/** EXP-1170: the New issue page — `drafts/{draftId}?board=&status=&parent=`. */
private const val ISSUE_DRAFT_ROUTE = "drafts/{draftId}?board={board}&status={status}&parent={parent}"

/**
 * Open the New issue page. The draft id is minted HERE, at tap time, so the
 * page and its autosave own one stable id from the first frame (EXP-1170).
 */
private fun NavHostController.openIssueDraft(
    boardId: String,
    statusId: String? = null,
    parentId: String? = null,
    draftId: String = java.util.UUID.randomUUID().toString(),
    singleTop: Boolean = false,
) {
    val query = listOfNotNull(
        "board=${Uri.encode(boardId)}",
        statusId?.let { "status=${Uri.encode(it)}" },
        parentId?.let { "parent=${Uri.encode(it)}" },
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
