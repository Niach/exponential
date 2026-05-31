package com.exponential.app.navigation

import androidx.compose.animation.AnimatedContentTransitionScope.SlideDirection
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
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
import com.exponential.app.ui.admin.AdminUsersScreen
import com.exponential.app.ui.admin.AdminWorkspacesScreen
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.home.HomeScreen
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.integrations.IntegrationsScreen
import com.exponential.app.ui.invite.InviteAcceptScreen
import com.exponential.app.ui.issue.IssueDetailScreen
import com.exponential.app.ui.issue.IssueListScreen
import com.exponential.app.ui.settings.ServerDetailScreen
import com.exponential.app.ui.settings.SettingsScreen
import com.exponential.app.ui.settings.SyncDiagnosticsScreen
import com.exponential.app.ui.settings.WorkspaceSettingsScreen
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
        if (state.token == null) return@LaunchedEffect
        when (target) {
            is DeepLinkBus.Target.Issue ->
                navController.navigate("issue/${target.id}") { launchSingleTop = true }
            is DeepLinkBus.Target.Invite ->
                navController.navigate("invite/${target.token}") { launchSingleTop = true }
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

    AppBackground {
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
        startDestination = "home",
        // iOS-style horizontal push/pop transitions.
        enterTransition = { slideIntoContainer(SlideDirection.Start, tween(280)) },
        exitTransition = { slideOutOfContainer(SlideDirection.Start, tween(280)) },
        popEnterTransition = { slideIntoContainer(SlideDirection.End, tween(280)) },
        popExitTransition = { slideOutOfContainer(SlideDirection.End, tween(280)) },
    ) {
        composable("home") {
            HomeScreen(
                onOpenProject = { _, projectId -> navController.navigate("project/$projectId") },
                onOpenSettings = { navController.navigate("settings") },
            )
        }
        composable("settings") {
            SettingsScreen(
                onOpenIntegrations = { navController.navigate("integrations") },
                onOpenServerDetail = { accountId -> navController.navigate("server/$accountId") },
                onOpenWorkspaceSettings = { navController.navigate("workspace-settings") },
                onOpenAdminUsers = { navController.navigate("admin-users") },
                onOpenAdminWorkspaces = { navController.navigate("admin-workspaces") },
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
        composable("admin-users") {
            AdminUsersScreen(onBack = { navController.popBackStack() })
        }
        composable("admin-workspaces") {
            AdminWorkspacesScreen(onBack = { navController.popBackStack() })
        }
        composable("integrations") {
            IntegrationsScreen(onBack = { navController.popBackStack() })
        }
        composable("project/{projectId}") { entry ->
            val projectId = entry.arguments?.getString("projectId").orEmpty()
            IssueListScreen(
                projectId = projectId,
                onOpenIssue = { id -> navController.navigate("issue/$id") },
                onBack = { navController.popBackStack() },
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
