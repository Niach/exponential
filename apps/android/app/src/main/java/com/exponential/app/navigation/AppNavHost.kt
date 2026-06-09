package com.exponential.app.navigation

import androidx.compose.animation.AnimatedContentTransitionScope.SlideDirection
import androidx.compose.animation.core.tween
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.exponential.app.AppConstants
import com.exponential.app.AppViewModel
import com.exponential.app.ExponentialApp
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.home.HomeScreen
import com.exponential.app.ui.inbox.InboxScreen
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.integrations.IntegrationsScreen
import com.exponential.app.ui.invite.InviteAcceptScreen
import com.exponential.app.ui.issue.CreateIssueScreen
import com.exponential.app.ui.onboarding.OnboardingScreen
import com.exponential.app.ui.issue.IssueDetailScreen
import com.exponential.app.ui.issue.IssueListScreen
import com.exponential.app.ui.settings.ServerDetailScreen
import com.exponential.app.ui.settings.SettingsScreen
import com.exponential.app.ui.settings.SyncDiagnosticsScreen
import com.exponential.app.ui.settings.WorkspaceSettingsScreen
import com.exponential.app.ui.share.ShareTargetPickerScreen
import com.exponential.app.ui.share.buildSharePrefill
import com.exponential.app.ui.theme.AppBackground
import dagger.hilt.android.EntryPointAccessors

/**
 * The single navigation surface, mirroring the iOS `AppNavigator`: a gradient
 * [AppBackground] behind one push-stack `NavHost`. No drawer, no rail, no
 * bottom tabs — every destination is a push onto the back stack. Replaces the
 * inline graph + `MainScaffold` drawer shell that used to live in MainActivity.
 */
@Composable
fun AppNavHost() {
    val viewModel: AppViewModel = hiltViewModel()
    val deepLinkBus = applicationDeepLinkBus()
    val workspaceSelection = applicationWorkspaceSelection()
    val state by viewModel.state.collectAsState()
    val navController = rememberNavController()
    val pendingTarget by deepLinkBus.target.collectAsState()

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
            is DeepLinkBus.Target.ShareContent -> {
                // Stash the shared content for the project route to consume, then
                // route to the project picker.
                workspaceSelection.setPendingShare(target)
                navController.navigate("share-pick") { launchSingleTop = true }
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

    // Gate the authenticated graph on onboarding: a brand-new user (no
    // onboardingCompletedAt on the active account, captured from the session at
    // login) starts in the wizard. Persisted, so it resolves synchronously at
    // startup; the key(activeAccountId) rebuild re-evaluates it per account.
    val activeAccount = state.accounts.firstOrNull { it.id == state.activeAccountId }
    val needsOnboarding = activeAccount?.onboardingCompletedAt == null

    AppBackground {
        // Every screen floats on AppBackground (a Box, not a Material Surface), so
        // without this provider bare `Text`/`Icon` would inherit LocalContentColor's
        // black default and render near-invisible on the dark gradient. Anchor the
        // default to onSurface (light) app-wide; explicit colors still win.
        CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.onSurface) {
        if (needsAuth) {
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
            // Account scoping is still a constructor-time snapshot in the
            // feature ViewModels, so a `key` rebuild on account switch keeps
            // the stack pointing at the freshly-scoped DB. (Slated to be
            // replaced by reactive scoping in a later phase.)
            key(state.activeAccountId) {
                AuthenticatedNav(
                    navController = navController,
                    cloudAlreadyAdded = cloudAlreadyAdded,
                    activeAccountId = state.activeAccountId,
                    needsOnboarding = needsOnboarding,
                    onSetInstanceUrl = { viewModel.setInstanceUrl(it) },
                )
            }
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
    onSetInstanceUrl: (String) -> Unit,
) {
    val workspaceSelection = applicationWorkspaceSelection()

    // Consume a cross-server project tap that was pre-set before the
    // `key(activeAccountId)` rebuild (HomeViewModel.onProjectTap), then push.
    val pendingProjectId by workspaceSelection.pendingProjectId.collectAsState()
    LaunchedEffect(pendingProjectId) {
        pendingProjectId?.let { projectId ->
            workspaceSelection.consumePendingProject()
            navController.navigate("project/$projectId") { launchSingleTop = true }
        }
    }
    // Same handoff for a Settings -> Workspaces tap on a workspace that lives
    // on a different server.
    val pendingWorkspaceSettings by workspaceSelection.pendingWorkspaceSettings.collectAsState()
    LaunchedEffect(pendingWorkspaceSettings) {
        if (pendingWorkspaceSettings) {
            workspaceSelection.consumePendingWorkspaceSettings()
            navController.navigate("settings")
            navController.navigate("workspace-settings") { launchSingleTop = true }
        }
    }

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
            HomeScreen(
                onOpenProject = { _, projectId -> navController.navigate("project/$projectId") },
                onOpenSettings = { navController.navigate("settings") },
                onOpenInbox = { navController.navigate("inbox") },
            )
        }
        composable("inbox") {
            InboxScreen(
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
            )
        }
        composable("settings") {
            SettingsScreen(
                onOpenIntegrations = { navController.navigate("integrations") },
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
        composable("integrations") {
            IntegrationsScreen(onBack = { navController.popBackStack() })
        }
        composable("share-pick") {
            ShareTargetPickerScreen(
                onPicked = { projectId ->
                    // Land on the project list, then push the prefilled create
                    // screen on top so backing out returns to the list.
                    navController.navigate("project/$projectId") {
                        popUpTo("share-pick") { inclusive = true }
                    }
                    navController.navigate("project/$projectId/new")
                },
                onCancel = {
                    // Drop the pending share so it doesn't prefill the next project opened.
                    workspaceSelection.consumePendingShare()
                    navController.popBackStack()
                },
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
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
                onCreateIssue = { navController.navigate("project/$projectId/new") },
            )
        }
        composable("project/{projectId}/new") {
            val pendingShare by workspaceSelection.pendingShare.collectAsState()
            val sharePrefill = remember(pendingShare) { pendingShare?.let { buildSharePrefill(it) } }
            CreateIssueScreen(
                onBack = { navController.popBackStack() },
                sharePrefill = sharePrefill,
                onSharePrefillConsumed = { workspaceSelection.consumePendingShare() },
            )
        }
        composable("issue/{issueId}") { entry ->
            val issueId = entry.arguments?.getString("issueId").orEmpty()
            IssueDetailScreen(issueId = issueId, onBack = { navController.popBackStack() })
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
