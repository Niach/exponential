package com.exponential.app.ui.settings

import com.exponential.app.ui.components.EmptyState
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.SettingsNav
import com.exponential.app.domain.SettingsNavGroups
import androidx.compose.foundation.background
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
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.AppConstants
import com.exponential.app.PlayStore
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.applyTeamSwitch
import com.exponential.app.data.planTeamSwitch
import kotlinx.coroutines.launch
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.MultiAccountTeamRepository
import com.exponential.app.data.db.ServerTeamGroup
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.gettingstarted.GettingStartedSheet
import com.exponential.app.ui.gettingstarted.GettingStartedViewModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.stateIn

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val selection: TeamSelection,
    multiAccountTeams: MultiAccountTeamRepository,
) : ViewModel() {
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts
    val serverGroups: StateFlow<List<ServerTeamGroup>> =
        multiAccountTeams.serverGroups.stateIn(
            viewModelScope,
            SharingStarted.WhileSubscribed(5_000),
            emptyList(),
        )

    // startAddServer removed — Settings now navigates to the instance route
    // directly via the onAddServer callback.

    /// Settings → Teams tap. Selects the team and (for cross-server
    /// taps) makes its account active; the caller navigates immediately —
    /// TeamSettingsViewModel scopes to the active account reactively, so
    /// no rebuild/pending-handoff dance is needed.
    fun onTeamSettingsTap(accountId: String, teamId: String) {
        selection.select(teamId)
        if (accountId != auth.activeAccountId.value) {
            auth.switchAccount(accountId)
        }
    }
}

