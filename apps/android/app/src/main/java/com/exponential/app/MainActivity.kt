package com.exponential.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.lifecycleScope
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.home.HomeScreen
import com.exponential.app.ui.home.HomeViewModel
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.admin.AdminUsersScreen
import com.exponential.app.ui.admin.AdminWorkspacesScreen
import com.exponential.app.ui.integrations.IntegrationsScreen
import com.exponential.app.ui.invite.InviteAcceptScreen
import com.exponential.app.ui.issue.IssueDetailScreen
import com.exponential.app.ui.issue.IssueListScreen
import com.exponential.app.ui.nav.MainScaffold
import com.exponential.app.ui.settings.ServerDetailScreen
import com.exponential.app.ui.settings.SettingsScreen
import com.exponential.app.ui.settings.WorkspaceSettingsScreen
import com.exponential.app.ui.theme.ExponentialTheme
import dagger.hilt.android.AndroidEntryPoint
import javax.inject.Inject
import kotlinx.coroutines.launch

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    @Inject lateinit var authRepository: AuthRepository
    @Inject lateinit var authApi: AuthApi
    @Inject lateinit var deepLinkBus: DeepLinkBus

    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { /* ignored */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        handleIntent(intent)
        maybeRequestNotificationPermission()
        setContent {
            ExponentialTheme {
                Surface(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(Color(0xFF09090B))
                ) {
                    AppRoot()
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        val data = intent?.data ?: return
        if (data.scheme != "exp") return
        when (data.host) {
            "oauth-return" -> handleOauthReturn(data)
            "issue" -> data.pathSegments.firstOrNull()?.let { deepLinkBus.openIssue(it) }
            "invite" -> data.pathSegments.firstOrNull()?.let { deepLinkBus.openInvite(it) }
        }
    }

    private fun handleOauthReturn(data: android.net.Uri) {
        // Token is in the URL fragment so it never lands in server logs.
        //
        // Pull the *encoded* fragment and decode once with Uri.decode (URI-style,
        // `+` stays literal). data.fragment + URLDecoder.decode would form-decode
        // `+` → space and corrupt the base64 HMAC signature better-call appends
        // to session cookies (signed cookie value is `${id}.${btoa(HMAC)}` and
        // btoa emits `+` `/` `=`). A mangled signature fails HMAC verification
        // in better-auth's bearer plugin and every authed request 401s.
        val encodedFragment = data.encodedFragment ?: return
        val token = encodedFragment
            .split("&")
            .map { it.split("=", limit = 2) }
            .firstOrNull { it.firstOrNull() == "token" }
            ?.getOrNull(1)
            ?.let { android.net.Uri.decode(it) }
            ?: return
        authRepository.setToken(token, authRepository.userEmail.value)
        lifecycleScope.launch {
            val accountId = authRepository.activeAccountId.value ?: return@launch
            val session = authApi.fetchSession(accountId)
            if (session != null) {
                authRepository.setToken(token, session.email, session.userId, session.isAdmin)
            }
        }
    }

    private fun maybeRequestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}

@Composable
private fun AppRoot() {
    val viewModel: AppViewModel = hiltViewModel()
    val deepLinkBus: DeepLinkBus = (LocalContext.current.applicationContext as ExponentialApp)
        .let {
            dagger.hilt.android.EntryPointAccessors.fromApplication(
                it,
                DeepLinkEntryPoint::class.java,
            ).deepLinkBus()
        }
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
            is DeepLinkBus.Target.Issue -> {
                navController.navigate("issue/${target.id}") { launchSingleTop = true }
            }
            is DeepLinkBus.Target.Invite -> {
                navController.navigate("invite/${target.token}") { launchSingleTop = true }
            }
        }
        deepLinkBus.consume()
    }

    val cloudAlreadyAdded = state.accounts.any { it.instanceUrl == AppConstants.PUBLIC_CLOUD_URL }

    if (state.instanceUrl == null) {
        UnauthenticatedNav(
            navController = navController,
            startDestination = "instance",
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
            showCancel = state.isAddingServer,
            onCancel = { viewModel.cancelAddServer() },
            cloudAlreadyAdded = cloudAlreadyAdded,
        )
    } else if (state.token == null) {
        UnauthenticatedNav(
            navController = navController,
            startDestination = "login",
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
            showCancel = false,
            onCancel = null,
            cloudAlreadyAdded = cloudAlreadyAdded,
        )
    } else {
        // Keying off the active account id forces Compose to tear down the entire
        // authenticated UI (including all ViewModels and DAO Flow subscriptions)
        // when the user switches between accounts. The DB itself is swapped by
        // SyncManager before this id changes.
        key(state.activeAccountId) {
            AuthenticatedShell(
                navController = navController,
                onSignOut = {
                    viewModel.signOut()
                    navController.navigate("login") { popUpTo("home") { inclusive = true } }
                },
            )
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
    showCancel: Boolean,
    onCancel: (() -> Unit)?,
    cloudAlreadyAdded: Boolean,
) {
    NavHost(navController = navController, startDestination = startDestination) {
        composable("instance") {
            InstanceScreen(
                onContinue = onInstanceSet,
                showCancel = showCancel,
                onCancel = onCancel,
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
private fun AuthenticatedShell(
    navController: NavHostController,
    onSignOut: () -> Unit,
) {
    // Hoist HomeViewModel up here so MainScaffold's drawer shares state with
    // any screen inside the NavHost that also injects it via hiltViewModel().
    val homeViewModel: HomeViewModel = hiltViewModel()
    val homeState by homeViewModel.state.collectAsState()
    LaunchedEffect(Unit) { homeViewModel.bootstrap() }

    // Consume any cross-server project tap that arrived before the
    // `key(activeAccountId)` rebuild — pre-set by HomeViewModel.onProjectTap
    // when the user picked a project on a different server.
    val workspaceSelection: WorkspaceSelection = (LocalContext.current.applicationContext as ExponentialApp)
        .let {
            dagger.hilt.android.EntryPointAccessors.fromApplication(
                it,
                WorkspaceSelectionEntryPoint::class.java,
            ).workspaceSelection()
        }
    val pendingProjectId by workspaceSelection.pendingProjectId.collectAsState()
    LaunchedEffect(pendingProjectId) {
        pendingProjectId?.let { projectId ->
            workspaceSelection.consumePendingProject()
            navController.navigate("project/$projectId") { launchSingleTop = true }
        }
    }
    // Same handoff for Settings → Workspaces tap on a workspace on a
    // different server: pendingWorkspaceSettings is flipped true, then after
    // the activeAccountId rebuild we re-push the Settings → WorkspaceSettings
    // stack so the user lands inside the right workspace on the right server.
    val pendingWorkspaceSettings by workspaceSelection.pendingWorkspaceSettings.collectAsState()
    LaunchedEffect(pendingWorkspaceSettings) {
        if (pendingWorkspaceSettings) {
            workspaceSelection.consumePendingWorkspaceSettings()
            navController.navigate("settings")
            navController.navigate("workspace-settings") { launchSingleTop = true }
        }
    }

    MainScaffold(
        navController = navController,
        serverGroups = homeState.serverGroups,
        activeAccountId = homeState.activeAccountId,
        selectedWorkspace = homeState.selectedWorkspace,
        projects = homeState.projects,
        email = homeState.email,
        activeProjectId = null,
        onSelectWorkspace = homeViewModel::selectWorkspace,
        onOpenProject = { id -> navController.navigate("project/$id") },
        onOpenIntegrations = { navController.navigate("integrations") },
        onOpenSettings = { navController.navigate("settings") },
        onSignOut = onSignOut,
    ) {
        NavHost(navController = navController, startDestination = "home") {
            composable("home") {
                HomeScreen(
                    onOpenProject = { _, projectId ->
                        navController.navigate("project/$projectId")
                    },
                )
            }
            composable("settings") {
                SettingsScreen(
                    onOpenIntegrations = { navController.navigate("integrations") },
                    onOpenServerDetail = { accountId ->
                        navController.navigate("server/$accountId")
                    },
                    onOpenWorkspaceSettings = { navController.navigate("workspace-settings") },
                    onOpenAdminUsers = { navController.navigate("admin-users") },
                    onOpenAdminWorkspaces = { navController.navigate("admin-workspaces") },
                )
            }
            composable("server/{accountId}") { entry ->
                val accountId = entry.arguments?.getString("accountId").orEmpty()
                ServerDetailScreen(
                    accountId = accountId,
                    onBack = { navController.popBackStack() },
                )
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
                )
            }
            composable("issue/{issueId}") { entry ->
                val issueId = entry.arguments?.getString("issueId").orEmpty()
                IssueDetailScreen(
                    issueId = issueId,
                    onBack = { navController.popBackStack() },
                )
            }
            composable("invite/{token}") { entry ->
                val token = entry.arguments?.getString("token").orEmpty()
                InviteAcceptScreen(
                    token = token,
                    onBack = { navController.popBackStack() },
                    onAccepted = {
                        navController.navigate("home") {
                            popUpTo("home") { inclusive = true }
                        }
                    },
                )
            }
        }
    }
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
