package com.exponential.app.ui.nav

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.navigation.NavDestination.Companion.hierarchy
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import androidx.navigation.compose.currentBackStackEntryAsState
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceEntity
import kotlinx.coroutines.launch

// CompositionLocal so any screen inside MainScaffold can open the drawer
// without re-plumbing the callback through every level of nesting.
val LocalDrawerOpener = staticCompositionLocalOf<() -> Unit> {
    error("LocalDrawerOpener not provided — wrap content in MainScaffold")
}

sealed class PrimaryDestination(
    val route: String,
    val label: String,
    val icon: ImageVector,
) {
    object Projects : PrimaryDestination("home", "Projects", Icons.Filled.Folder)
    object Settings : PrimaryDestination("settings", "Settings", Icons.Filled.Settings)

    companion object {
        val all = listOf(Projects, Settings)
    }
}

@Composable
fun MainScaffold(
    navController: NavHostController,
    workspaces: List<WorkspaceEntity>,
    selectedWorkspace: WorkspaceEntity?,
    projects: List<ProjectEntity>,
    email: String?,
    activeProjectId: String?,
    onSelectWorkspace: (String) -> Unit,
    onOpenProject: (String) -> Unit,
    onOpenIntegrations: () -> Unit,
    onSignOut: () -> Unit,
    content: @Composable () -> Unit,
) {
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val backStack by navController.currentBackStackEntryAsState()
    val currentRoute = backStack?.destination?.route
    val showBottomBar = PrimaryDestination.all.any { it.route == currentRoute }

    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            AppDrawer(
                workspaces = workspaces,
                selectedWorkspace = selectedWorkspace,
                projects = projects,
                email = email,
                activeProjectId = activeProjectId,
                onSelectWorkspace = {
                    onSelectWorkspace(it)
                    scope.launch { drawerState.close() }
                },
                onOpenProject = {
                    scope.launch { drawerState.close() }
                    onOpenProject(it)
                },
                onOpenIntegrations = {
                    scope.launch { drawerState.close() }
                    onOpenIntegrations()
                },
                onSignOut = {
                    scope.launch { drawerState.close() }
                    onSignOut()
                },
            )
        },
    ) {
        CompositionLocalProvider(
            LocalDrawerOpener provides { scope.launch { drawerState.open() } }
        ) {
            Column(Modifier.fillMaxSize()) {
                Box(Modifier.weight(1f).fillMaxSize()) {
                    content()
                }
                if (showBottomBar) {
                    NavigationBar {
                        PrimaryDestination.all.forEach { destination ->
                            val selected = backStack?.destination?.hierarchy
                                ?.any { it.route == destination.route } == true
                            NavigationBarItem(
                                selected = selected,
                                onClick = {
                                    navController.navigate(destination.route) {
                                        popUpTo(navController.graph.findStartDestination().id) {
                                            saveState = true
                                        }
                                        launchSingleTop = true
                                        restoreState = true
                                    }
                                },
                                icon = { Icon(destination.icon, contentDescription = null) },
                                label = { Text(destination.label) },
                            )
                        }
                    }
                }
            }
        }
    }
}
