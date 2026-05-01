package com.exponential.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.exponential.app.ui.auth.LoginScreen
import com.exponential.app.ui.home.HomeScreen
import com.exponential.app.ui.instance.InstanceScreen
import com.exponential.app.ui.theme.ExponentialTheme
import dagger.hilt.android.AndroidEntryPoint

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
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
}

@androidx.compose.runtime.Composable
private fun AppRoot() {
    val viewModel: AppViewModel = hiltViewModel()
    val state by viewModel.state.collectAsState()
    val navController = rememberNavController()

    val startDestination = when {
        state.instanceUrl == null -> "instance"
        state.token == null -> "login"
        else -> "home"
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
            HomeScreen(onSignOut = {
                viewModel.signOut()
                navController.navigate("login") {
                    popUpTo("home") { inclusive = true }
                }
            })
        }
    }
}
