package com.exponential.app.navigation

import androidx.compose.animation.AnimatedContentTransitionScope.SlideDirection
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
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
import com.exponential.app.data.WorkspaceSelection
import androidx.browser.customtabs.CustomTabsIntent
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.push.WebLinkResolver
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.components.BottomNavBar
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
import com.exponential.app.ui.search.SearchScreen
import com.exponential.app.ui.session.AgentSessionScreen
import com.exponential.app.ui.session.AgentsScreen
import com.exponential.app.ui.settings.ServerDetailScreen
import com.exponential.app.ui.settings.SettingsScreen
import com.exponential.app.ui.settings.SyncDiagnosticsScreen
import com.exponential.app.ui.settings.WorkspaceSettingsScreen
import com.exponential.app.ui.share.ShareTargetPickerViewModel
import com.exponential.app.ui.share.buildSharePrefill
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.update.UpdateRequiredScreen
import dagger.hilt.android.EntryPointAccessors

/**
 * The single navigation surface, mirroring the iOS `AppNavigator`: a gradient
 * [AppBackground] behind one push-stack `NavHost`, with the floating bottom
 * pill (Issues · My Work · Reviews · Agents · Search + compose FAB) overlaid
 * on the top-level routes. Replaces the inline graph + `MainScaffold` drawer
 * shell that used to live in MainActivity.
 */
