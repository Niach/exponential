package com.exponential.app.ui.nav

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.PermanentDrawerSheet
import androidx.compose.material3.PermanentNavigationDrawer
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp
import androidx.navigation.NavDestination.Companion.hierarchy
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import androidx.navigation.compose.currentBackStackEntryAsState
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceEntity
import kotlinx.coroutines.launch

// CompositionLocal so any screen inside MainScaffold can open the drawer
// without re-plumbing the callback through every level of nesting. On
// Expanded widths the drawer is always visible, so opener is a no-op.
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

// Material 3 width-size-class threshold for "expanded" — the canonical
// breakpoint at which a permanent side drawer becomes preferable to a
// modal one. Below this, we use the modal drawer + bottom NavigationBar.
private const val EXPANDED_WIDTH_DP = 840

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
    onOpenSettings: () -> Unit,
    onSignOut: () -> Unit,
    content: @Composable () -> Unit,
) {
    val isExpanded = LocalConfiguration.current.screenWidthDp >= EXPANDED_WIDTH_DP

    // Build the avatar-menu bag once so both shells provide identical state to
    // screens inside the NavHost via LocalAvatarMenu.
    val avatarState = remember(workspaces, selectedWorkspace, email) {
        AvatarMenuState(
            email = email,
            workspaces = workspaces,
            selectedWorkspace = selectedWorkspace,
            onSelectWorkspace = onSelectWorkspace,
            onOpenSettings = onOpenSettings,
            onSignOut = onSignOut,
        )
    }

    if (isExpanded) {
        ExpandedShell(
            navController = navController,
            workspaces = workspaces,
            selectedWorkspace = selectedWorkspace,
            projects = projects,
            email = email,
            activeProjectId = activeProjectId,
            onSelectWorkspace = onSelectWorkspace,
            onOpenProject = onOpenProject,
            onOpenIntegrations = onOpenIntegrations,
            onSignOut = onSignOut,
            avatarState = avatarState,
            content = content,
        )
    } else {
        CompactShell(
            navController = navController,
            workspaces = workspaces,
            selectedWorkspace = selectedWorkspace,
            projects = projects,
            email = email,
            activeProjectId = activeProjectId,
            onSelectWorkspace = onSelectWorkspace,
            onOpenProject = onOpenProject,
            onOpenIntegrations = onOpenIntegrations,
            onSignOut = onSignOut,
            avatarState = avatarState,
            content = content,
        )
    }
}

@Composable
private fun CompactShell(
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
    avatarState: AvatarMenuState,
    content: @Composable () -> Unit,
) {
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()

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
        // On compact width the bottom NavigationBar is gone — the only two
        // tabs (Projects, Settings) became a stack-of-screens accessed from
        // the avatar menu in the TopAppBar. Drawer remains as an opt-in
        // workspace/project switcher reached via the hamburger.
        CompositionLocalProvider(
            LocalDrawerOpener provides { scope.launch { drawerState.open() } },
            LocalAvatarMenu provides avatarState,
        ) {
            Box(Modifier.fillMaxSize()) { content() }
        }
    }
}

@Composable
private fun ExpandedShell(
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
    avatarState: AvatarMenuState,
    content: @Composable () -> Unit,
) {
    val backStack by navController.currentBackStackEntryAsState()

    PermanentNavigationDrawer(
        drawerContent = {
            PermanentDrawerSheet(modifier = Modifier.width(320.dp)) {
                AppDrawer(
                    workspaces = workspaces,
                    selectedWorkspace = selectedWorkspace,
                    projects = projects,
                    email = email,
                    activeProjectId = activeProjectId,
                    onSelectWorkspace = onSelectWorkspace,
                    onOpenProject = onOpenProject,
                    onOpenIntegrations = onOpenIntegrations,
                    onSignOut = onSignOut,
                )
            }
        },
    ) {
        // Drawer is always visible at expanded width; opener is a no-op so
        // the menu icon button doesn't try to slide an already-open drawer.
        // Still provide LocalAvatarMenu so tablets see the same avatar menu
        // in the TopAppBar as phones do.
        CompositionLocalProvider(
            LocalDrawerOpener provides {},
            LocalAvatarMenu provides avatarState,
        ) {
            Row(Modifier.fillMaxSize()) {
                NavigationRail {
                    PrimaryDestination.all.forEach { destination ->
                        val selected = backStack?.destination?.hierarchy
                            ?.any { it.route == destination.route } == true
                        NavigationRailItem(
                            selected = selected,
                            onClick = { navigateToPrimary(navController, destination) },
                            icon = { Icon(destination.icon, contentDescription = null) },
                            label = { Text(destination.label) },
                        )
                    }
                }
                Box(Modifier.fillMaxSize().weight(1f)) { content() }
            }
        }
    }
}

private fun navigateToPrimary(
    navController: NavHostController,
    destination: PrimaryDestination,
) {
    navController.navigate(destination.route) {
        popUpTo(navController.graph.findStartDestination().id) { saveState = true }
        launchSingleTop = true
        restoreState = true
    }
}
