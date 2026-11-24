package com.exponential.app.ui.settings

import com.exponential.app.domain.SettingsNav
import com.exponential.app.domain.SettingsNavGroups
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.AppConstants
import com.exponential.app.PlayStore
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.data.chatstore.ChatStoreSettings
import com.exponential.app.data.chatstore.KeepToolSteps
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.gettingstarted.GettingStartedSheet
import com.exponential.app.ui.gettingstarted.GettingStartedViewModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: com.exponential.app.data.db.DatabaseHolder,
    private val chatStoreSettings: ChatStoreSettings,
) : ViewModel() {
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts
    val activeAccountId: StateFlow<String?> = auth.activeAccountId

    /** The viewer's synced row (the card's picture). */
    @OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
    val viewer: StateFlow<com.exponential.app.data.db.UserEntity?> =
        kotlinx.coroutines.flow.combine(com.exponential.app.data.db.accountDatabaseFlow(auth, holder), auth.userId) { db, me -> db to me }
            .flatMapLatest { (db, me) -> if (db == null || me == null) kotlinx.coroutines.flow.flowOf(null) else db.userDao().observeById(me) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** #EDE-114 (chat-store.json THE STORAGE SETTING): this device's `Keep tool steps` (null = Forever). */
    val keepToolStepsDays: StateFlow<Int?> = chatStoreSettings.days

    fun setKeepToolSteps(days: Int?) = chatStoreSettings.setDays(days)
}

/**
 * #EXP-2109 (settings-nav.json `phone.settings`): THE SETTINGS TAB, only what is
 * the viewer's: their card (opens Account), then account, Assistant,
 * notifications, security · devices · about, then the servers and General (Getting started in it).
 */
@Composable
fun SettingsScreen(
    onOpenAccount: (accountId: String) -> Unit,
    onOpenAssistant: () -> Unit,
    /** #EXP-1815: the source switches. */
    onOpenNotifications: () -> Unit,
    onOpenSecurity: () -> Unit,
    onOpenDevices: () -> Unit,
    onOpenAbout: () -> Unit,
    onOpenSyncDiagnostics: () -> Unit,
    onAddServer: () -> Unit,
    /** EXP-1270: Getting started's "Start a run": the composer. */
    onOpenComposer: () -> Unit,
    viewModel: SettingsViewModel = hiltViewModel(),
    gettingStartedViewModel: GettingStartedViewModel = hiltViewModel(),
) {
    val accounts by viewModel.accounts.collectAsStateWithLifecycle()
    val activeId by viewModel.activeAccountId.collectAsStateWithLifecycle()
    val viewer by viewModel.viewer.collectAsStateWithLifecycle()
    // EXP-1270: the phones' Getting started ENTRY (web + IDE: the sidebar
    // entry) — the same block in a bottom sheet, gone once complete.
    val gettingStarted by gettingStartedViewModel.state.collectAsStateWithLifecycle()
    var gettingStartedOpen by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val toaster = LocalToaster.current
    val account = accounts.firstOrNull { it.id == activeId }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(bottom = com.exponential.app.ui.components.BottomBarInset)
                .testTag("settings-tab"),
        ) {
            com.exponential.app.ui.components.PageHeader(
                title = com.exponential.app.domain.Chats.Copy.HEADERS.getValue("settings"),
            )
            Column(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                if (account != null) {
                    ViewerCard(account, viewer, onClick = { onOpenAccount(account.id) })
                }
                SettingsNavGroups.phoneSettingsGroups().forEach { group ->
                    Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                        group.forEach { id ->
                            SettingsPageRow(id) {
                                when (id) {
                                    "account" -> account?.let { onOpenAccount(it.id) }
                                    SettingsNavGroups.ASSISTANT -> onOpenAssistant()
                                    "notifications" -> onOpenNotifications()
                                    "security" -> onOpenSecurity()
                                    SettingsNav.DEVICES -> onOpenDevices()
                                    "about" -> onOpenAbout()
                                }
                            }
                        }
                    }
                }
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader("Servers")
                    Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                        // EXP-1320: an entity list = flat gapless rows, no dividers.
                        accounts.forEach { row -> ServerRow(account = row, onClick = { onOpenAccount(row.id) }) }
                        SettingsRow(icon = ExpIcons.uiAdd, title = "Add server", showChevron = false, onClick = onAddServer)
                    }
                }
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader("General")
                    Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                        if (gettingStarted.visible) {
                            Box(Modifier.testTag(GettingStartedCopy.ENTRY_TEST_ID)) {
                                SettingsRow(
                                    icon = ExpIcons.navGettingStarted,
                                    title = GettingStartedCopy.TITLE,
                                    onClick = { gettingStartedOpen = true },
                                )
                            }
                        }
                        // EXP-232: staging-only — exposes live shape status and
                        // a "Resync now" data-wipe hatch, not for store builds.
                        if (AppConstants.IS_STAGING) {
                            SettingsRow(icon = ExpIcons.settingsSync, title = "Sync diagnostics", onClick = onOpenSyncDiagnostics)
                        }
                        // EXP-260 — Android is the only client with a store
                        // listing so far, so this row is Android-only.
                        SettingsRow(
                            icon = ExpIcons.settingsRate,
                            title = "Rate our app",
                            trailingIcon = ExpIcons.uiExternalLink,
                            onClick = {
                                // Neither Play nor a browser: say so rather
                                // than leave the row looking inert.
                                if (!PlayStore.openListing(context)) toaster.error("Couldn't open Google Play")
                            },
                        )
                        KeepToolStepsRow(viewModel)
                    }
                }
            }
        }
    }
    if (gettingStartedOpen) {
        GettingStartedSheet(onOpenAgent = onOpenComposer, onDismiss = { gettingStartedOpen = false })
    }
}