@Composable
fun AppNavHost() {
    val viewModel: AppViewModel = hiltViewModel()
    val deepLinkBus = applicationDeepLinkBus()
    val workspaceSelection = applicationWorkspaceSelection()
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
                // to consume (it carries its own inline project selector).
                workspaceSelection.setPendingShare(target)
                navController.navigate("share-compose") { launchSingleTop = true }
            }
            is DeepLinkBus.Target.GithubConnected ->
                // Not a navigation target — the open GithubRepoPicker sheet
                // consumes it and re-fetches. Leave it in the bus (a later deep
                // link simply overwrites it if no picker is up).
                return@LaunchedEffect
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
            // Highest priority: the server has 426'd this build (below its
            // minimum version, EXP-104). Replace the whole NavHost with the
            // blocking update screen — no navigation, no authed requests.
            UpdateRequiredScreen(info = updateRequired)
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
            val currentProjectId by viewModel.currentProjectId.collectAsStateWithLifecycle()
            AuthenticatedNav(
                navController = navController,
                cloudAlreadyAdded = cloudAlreadyAdded,
                activeAccountId = state.activeAccountId,
                needsOnboarding = needsOnboarding,
                unreadCount = unreadCount,
                agentsRunning = agentsRunning,
                currentProjectId = currentProjectId,
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
    needsOnboarding: Boolean,
    unreadCount: Int,
    agentsRunning: Boolean,
    currentProjectId: String?,
    onSetInstanceUrl: (String) -> Unit,
) {
    val workspaceSelection = applicationWorkspaceSelection()

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
    // last-opened project — its current-project resolution starts there.)

    // Linear-style floating bottom bar over the top-level routes only; detail
    // and settings screens get the full height back.
    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = backStackEntry?.destination?.route
    val barVisible = !needsOnboarding &&
        currentRoute in setOf("home", "search", "agents", "personal", "reviews", "project/{projectId}")
    // The single add-issue affordance: the FAB shows while a project is in
    // view — the Issues tab root (its resolved current project) or a pushed
    // project route — so it always targets the project on screen.
    val composeProjectId = when (currentRoute) {
        "project/{projectId}" -> backStackEntry?.arguments?.getString("projectId")
        "home" -> currentProjectId
        else -> null
    }

    Box(modifier = Modifier.fillMaxSize()) {
    NavHost(
        navController = navController,
        startDestination = if (needsOnboarding) "onboarding" else "home",
        // iOS-style horizontal push/pop transitions.
        enterTransition = { slideIntoContainer(SlideDirection.Start, tween(280)) },
        exitTransition = { slideOutOfContainer(SlideDirection.Start, tween(280)) },
        popEnterTransition = { slideIntoContainer(SlideDirection.End, tween(280)) },
        popExitTransition = { slideOutOfContainer(SlideDirection.End, tween(280)) },
    ) {
        composable("onboarding") {
            OnboardingScreen(
                onDone = {
                    navController.navigate("home") { popUpTo("onboarding") { inclusive = true } }
                },
            )
        }
        composable("home") {
            // The Issues tab root: the current project's list with the inline
            // switcher; picking another project swaps it in place (no push).
            IssueListScreen(
                projectId = currentProjectId,
                mode = IssueListMode.Root,
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenSettings = { navController.navigate("settings") },
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
            )
        }
        composable("personal") {
            // "My Work" — Inbox + My Issues merged into one project-independent
            // personal tab (EXP-58). Notification taps never land here directly
            // (pushes deep-link straight to issue/{id}), so renaming the old
            // "inbox" route is safe.
            PersonalScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
            )
        }
        composable("reviews") {
            // Reviews — its own bottom-bar destination beside My Work
            // (EXP-147; it used to be a PersonalScreen segment). Rows open the
            // Review detail (EXP-168); the long-press sheet keeps issue access.
            ReviewsScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onOpenChanges = { id -> navController.navigate("issue/$id/changes") },
            )
        }
        composable("settings") {
            SettingsScreen(
                onOpenServerDetail = { accountId -> navController.navigate("server/$accountId") },
                onOpenWorkspaceSettings = { navController.navigate("workspace-settings") },
                onOpenSyncDiagnostics = { navController.navigate("sync-diagnostics") },
                onAddServer = { navController.navigate("add-server") },
                onBack = { navController.popBackStack() },
            )
        }
        composable("sync-diagnostics") {
            SyncDiagnosticsScreen(onBack = { navController.popBackStack() })
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
        composable("workspace-settings") {
            WorkspaceSettingsScreen(onBack = { navController.popBackStack() })
        }
        composable("share-compose") {
            // Single-screen share composer: the prefilled create form with the
            // "Share to" destination selector on top (EXP-60). The pending
            // share lives in the WorkspaceSelection singleton (not route
            // state) so backing out and re-entering re-fills the form; it's
            // consumed exactly once — on a successful create or an explicit
            // discard.
            val pendingShare by workspaceSelection.pendingShare.collectAsStateWithLifecycle()
            val sharePrefill = remember(pendingShare) { pendingShare?.let { buildSharePrefill(it) } }
            val shareVm: ShareTargetPickerViewModel = hiltViewModel()
            val shareState by shareVm.state.collectAsStateWithLifecycle()
            CreateIssueScreen(
                onBack = { navController.popBackStack() },
                sharePrefill = sharePrefill,
                onSharePrefillConsumed = { workspaceSelection.consumePendingShare() },
                shareMode = true,
                shareGroups = shareState.groups,
                shareRecentProjectId = shareState.recentProjectId,
                shareGroupsLoading = shareState.isLoading,
            )
        }
        composable("project/{projectId}") { entry ->
            val projectId = entry.arguments?.getString("projectId").orEmpty()
            // Remembering the opened project drives the share picker's default.
            LaunchedEffect(projectId) {
                if (projectId.isNotBlank() && activeAccountId != null) {
                    workspaceSelection.rememberLastProject(activeAccountId, projectId)
                }
            }
            IssueListScreen(
                projectId = projectId,
                mode = IssueListMode.Pushed,
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
            )
        }
        composable("project/{projectId}/new") {
            // The pending share lives in the WorkspaceSelection singleton (not
            // route state), so backing out of this screen and re-entering
            // re-fills the form. The screen consumes it exactly once — on a
            // successful create or an explicit discard.
            val pendingShare by workspaceSelection.pendingShare.collectAsStateWithLifecycle()
            val sharePrefill = remember(pendingShare) { pendingShare?.let { buildSharePrefill(it) } }
            CreateIssueScreen(
                onBack = { navController.popBackStack() },
                sharePrefill = sharePrefill,
                onSharePrefillConsumed = { workspaceSelection.consumePendingShare() },
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
            ChangesScreen(onBack = { navController.popBackStack() })
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

    if (barVisible) {
        BottomNavBar(
            issuesActive = currentRoute == "home",
            searchActive = currentRoute == "search",
            agentsActive = currentRoute == "agents",
            personalActive = currentRoute == "personal",
            reviewsActive = currentRoute == "reviews",
            unreadCount = unreadCount,
            agentsRunning = agentsRunning,
            showsCompose = composeProjectId != null,
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
            onCompose = {
                composeProjectId?.let { navController.navigate("project/$it/new") }
            },
            modifier = Modifier.align(Alignment.BottomCenter),
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
private fun applicationWorkspaceSelection(): WorkspaceSelection {
    val app = LocalContext.current.applicationContext as ExponentialApp
    return EntryPointAccessors
        .fromApplication(app, WorkspaceSelectionEntryPoint::class.java)
        .workspaceSelection()
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
private interface WorkspaceSelectionEntryPoint {
    fun workspaceSelection(): WorkspaceSelection
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface WebLinkResolverEntryPoint {
    fun webLinkResolver(): WebLinkResolver
}
