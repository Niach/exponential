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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.lifecycleScope
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.home.HomeScreen
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.integrations.IntegrationsScreen
import com.exponential.app.ui.issue.IssueListScreen
import com.exponential.app.ui.issue.IssueDetailScreen
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
        }
    }

    private fun handleOauthReturn(data: android.net.Uri) {
        // Token is in the URL fragment so it never lands in server logs.
        val fragment = data.fragment ?: return
        val token = fragment
            .split("&")
            .map { it.split("=", limit = 2) }
            .firstOrNull { it.firstOrNull() == "token" }
            ?.getOrNull(1)
            ?.let { java.net.URLDecoder.decode(it, "UTF-8") }
            ?: return
        authRepository.setToken(token, authRepository.userEmail.value)
        lifecycleScope.launch {
            val email = authApi.fetchSession()
            if (email != null) authRepository.setToken(token, email)
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

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
@androidx.compose.runtime.Composable
private fun AppRoot() {
    val viewModel: AppViewModel = hiltViewModel()
    val deepLinkBus: DeepLinkBus = (androidx.compose.ui.platform.LocalContext.current
        .applicationContext as ExponentialApp)
        .let {
            // Reach into the Hilt singleton via an entry point.
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
                navController.navigate("issue/${target.id}") {
                    launchSingleTop = true
                }
            }
        }
        deepLinkBus.consume()
    }

    NavHost(navController = navController, startDestination = startDestination) {
        composable("instance") {
            InstanceScreen(onContinue = { url ->
                viewModel.setInstanceUrl(url)
                navController.navigate("login") {
                    popUpTo("instance") { inclusive = true }
                }
            })
        }
        composable("login") {
            LoginScreen(
                instanceUrl = state.instanceUrl ?: "",
                onLoggedIn = {
                    navController.navigate("home") {
                        popUpTo("login") { inclusive = true }
                    }
                },
                onChangeInstance = {
                    viewModel.clearInstance()
                    navController.navigate("instance") {
                        popUpTo("login") { inclusive = true }
                    }
                },
            )
        }
        composable("home") {
            HomeScreen(
                onOpenProject = { projectId ->
                    navController.navigate("project/$projectId")
                },
                onOpenIntegrations = { navController.navigate("integrations") },
                onSignOut = {
                    viewModel.signOut()
                    navController.navigate("login") {
                        popUpTo("home") { inclusive = true }
                    }
                },
            )
        }
        composable("integrations") {
            IntegrationsScreen(onBack = { navController.popBackStack() })
        }
        composable("project/{projectId}") { entry ->
            val projectId = entry.arguments?.getString("projectId").orEmpty()
            IssueListScreen(
                projectId = projectId,
                onBack = { navController.popBackStack() },
                onOpenIssue = { issueId -> navController.navigate("issue/$issueId") },
            )
        }
        composable("issue/{issueId}") { entry ->
            val issueId = entry.arguments?.getString("issueId").orEmpty()
            IssueDetailScreen(
                issueId = issueId,
                onBack = { navController.popBackStack() },
            )
        }
    }
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface DeepLinkEntryPoint {
    fun deepLinkBus(): DeepLinkBus
}