// iOS-parity Settings: a centered nav title, the zinc AppBackground gradient,
// and grouped glass sections (Servers / Teams / General) of
// leading-icon + title (+ optional subtitle) + trailing-chevron rows.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    onOpenServerDetail: (accountId: String) -> Unit,
    onOpenTeamSettings: (section: String) -> Unit,
    onOpenSyncDiagnostics: () -> Unit,
    onOpenAbout: () -> Unit,
    onAddServer: () -> Unit,
    onBack: () -> Unit,
    /** EXP-1270: Getting started's "Start a run" — the Agent home. */
    onOpenAgentHome: () -> Unit = {},
    viewModel: SettingsViewModel = hiltViewModel(),
    gettingStartedViewModel: GettingStartedViewModel = hiltViewModel(),
) {
    val accounts by viewModel.accounts.collectAsStateWithLifecycle()
    // EXP-1270: the phones' Getting started ENTRY (web + IDE: the sidebar
    // entry) — the same block in a bottom sheet, gone once complete.
    val gettingStarted by gettingStartedViewModel.state.collectAsStateWithLifecycle()
    var gettingStartedOpen by remember { mutableStateOf(false) }
    val serverGroups by viewModel.serverGroups.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val toaster = LocalToaster.current

    AppBackground {
        Scaffold(
            topBar = {
                CenterAlignedTopAppBar(
                    title = { TopBarTitle("Settings") },
                    navigationIcon = {
                        TopBarBackButton(onClick = onBack)
                    },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                        containerColor = Color.Transparent,
                    ),
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
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                // Servers section — one row per server account, then "Add server".
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader("Servers")
                    Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                        // EXP-1320: an entity list = flat gapless rows, no dividers.
                        accounts.forEach { account ->
                            ServerRow(
                                account = account,
                                onClick = { onOpenServerDetail(account.id) },
                            )
                        }
                        SettingsRow(
                            icon = ExpIcons.uiAdd,
                            title = "Add server",
                            showChevron = false,
                            onClick = onAddServer,
                        )
                    }
                }

                // Teams section — the switcher list: tapping a team
                // selects it (and switches the active account for cross-server
                // taps) before opening that team's settings.
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader("Teams")
                    if (serverGroups.isEmpty()) {
                        EmptyState(EmptyStateCopy.teams)
                    } else {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            serverGroups.forEach { group ->
                                TeamGroupBlock(
                                    group = group,
                                    showHeader = serverGroups.size > 1,
                                    onTeamTap = { teamId ->
                                        viewModel.onTeamSettingsTap(group.accountId, teamId)
                                        onOpenTeamSettings(SettingsNav.GENERAL)
                                    },
                                )
                            }
                        }
                    }
                }

                // General section.
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
                            SettingsRow(
                                icon = ExpIcons.settingsSync,
                                title = "Sync diagnostics",
                                onClick = onOpenSyncDiagnostics,
                            )
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
                                if (!PlayStore.openListing(context)) {
                                    toaster.error("Couldn't open Google Play")
                                }
                            },
                        )
                        // EXP-262: version + third-party licence notices.
                        SettingsRow(
                            icon = ExpIcons.settingsAbout,
                            title = "About",
                            onClick = onOpenAbout,
                        )
                    }
                }
            }
        }
    }
    if (gettingStartedOpen) {
        GettingStartedSheet(
            onOpenAgent = onOpenAgentHome,
            onDismiss = { gettingStartedOpen = false },
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
    /** Thread 58cb32df: the Team tab's counts (Members 8 · Boards 3). */
    trailingCount: Int? = null,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** S-3: a drawn lead instead of [icon] (the Assistant's brand mark). */
    leading: (@Composable () -> Unit)? = null,
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

// One server's hostname header + a glass card of its teams (the switcher).
// The header only disambiguates when several servers are connected — with a
// single server it's noise (iOS parity), so the caller hides it.
@Composable
private fun TeamGroupBlock(
    group: ServerTeamGroup,
    showHeader: Boolean,
    onTeamTap: (teamId: String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (showHeader) {
            Column(modifier = Modifier.padding(horizontal = 4.dp)) {
                Text(
                    group.hostname,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (!group.userEmail.isNullOrBlank()) {
                    Text(
                        group.userEmail!!,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
            group.teams.forEach { team ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onTeamTap(team.id) }
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    TeamAvatar(team, size = 22.dp)
                    Spacer(Modifier.width(12.dp))
                    Text(
                        team.name,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.width(8.dp))
                    Icon(
                        ExpIcons.uiChevronRight,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                    )
                }
            }
        }
    }
}


// ── The Team tab (thread 58cb32df, chats.json `shell`) ──────────────────────

/** S-3: the `nav` groups the Team tab draws (Apps = its own tab; Personal =
 *  the servers + General blocks). */
private val TEAM_TAB_NAV_GROUPS = setOf("team", "boards", "agents")

/** What the Team tab reads about the ACTIVE team. */
data class TeamTabState(
    val team: com.exponential.app.data.db.TeamEntity? = null,
    val members: Int = 0,
    val boards: Int = 0,
    val agents: Int = 0,
    /** S-3: the viewer's own Assistant in the team (null = none yet). */
    val assistantId: String? = null,
    /** The viewer owns the active team (settings-nav.json `visible: owner`). */
    val manageTeam: Boolean = false,
    /** Teams (of the active account) with an unread notification: an amber dot. */
    val unreadTeamIds: Set<String> = emptySet(),
)

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
@HiltViewModel
class TeamTabViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val holder: com.exponential.app.data.db.DatabaseHolder,
    private val selection: TeamSelection,
) : ViewModel() {
    private val dbFlow = com.exponential.app.data.db.accountDatabaseFlow(auth, holder)

    val selectedId: StateFlow<String?> = selection.selectedId

    val state: StateFlow<TeamTabState> =
        kotlinx.coroutines.flow.combine(dbFlow, selection.selectedId, auth.userId) { db, team, me -> Triple(db, team, me) }
            .flatMapLatest { (db, teamId, me) ->
                if (db == null) {
                    kotlinx.coroutines.flow.flowOf(TeamTabState())
                } else {
                    val teamFlow = if (teamId == null) kotlinx.coroutines.flow.flowOf(null) else db.teamDao().observeById(teamId)
                    val members = if (teamId == null) kotlinx.coroutines.flow.flowOf(emptyList()) else db.teamMemberDao().observeByTeam(teamId)
                    val agents = if (teamId == null) kotlinx.coroutines.flow.flowOf(emptyList()) else db.agentDefinitionDao().observeByTeam(teamId)
                    val notifications = if (me == null) kotlinx.coroutines.flow.flowOf(emptyList()) else db.notificationDao().observeByUser(me)
                    kotlinx.coroutines.flow.combine(
                        teamFlow,
                        members,
                        db.boardDao().observeAll(),
                        agents,
                        kotlinx.coroutines.flow.combine(notifications, db.issueDao().observeAll()) { n, i -> n to i },
                    ) { team, memberRows, boards, agentRows, (notes, issues) ->
                        val boardTeam = boards.associate { it.id to it.teamId }
                        val issueTeam = issues.associate { it.id to boardTeam[it.boardId] }
                        TeamTabState(
                            team = team,
                            members = memberRows.size,
                            boards = boards.count { it.teamId == teamId },
                            // S-3: other members' Assistants are never counted.
                            agents = agentRows.count {
                                com.exponential.app.domain.AgentSettings.Assistant.visibleAgent(it.role, it.ownerId, me)
                            },
                            assistantId = agentRows.firstOrNull { it.isAssistant && it.ownerId == me }?.id,
                            manageTeam = me != null && memberRows.any {
                                it.userId == me && it.role == com.exponential.app.domain.DomainContract.teamRoleOwner
                            },
                            unreadTeamIds = notes.filter { it.readAt == null }
                                .mapNotNull { it.teamId ?: it.issueId?.let(issueTeam::get) }
                                .toSet(),
                        )
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamTabState())

    /** The ONE place a phone switches its team (chats.json): select it, move
     *  the Board tab onto the team's board and, for another server's team,
     *  make that account active landing on that team (iOS `switchTeam`). */
    fun switchTeam(accountId: String, teamId: String) {
        viewModelScope.launch {
            selection.applyTeamSwitch(selection.planTeamSwitch(holder, accountId, teamId), auth)
        }
    }
}

/**
 * Thread 58cb32df: the TEAM tab (chats.json `shell`): the active team's avatar
 * (84) + name + `copy.team.meta`, a glass group of the viewer's teams (the
 * active one checked, a team with an unread notification an amber dot; a tap
 * SWITCHES — the only place a phone switches its team), then the team's
 * settings rows (#EXP-1381: settings-nav.json `phoneRows`, each opening its
 * OWN page `team-settings/<id>`), then the old settings root: the servers
 * (the account rows) and General.
 */
@Composable
fun TeamTabScreen(
    onOpenServerDetail: (accountId: String) -> Unit,
    /** A [com.exponential.app.domain.SettingsNav] row id. */
    onOpenTeamSettings: (section: String) -> Unit,
    onOpenSyncDiagnostics: () -> Unit,
    onOpenAbout: () -> Unit,
    onAddServer: () -> Unit,
    onOpenAgentHome: () -> Unit = {},
    /** S-3: the Agents group's Assistant row opens its page. */
    onOpenAgentSettings: (agentId: String) -> Unit = {},
    viewModel: SettingsViewModel = hiltViewModel(),
    teamViewModel: TeamTabViewModel = hiltViewModel(),
    gettingStartedViewModel: GettingStartedViewModel = hiltViewModel(),
) {
    val accounts by viewModel.accounts.collectAsStateWithLifecycle()
    val serverGroups by viewModel.serverGroups.collectAsStateWithLifecycle()
    val state by teamViewModel.state.collectAsStateWithLifecycle()
    val selectedId by teamViewModel.selectedId.collectAsStateWithLifecycle()
    val gettingStarted by gettingStartedViewModel.state.collectAsStateWithLifecycle()
    var gettingStartedOpen by remember { mutableStateOf(false) }
    // The board switcher's old "New team" lives with the teams now.
    var teamSetupOpen by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val toaster = LocalToaster.current
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(bottom = com.exponential.app.ui.components.BottomBarInset)
                .testTag("team-tab"),
        ) {
            // #EXP-1384 (chats.json `shell.header`): the page header, the
            // team's name its title.
            com.exponential.app.ui.components.PageHeader(
                title = state.team?.name?.takeIf { it.isNotBlank() }
                    ?: com.exponential.app.domain.Chats.Copy.TABS.getValue("team"),
            )
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 16.dp, end = 16.dp, top = 8.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                TeamAvatar(state.team, size = 84.dp)
                Spacer(Modifier.size(12.dp))
                Text(
                    com.exponential.app.domain.Chats.Copy.teamMeta(state.members, state.boards),
                    style = MaterialTheme.typography.bodyMedium,
                    color = muted,
                )
            }

            // The viewer's teams: a tap switches (the active one checked).
            Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp).testTag("team-switch")) {
                serverGroups.forEach { group ->
                    group.teams.forEach { team ->
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { teamViewModel.switchTeam(group.accountId, team.id) }
                                .padding(horizontal = 16.dp, vertical = 12.dp)
                                .testTag("team-switch-row"),
                        ) {
                            TeamAvatar(team, size = 24.dp)
                            Spacer(Modifier.width(12.dp))
                            Text(
                                team.name,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface,
                                modifier = Modifier.weight(1f),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            when {
                                team.id == selectedId -> Icon(
                                    ExpIcons.uiCheck,
                                    contentDescription = "Selected",
                                    modifier = Modifier.size(18.dp),
                                    tint = MaterialTheme.colorScheme.onSurface,
                                )
                                team.id in state.unreadTeamIds -> Box(
                                    Modifier
                                        .size(8.dp)
                                        .background(com.exponential.app.ui.issue.NeedsInputAmber, androidx.compose.foundation.shape.CircleShape),
                                )
                            }
                        }
                    }
                }
                SettingsRow(
                    icon = ExpIcons.uiAdd,
                    title = "New team",
                    showChevron = false,
                    onClick = { teamSetupOpen = true },
                    modifier = Modifier.testTag("team-new"),
                )
            }

            // S-3: the team's settings rows = settings-nav.json `nav` for
            // android (`SettingsNavGroups`): Team, Boards, Agents — each item
            // with a phone page (`phoneRows`), `issues` flattened into its
            // children, the injected lists as their pages, the Devices row
            // closing the Team group; Personal = the servers + General below.
            val navGroups = SettingsNavGroups.groups(isOwner = state.manageTeam)
            navGroups.filterKeys { it in TEAM_TAB_NAV_GROUPS }.forEach { (groupId, items) ->
                val rowIds = buildList {
                    items.forEach { item ->
                        when (item) {
                            "issues" -> addAll(SettingsNavGroups.CHILDREN[item].orEmpty())
                            SettingsNavGroups.BOARDS -> add(SettingsNav.BOARDS)
                            SettingsNavGroups.AGENTS -> add(SettingsNav.AGENTS)
                            SettingsNavGroups.ASSISTANT -> if (state.assistantId != null) add(item)
                            else -> add(item)
                        }
                    }
                    if (groupId == "team") add(SettingsNav.DEVICES)
                }.filter { it == SettingsNavGroups.ASSISTANT || it in SettingsNav.visiblePhoneRows(isOwner = state.manageTeam) }
                if (rowIds.isEmpty()) return@forEach
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader(SettingsNavGroups.groupLabel(groupId))
                    Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                        rowIds.forEach { id ->
                            if (id == SettingsNavGroups.ASSISTANT) {
                                SettingsRow(
                                    icon = ExpIcons.settingsAgents,
                                    title = com.exponential.app.domain.AgentSettings.Assistant.TITLE,
                                    leading = {
                                        com.exponential.app.ui.components.ExponentialMark(
                                            size = 18.dp,
                                            tint = MaterialTheme.colorScheme.onSurface,
                                        )
                                    },
                                    onClick = { state.assistantId?.let(onOpenAgentSettings) },
                                    modifier = Modifier.testTag("settings-assistant-row"),
                                )
                            } else {
                                val row = SettingsNav.row(id)
                                SettingsRow(
                                    icon = ExpIcons.byConcept(row.icon) ?: ExpIcons.uiChevronRight,
                                    title = row.label,
                                    trailingCount = when (row.count) {
                                        "members" -> state.members
                                        "boards" -> state.boards
                                        "agents" -> state.agents
                                        else -> null
                                    },
                                    onClick = { onOpenTeamSettings(row.id) },
                                    modifier = Modifier.testTag(SettingsNav.ROW_TEST_ID),
                                )
                            }
                        }
                    }
                }
            }

            // The old settings root: the servers (the account rows) + General.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SectionHeader("Servers")
                Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                    accounts.forEach { account ->
                        ServerRow(account = account, onClick = { onOpenServerDetail(account.id) })
                    }
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
                    if (AppConstants.IS_STAGING) {
                        SettingsRow(icon = ExpIcons.settingsSync, title = "Sync diagnostics", onClick = onOpenSyncDiagnostics)
                    }
                    SettingsRow(
                        icon = ExpIcons.settingsRate,
                        title = "Rate our app",
                        trailingIcon = ExpIcons.uiExternalLink,
                        onClick = {
                            if (!PlayStore.openListing(context)) toaster.error("Couldn't open Google Play")
                        },
                    )
                    SettingsRow(icon = ExpIcons.settingsAbout, title = "About", onClick = onOpenAbout)
                }
            }
        }
        }
    }
    if (gettingStartedOpen) {
        GettingStartedSheet(onOpenAgent = onOpenAgentHome, onDismiss = { gettingStartedOpen = false })
    }
    if (teamSetupOpen) {
        com.exponential.app.ui.onboarding.TeamSetupSheet(
            onDismiss = { teamSetupOpen = false },
            onCreated = { teamSetupOpen = false },
            onJoined = { teamSetupOpen = false },
        )
    }
}