/** The viewer's card: avatar, name, email; opens Account. */
@Composable
private fun ViewerCard(account: ServerAccount, user: com.exponential.app.data.db.UserEntity?, onClick: () -> Unit) {
    val name = account.userName?.takeIf { it.isNotBlank() } ?: user?.name?.takeIf { it.isNotBlank() } ?: account.userEmail.orEmpty()
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 4.dp, vertical = 8.dp)
            .testTag("settings-viewer"),
    ) {
        com.exponential.app.ui.components.UserAvatar(user = user, nameOrEmail = name, size = 56.dp, userId = account.userId)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                name,
                style = MaterialTheme.typography.titleMedium.copy(letterSpacing = 0.sp),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val email = account.userEmail
            if (!email.isNullOrBlank() && email != name) {
                Text(
                    email,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** One `phone.settings` page's row. */
@Composable
private fun SettingsPageRow(id: String, onClick: () -> Unit) {
    val page = SettingsNavGroups.page(id) ?: return
    SettingsRow(
        icon = ExpIcons.byConcept(page.icon) ?: ExpIcons.uiChevronRight,
        title = page.label,
        onClick = onClick,
        modifier = Modifier.testTag("settings-$id"),
        // #EXP-1987: the brand mark, never the page's concept glyph.
        leading = if (id == SettingsNavGroups.ASSISTANT) {
            { com.exponential.app.ui.components.ExponentialMark(size = 20.dp, tint = MaterialTheme.colorScheme.onSurface) }
        } else {
            null
        },
    )
}

/**
 * #EDE-114 (chat-store.json THE STORAGE SETTING): `Keep tool steps`, this
 * device's chat store window, THE picker row + `Picker` sheet the settings
 * pages already use (label, value + chevron; no subtitle).
 */
@Composable
private fun KeepToolStepsRow(viewModel: SettingsViewModel) {
    val days by viewModel.keepToolStepsDays.collectAsStateWithLifecycle()
    KeepToolStepsRow(days, viewModel::setKeepToolSteps)
}

@Composable
internal fun KeepToolStepsRow(days: Int?, onPick: (Int?) -> Unit) {
    val selected = KeepToolSteps.option(days)
    Picker(
        items = KeepToolSteps.OPTIONS.map { PickerItem(value = it.id, label = it.label) },
        mode = PickerMode.Single,
        value = setOf(selected.id),
        onChange = { picked -> picked.firstOrNull()?.let(KeepToolSteps::fromId)?.let { onPick(it.days) } },
        title = KeepToolSteps.LABEL,
    ) { open ->
        PickerValueRow(
            label = KeepToolSteps.LABEL,
            value = selected.label,
            onClick = open,
            modifier = Modifier.testTag(KeepToolSteps.TEST_ID),
        )
    }
}

// A navigational glass row: leading icon (22dp, secondary), title (+ optional
// subtitle), then a trailing chevron (quaternary) by default. iOS settingsRow.
@Composable
internal fun SettingsRow(
    icon: ImageVector,
    title: String,
    subtitle: String? = null,
    trailingIcon: ImageVector = ExpIcons.uiChevronRight,
    showChevron: Boolean = true,
    /** A team settings row's count (Members 8 · Boards 3). */
    trailingCount: Int? = null,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** S-3: a drawn lead instead of [icon] (the Assistant's brand mark). */
    leading: (@Composable () -> Unit)? = null,
    /** #EXP-2109: a drawn trailing before the chevron (a count badge). */
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        if (leading != null) {
            Box(Modifier.size(22.dp), contentAlignment = Alignment.Center) { leading() }
        } else {
            Icon(
                icon,
                contentDescription = null,
                modifier = Modifier.size(22.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (subtitle != null) {
                Text(
                    subtitle,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (trailingCount != null && trailingCount > 0) {
            Text(
                trailingCount.toString(),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        trailing?.invoke()
        if (showChevron) {
            Spacer(Modifier.width(8.dp))
            Icon(
                trailingIcon,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            )
        }
    }
}

@Composable
private fun ServerRow(account: ServerAccount, onClick: () -> Unit) {
    // EXP-311: the row is always titled by the server ("Exponential Cloud" /
    // hostname, iOS parity) with the signed-in email below — the account
    // identity (avatar, name, email) lives in the detail screen.
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Icon(
            ExpIcons.settingsServers,
            contentDescription = null,
            modifier = Modifier.size(22.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                account.displayName,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            when {
                account.token == null -> Text(
                    "Signed out",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.tertiary,
                )
                !account.userEmail.isNullOrBlank() -> Text(
                    account.userEmail!!,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                else -> Text(
                    "Signed in",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
        )
    }
}

// ── A team's settings page (#EXP-2109, settings-nav.json `phone.team`) ───────

/** What the team settings page reads about its team. */
data class TeamSettingsRootState(
    val members: Int = 0,
    val boards: Int = 0,
    val agents: Int = 0,
    /** The viewer owns the team (settings-nav.json `visible: owner`). */
    val owner: Boolean = false,
)

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
@HiltViewModel
class TeamSettingsRootViewModel @Inject constructor(
    savedStateHandle: androidx.lifecycle.SavedStateHandle,
    auth: AuthRepository,
    holder: com.exponential.app.data.db.DatabaseHolder,
) : ViewModel() {
    val teamId: String = savedStateHandle[com.exponential.app.data.ARG_TEAM] ?: ""

    val state: StateFlow<TeamSettingsRootState> =
        kotlinx.coroutines.flow.combine(com.exponential.app.data.db.accountDatabaseFlow(auth, holder), auth.userId) { db, me -> db to me }
            .flatMapLatest { (db, me) ->
                if (db == null) {
                    kotlinx.coroutines.flow.flowOf(TeamSettingsRootState())
                } else {
                    kotlinx.coroutines.flow.combine(
                        db.teamMemberDao().observeByTeam(teamId),
                        db.boardDao().observeByTeam(teamId),
                        db.agentDefinitionDao().observeByTeam(teamId),
                    ) { members, boards, agents ->
                        TeamSettingsRootState(
                            members = members.size,
                            boards = boards.size,
                            // #EXP-1987: the Agents page lists the team's agents only.
                            agents = agents.count { !it.isAssistant },
                            owner = me != null && members.any {
                                it.userId == me && it.role == com.exponential.app.domain.DomainContract.teamRoleOwner
                            },
                        )
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamSettingsRootState())
}

/** A team's SETTINGS page (its `…` menu): `phone.team` in order under the
 *  title Settings, each row its own page. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamSettingsRootScreen(
    onBack: () -> Unit,
    /** A [SettingsNav] row id. */
    onOpenSection: (teamId: String, section: String) -> Unit,
    viewModel: TeamSettingsRootViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { TopBarTitle(com.exponential.app.domain.SidebarTree.Copy.SETTINGS) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .testTag(com.exponential.app.ui.teams.TeamsTestIds.TEAM_SETTINGS),
        ) {
            Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                SettingsNavGroups.phoneTeamRows(isOwner = state.owner).forEach { row ->
                    SettingsRow(
                        icon = ExpIcons.byConcept(row.icon) ?: ExpIcons.uiChevronRight,
                        title = row.label,
                        trailingCount = when (row.count) {
                            "members" -> state.members
                            "boards" -> state.boards
                            "agents" -> state.agents
                            else -> null
                        },
                        onClick = { onOpenSection(viewModel.teamId, row.id) },
                        modifier = Modifier.testTag(SettingsNav.ROW_TEST_ID),
                    )
                }
            }
        }
    }
}

/** #EXP-2109 (settings-nav.json `security`): how the account signs in. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SecuritySettingsScreen(
    onBack: () -> Unit,
    settings: SettingsViewModel = hiltViewModel(),
    viewModel: ServerDetailViewModel = hiltViewModel(),
) {
    val accounts by settings.accounts.collectAsStateWithLifecycle()
    val activeId by settings.activeAccountId.collectAsStateWithLifecycle()
    val account = accounts.firstOrNull { it.id == activeId }
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { TopBarTitle(SettingsNavGroups.PAGES.getValue("security").label) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
        ) {
            if (account?.token != null) {
                LaunchedEffect(account.id, account.token) { viewModel.loadMethods(account.id) }
                SignInMethodsSection(accountId = account.id, viewModel = viewModel)
            }
        }
    }
}
