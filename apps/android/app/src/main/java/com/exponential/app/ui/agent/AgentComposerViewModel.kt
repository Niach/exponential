package com.exponential.app.ui.agent

import com.exponential.app.data.routeTeamIdFlow
import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.McpServersApi
import com.exponential.app.domain.McpServerOption
import com.exponential.app.domain.mcpServerReady
import com.exponential.app.domain.preselectMcpServerIds
import kotlinx.coroutines.flow.collectLatest
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AccountOption
import com.exponential.app.domain.AccountOptions
import com.exponential.app.domain.AgentComposerPrompt
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.domain.steerAttachmentRefusal
import com.exponential.app.domain.uploadedImageIds
import com.exponential.app.domain.uploadedSteerFiles
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineImage
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.agentSeed
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.components.defaultAgentFor
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.issueRefTarget
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerLaunchDelegate
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import com.exponential.app.domain.AgentHome
import com.exponential.app.domain.FeedbackStates

// EXP-825: the ONE launcher's state — the Agent page composer. It replaced the
// three-tab Start-coding sheet (Issues | Actions | Chat, EXP-257/EXP-615) and
// the dedicated create-action sheet: what a run is ABOUT is a SUBJECT picked
// into the composer (issue chips, or one action chip; picking the other kind
// SWAPS — no disabled controls), the free text is the chat prompt while there
// is no subject and "additional instructions" once there is one, images ride
// the text as the steer embed format, and the options row under the card
// carries device/agent/model/plan with the rest behind a `⋯` sheet. Every play
// button in the app navigates here with a seed (`AgentComposerSeed`, ×4)
// instead of opening a sheet of its own.
//
// What moved here from the sheet verbatim: the EXP-437 device-seeded options
// cluster (a settled machine seeds every option, a re-emit never stomps
// edits), the EXP-349 repo-input seed latch, the EXP-323 `pr` seed and the
// EXP-481 resume default. The lookup data (actions, repos, boards, PRs,
// worktrees) stays on [AgentLaunchDataViewModel]; the send + started-run
// watch is the shared [SteerLaunchDelegate].

/**
 * The composer field's placeholder — web `composerPlaceholder` byte for byte:
 * no subject asks for the message; any subject (issue chips or, NEW-2, a
 * picked prompt — prompts carry no composer hint any more) asks for what is
 * optional next to it.
 */
internal fun composerPlaceholder(
    subject: ComposerSubject?,
    /** EXP-1253: the Agent HOME asks composer-home.json's question instead. */
    home: Boolean = false,
): String {
    if (subject == null) return if (home) AgentHome.PLACEHOLDER else DomainContract.composerUiChatPlaceholder
    // S-3: an agent picked in the agent row keeps the home's question.
    if (home && subject is ComposerSubject.Agent) return AgentHome.PLACEHOLDER
    return DomainContract.composerUiInstructionsPlaceholder
}

/** EXP-1271: what the composer's starting row says — where, and since when. */
data class ComposerStarting(val deviceLabel: String, val sinceMs: Long)


/** What the composer is about — issue chips OR one prompt chip, never both
 *  (web `subject.kind` issues | prompt). */
sealed interface ComposerSubject {
    data class Issues(val ids: List<String>) : ComposerSubject

    /** NEW-2: a saved prompt (`sessions.send({promptId})`); no inputs. */
    data class Prompt(val id: String) : ComposerSubject

    /** NEW-3: a team agent (`sessions.send({agentId})`): the message goes
     *  into the caller's DM with it, on the agent's own device and login. */
    data class Agent(val id: String) : ComposerSubject
}

/**
 * The launch options as picked (EXP-437 seeding rules): the machine is a
 * PREFERENCE resolved against the startable pool (nothing set = the default
 * machine), [account] is the picked login's profile id (`system` = the
 * ambient one) or "" for none named, which runs on the machine's LAST USED
 * login.
 *
 * EXP-836: a play button's REQUEST and a person's PICK are separate fields, not
 * one `deviceId`. The request outranks the pick while its machine is startable
 * — including while the devices shape is still hydrating, so a late-arriving
 * row still wins over the default — and a pick CLEARS it, which is what makes a
 * request one-shot. Keeping them apart is also what lets the composer SAY that
 * a requested machine cannot take the run ([LaunchDeviceRules.requestNote])
 * instead of silently falling back to the default one.
 */
data class LaunchDraft(
    val requestedDeviceId: String? = null,
    val pickedDeviceId: String? = null,
    val agent: String = DEFAULT_AGENT,
    val model: String = "",
    /** EXP-981: claude only — the model its subagents run on. */
    val subagentModel: String = "",
    val effort: String = "",
    val ultracode: Boolean = false,
    val account: String = "",
) {
    /**
     * EXP-872: the picked login as an `AccountOption.key`, which is what the
     * ONE account picker selects on. `""` (nothing picked yet) reads as the
     * agent's unpinned option.
     */
    val accountKey: String get() = "$agent:$account"

    /**
     * The `account` a start sends: the picked id VERBATIM; null = none named
     * (nothing picked, or a legacy `system`), which runs on the machine's LAST
     * USED login.
     */
    val wireAccount: String? get() = AccountOptions.pinnedAccount(account)

    /** [account] set to [option]'s id verbatim; "" only for no option at all. */
    fun withAccount(option: AccountOption?): LaunchDraft = copy(account = option?.id.orEmpty())
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentComposerViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val steerLaunch: SteerLaunchDelegate,
    private val imagesApi: IssueImagesApi,
    private val issuesApi: IssuesApi,
    private val mcpServersApi: McpServersApi,
    private val agentsApi: com.exponential.app.data.api.AgentsApi,
) : ViewModel() {

    /** S-3: the viewer (the agent row's `me`). */
    val me: StateFlow<String?> get() = auth.userId

    /** S-3: the composer home calls `agents.ensureAssistant` on mount. */
    fun ensureAssistant() {
        val team = teamId.value
        viewModelScope.launch {
            com.exponential.app.data.api.AssistantEnsurer.ensure(agentsApi, auth.activeAccountId.value, team)
        }
    }


    /** The route's one-shot preselection — applied once in `init`. */
    val seed: AgentComposerSeed = AgentComposerSeed.fromArgs(savedStateHandle)

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /**
     * The team: the pools, the start's teamId, the upload scope. The
     * SELECTED team, unless the seed names a team-owned subject (EXP-1186: a
     * cross-team Actions/Reviews row starts in ITS team, [routeTeamIdFlow]).
     */
    val teamId: StateFlow<String?> =
        routeTeamIdFlow(dbFlow, savedStateHandle, selection.selectedId)
            .stateIn(viewModelScope, SharingStarted.Eagerly, selection.selectedId.value)

    // ── The shared launch rails ─────────────────────────────────────────────
    val steerEnabled: StateFlow<Boolean?> get() = steerLaunch.enabled
    val runState: StateFlow<ActionRunState> get() = steerLaunch.runState
    /**
     * The session the screen opens, consumed once: a resume's row, or a
     * composer start's row when the draft was unchanged since its send (a
     * draft changed after the timeout stays and nothing navigates).
     */
    private val _openSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _openSessionId
    val startCandidates: StateFlow<List<IssueOption>> get() = steerLaunch.startCandidates

    fun consumeStartedSession() {
        _openSessionId.value = null
    }

    /** Every ONLINE machine, runnable or not — the signed-out caption's source. */
    val onlineDevices: StateFlow<List<SteerDevice>?> get() = steerLaunch.devices

    // The delegate builds its flows in `attach`; it must run BEFORE any
    // property below captures `steerLaunch.devices` — Kotlin runs property
    // initializers and init blocks in declaration order, and the `init` at
    // the bottom of this class handed `candidateDevices` the delegate's
    // never-resolving placeholder (the composer sat on "CLI default" with a
    // disabled submit forever).
    init {
        steerLaunch.attach(viewModelScope, teamId)
    }

    /**
     * The machines a start can go to: ONLINE (the delegate's pool) with a
     * runnable agent (EXP-409) — every subject shares ONE pool since EXP-672.
     * null while presence is still resolving.
     */
    val candidateDevices: StateFlow<List<SteerDevice>?> = steerLaunch.devices
        .map { devices -> devices?.filter { it.hasRunnableAgent } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // ── Subject ─────────────────────────────────────────────────────────────
    private val _subject = MutableStateFlow<ComposerSubject?>(null)
    val subject: StateFlow<ComposerSubject?> = _subject

    /**
     * EXP-980: the synced pools the blocked-start question reads — every
     * relation and every issue. A blocker on another board still blocks, so
     * neither may be scoped to the picked issues.
     */
    val allIssues: StateFlow<List<IssueEntity>> =
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val allRelations: StateFlow<List<IssueRelationEntity>> =
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * EXP-980: what still BLOCKS the picked issues — the open `blocks`
     * relations, resolved against the synced rows. For a BATCH only blockers
     * OUTSIDE the picked set count (`IssueGraph.openBlockersOfSet`, the ×4
     * rule): one picked into the same batch is not in its way. Empty for a
     * chat, an action, and whenever nothing open is left blocking.
     */
    val openBlockers: StateFlow<List<IssueEntity>> = combine(
        _subject.map { (it as? ComposerSubject.Issues)?.ids.orEmpty() }.distinctUntilChanged(),
        allRelations,
        allIssues,
    ) { ids, relations, issues ->
        if (ids.isEmpty()) emptyList() else IssueGraph.openBlockersOfSet(ids, relations, issues)
        // EAGER (EXP-1215): nothing on screen collects this, and a
        // WhileSubscribed flow would leave the issue/relation pools at their
        // empty seeds; eager keeps them live for the composer's lifetime.
        // [submit] does NOT read this flow's `.value` (a combine lands a hop
        // behind its sources): it recomputes from the pools on the tap.
    }.stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    /**
     * SLOP-3: every synced board; an issue's repository is its board's
     * `repository_id`. Eager so a submit reads a warm value.
     */
    private val allBoards: StateFlow<List<BoardEntity>> =
        dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    // ── Home (EXP-1253, composer-home.json) ─────────────────────────────────
    /** The caller's user-row name: the greeting's `{name}` (null = none synced). */
    val userName: StateFlow<String?> = combine(dbFlow, auth.userId) { db, userId -> db to userId }
        .flatMapLatest { (db, userId) ->
            if (db == null || userId == null) flowOf(null) else db.userDao().observeById(userId).map { it?.name }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * Whether THIS team has a live board; null until the boards have synced.
     * The home shows Getting started under the composer until it does.
     */
    val teamHasBoard: StateFlow<Boolean?> = combine(dbFlow, allBoards, teamId) { db, boards, team ->
        if (db == null || team == null) null else boards.any { it.teamId == team && it.deletedAt == null }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // ── Draft ───────────────────────────────────────────────────────────────
    private val _draft = MutableStateFlow("")
    /** The text — the screen owns the caret (`TextFieldValue`), this the string. */
    val draft: StateFlow<String> = _draft

    private val _images = MutableStateFlow<List<PendingAttachment>>(emptyList())
    val images: StateFlow<List<PendingAttachment>> = _images

    /** A refused pick (count / size caps): invalid input, under the composer. */
    private val _imageError = MutableStateFlow<String?>(null)
    val imageError: StateFlow<String?> = _imageError

    /** FEED-79: a start's attachment upload failed (the server's message); a toast. */
    private val _uploadFailure = MutableStateFlow<String?>(null)
    val uploadFailure: StateFlow<String?> = _uploadFailure

    fun clearUploadFailure() {
        _uploadFailure.value = null
    }

    /**
     * EXP-615/739: the chat's OPTIONAL repository — "" = none, and the run
     * goes to the agent's scratch dir. EXP-993: mobile has no picker for it
     * (the composer's options line is pills on a phone, and one of them can
     * never be a repository list), so this is seeded from the team and never
     * shown.
     */
    private val _chatRepoId = MutableStateFlow("")
    /** S-3: the + menu's Repository › (a team with more than one). */
    val chatRepoId: StateFlow<String> = _chatRepoId

    fun setChatRepo(repoId: String) {
        _chatRepoId.value = repoId
    }

    /** EXP-481: "Resume previous session" — ON whenever it is offerable; a
     * manual flip sticks for the page's lifetime. */
    private val _resume = MutableStateFlow(true)
    val resume: StateFlow<Boolean> = _resume

    private val _sending = MutableStateFlow(false)
    /** The upload + send phase is in flight. */
    val sending: StateFlow<Boolean> = _sending

    /**
     * EXP-1271 (feedback-states.json `starting`): from the Start press until
     * the run's synced row lands. The card shows "Starting on <device>" with
     * the mark; the draft stays put so a timeout leaves it ready to retry.
     */
    private val _starting = MutableStateFlow<ComposerStarting?>(null)
    val starting: StateFlow<ComposerStarting?> = _starting

    /**
     * The device a start gave up on after `starting.timeoutMs` — the page
     * raises the timeout toast (Retry re-submits) and consumes it.
     */
    private val _startTimedOut = MutableStateFlow<String?>(null)
    val startTimedOut: StateFlow<String?> = _startTimedOut

    /** The timeout toast on screen, dismissed when the late row lands. */
    private var timeoutToastId: String? = null
    private val _dismissToast = MutableStateFlow<String?>(null)
    val dismissToast: StateFlow<String?> = _dismissToast

    fun consumeStartTimedOut(toastId: String? = null) {
        _startTimedOut.value = null
        timeoutToastId = toastId
    }

    fun consumeDismissToast() {
        _dismissToast.value = null
    }

    /**
     * Drop the delegate's refused-send state once the page has toasted it.
     * [toastId] = that failure toast: a start of unknown fate is still
     * followed (recheck R3), and its late row takes the toast down.
     */
    fun clearRunFailure(toastId: String? = null) {
        steerLaunch.clearFailure()
        if (toastId != null) timeoutToastId = toastId
    }

    init {
        // EXP-1291: an accepted `sessions.send` answers with the session at
        // once (a `queued` row the device picks up) — the draft clears and
        // the page opens it.
        viewModelScope.launch {
            steerLaunch.startedSessionId.collect { id ->
                if (id == null) return@collect
                steerLaunch.consumeStartedSession()
                _starting.value = null
                timeoutToastId?.let { _dismissToast.value = it }
                timeoutToastId = null
                clearSentDraft()
                _openSessionId.value = id
            }
        }
    }

    private fun clearSentDraft() {
        _images.value = emptyList()
        _draft.value = ""
        _subject.value = null
    }

    // ── Launch options (EXP-437 seeding) ────────────────────────────────────
    private val _launch = MutableStateFlow(LaunchDraft())
    val launch: StateFlow<LaunchDraft> = _launch

    /**
     * The settled machine: the preferred one when it is still startable, else
     * the caller's default (EXP-622), else the first candidate. Null while the
     * pool is unresolved or empty.
     */
    val device: StateFlow<SteerDevice?> = combine(candidateDevices, _launch) { devices, draft ->
        LaunchDeviceRules.resolve(devices.orEmpty(), draft.requestedDeviceId, draft.pickedDeviceId)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-836: why the machine a play button NAMED is not the one this run
     * would go to (offline by now, every agent signed out there, removed from
     * the registry) — the amber note on the options line, web strings verbatim.
     * Null while there is nothing to say.
     */
    val deviceRequestNote: StateFlow<String?> =
        combine(_launch, device, steerLaunch.registry) { draft, settled, registry ->
            LaunchDeviceRules.requestNote(draft.requestedDeviceId, settled?.deviceId, registry)
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // The machine whose defaults currently seed the options — a re-poll that
    // re-emits the SAME device must not stomp the user's edits (EXP-437).
    private var seededDeviceId: String? = null

    // EXP-1278: the seeded machine's login list — a device change reads the
    // pick off it and carries it when the new machine has the same login.
    private var seededAccountOptions: List<AccountOption> = emptyList()

    // ── `@` / `#` vocabularies (EXP-802/EXP-805) ─────────────────────────────

    /**
     * The team's issues, newest-first — the `#` autocomplete and the chips a
     * typed `#APP-3` renders as (the AgentSessionViewModel shape, status
     * glyph precomputed).
     */
    val issueRefCandidates: StateFlow<List<IssueRefTarget>> = teamId
        .flatMapLatest { teamId ->
            if (teamId == null) {
                flowOf(emptyList())
            } else {
                combine(
                    allIssues,
                    dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
                    dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) {
                        it.issueStatusDao().observeByTeam(teamId)
                    },
                ) { issues, boards, statusRows ->
                    val statuses =
                        if (statusRows.isEmpty()) IssueStatusResolver.builtinDefaults
                        else IssueStatusResolver.teamStatuses(statusRows)
                    val teamBoardIds = boards.filter { it.teamId == teamId }.map { it.id }.toSet()
                    issues
                        .filter { it.boardId in teamBoardIds }
                        .sortedByDescending { it.createdAt }
                        .map { issueRefTarget(it, statuses) }
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * EXP-892: the composer's `#` menu server half — `issues.search` over the
     * selected team. Failures degrade to local-only.
     */
    suspend fun searchIssueRefs(query: String): List<IssueRefTarget> {
        val accountId = auth.activeAccountId.value ?: return emptyList()
        val team = teamId.value ?: return emptyList()
        return try {
            issuesApi.search(accountId, team, query).map(::issueRefTarget)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            emptyList()
        }
    }

    /** The team's members — the `@` vocabulary (stored as the plain `@email`). */
    val mentionMembers: StateFlow<List<MentionMember>> = teamId
        .flatMapLatest { teamId ->
            if (teamId == null) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeByTeam(teamId) }
            }
        }
        .map { users -> users.map { MentionMember(it.name ?: it.email, it.email) } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    init {
        applySeed(seed)
        // A settled machine seeds every option ONCE (agent, then that agent's
        // advertised model/effort/toggles); the SAME machine re-emitting only
        // re-clamps the agent to what it can still run.
        viewModelScope.launch {
            device.collect { settled ->
                settled ?: return@collect
                // EXP-1158: the machine's LAST USED login names the agent — the
                // last used agent's active login, or the first login it reports.
                val options = accountOptionsFor(settled, availableAgentsFor(settled))
                if (seededDeviceId == settled.deviceId) {
                    seededAccountOptions = options
                    val available = availableAgentsFor(settled)
                    // A heartbeat can land AFTER the machine settled: a pick
                    // that no longer names a reported login goes back to the
                    // last used one (idempotent — the next beat matches again).
                    if (options.none { it.key == _launch.value.accountKey }) {
                        AccountOptions.lastUsed(options)?.let(::selectAccount)
                    } else if (_launch.value.agent !in available) {
                        applyAgentSeed(available.firstOrNull() ?: DEFAULT_AGENT, settled)
                    }
                    return@collect
                }
                val previous = seededAccountOptions.firstOrNull { it.key == _launch.value.accountKey }
                    ?: seededAccountOptions.firstOrNull()
                seededDeviceId = settled.deviceId
                seededAccountOptions = options
                // EXP-1278: the same login on the new machine keeps the whole
                // pick — the agent's model/effort/toggles stay as they were.
                val carried = AccountOptions.carried(options, previous)
                if (carried != null && carried.agent == _launch.value.agent) {
                    applyAccountPick(carried)
                    return@collect
                }
                val lastUsed = AccountOptions.lastUsed(options)
                applyAgentSeed(lastUsed?.agent ?: defaultAgentFor(settled), settled)
                applyAccountPick(lastUsed)
            }
        }
    }

    // ── Seed ────────────────────────────────────────────────────────────────

    /**
     * Apply a preselection: a prompt wins over `issues` (no subject = a
     * plain message); text lands only in an EMPTY draft; a device seed is a
     * PREFERENCE (sticky until the user picks).
     */
    fun applySeed(seed: AgentComposerSeed) {
        val promptId = seed.promptId
        if (promptId != null) {
            _subject.value = ComposerSubject.Prompt(promptId)
        } else if (seed.effectiveIssueIds.isNotEmpty()) {
            _subject.value = ComposerSubject.Issues(seed.effectiveIssueIds.distinct())
        }
        seed.deviceId?.let { id -> _launch.value = _launch.value.copy(requestedDeviceId = id) }
        val text = seed.text
        if (!text.isNullOrEmpty() && _draft.value.isBlank()) _draft.value = text
    }

    // ── Subject actions ─────────────────────────────────────────────────────

    /** Picking an issue while a prompt is the subject SWAPS (EXP-825). */
    fun toggleIssue(issueId: String) {
        val current = _subject.value
        _subject.value = if (current !is ComposerSubject.Issues) {
            ComposerSubject.Issues(listOf(issueId))
        } else {
            val ids = if (issueId in current.ids) current.ids - issueId else current.ids + issueId
            if (ids.isEmpty()) null else ComposerSubject.Issues(ids)
        }
        steerLaunch.clearFailure()
    }

    /** NEW-2: pick a saved prompt as the subject (the `/` menu, NEW-7). */
    fun pickPrompt(promptId: String) {
        _subject.value = ComposerSubject.Prompt(promptId)
        steerLaunch.clearFailure()
    }

    fun clearPrompt() {
        if (_subject.value is ComposerSubject.Prompt) _subject.value = null
        steerLaunch.clearFailure()
    }

    /** NEW-3: Message an agent ›'s pick — the agent replaces whatever subject
     *  is up (picking an issue or a prompt swaps it back out). */
    fun pickAgent(agentId: String) {
        _subject.value = ComposerSubject.Agent(agentId)
        steerLaunch.clearFailure()
    }

    fun clearAgent() {
        if (_subject.value is ComposerSubject.Agent) _subject.value = null
        steerLaunch.clearFailure()
    }

    fun setDraft(text: String) {
        if (text == _draft.value) return
        _draft.value = text
        // A refused send's red caption goes away as the user edits — the
        // next attempt is a new one.
        steerLaunch.clearFailure()
    }

    /**
     * EXP-993: a chat takes the team's FIRST repository. With no picker on
     * this client the alternative was a scratch-dir run on a team that has a
     * repo, which is never what someone typing into the composer meant; a
     * team with none still runs repo-less.
     */
    fun defaultChatRepo(repoIds: List<String>) {
        if (_chatRepoId.value.isEmpty()) _chatRepoId.value = repoIds.firstOrNull().orEmpty()
    }

    fun setResume(value: Boolean) {
        _resume.value = value
    }

    /**
     * Queue a picked image or file (EXP-511 semantics, wave D: any type): an
     * image drops its `[Image #k]` marker at the caret (the composer counts
     * images only); a file is a tile with no marker. Images ≤ 10 MB × 4,
     * files ≤ 50 MB × 4; a refusal reports back through [imageError].
     */
    fun addImage(uri: Uri, bytes: ByteArray, filename: String, contentType: String) {
        _imageError.value = null
        val canonical = canonicalContentType(contentType)
        val isImage = isInlineImage(canonical)
        val current = _images.value
        val refusal = steerAttachmentRefusal(
            imageCount = current.count { it.isImage },
            fileCount = current.count { !it.isImage },
            isImage = isImage,
            sizeBytes = bytes.size.toLong(),
        )
        if (refusal != null) {
            _imageError.value = refusal
            return
        }
        _images.value = current + PendingAttachment(
            uri = uri,
            bytes = bytes,
            filename = filename,
            contentType = canonical,
            isImage = isImage,
        )
    }

    /** A pick refused before it could be read (wave D: over the size cap). */
    fun refuseAttachment(message: String) {
        _imageError.value = message
    }

    fun removeImage(index: Int) {
        _images.value = _images.value.filterIndexed { i, _ -> i != index }
    }

    // ── Options ─────────────────────────────────────────────────────────────

    /** A person's pick — it replaces the seed's request for good (EXP-836). */
    fun setDevice(deviceId: String) {
        _launch.value = _launch.value.copy(pickedDeviceId = deviceId, requestedDeviceId = null)
    }

    /**
     * EXP-872: the ONE launch pick — a login. It sets the ACCOUNT the run
     * spends and, since every option carries its agent, the agent too: a
     * different agent re-seeds every option that follows it (the vocabularies
     * differ per agent and so do the machine's advertised defaults, EXP-437),
     * while another login of the SAME agent leaves the rest of the draft
     * alone.
     */
    fun selectAccount(option: AccountOption) {
        if (option.agent != _launch.value.agent) applyAgentSeed(option.agent, device.value)
        applyAccountPick(option)
    }

    /** The picked login as the wire carries it ([LaunchDraft.withAccount]). */
    private fun applyAccountPick(option: AccountOption?) {
        _launch.value = _launch.value.withAccount(option)
    }

    fun setModel(value: String) {
        _launch.value = _launch.value.copy(model = value)
    }

    fun setEffort(value: String) {
        _launch.value = _launch.value.copy(effort = value)
    }

    /** EXP-981: claude's subagent model (always explicit). */
    fun setSubagentModel(value: String) {
        _launch.value = _launch.value.copy(subagentModel = value)
    }

    fun setUltracode(value: Boolean) {
        _launch.value = _launch.value.copy(ultracode = value)
    }

    // ── EXP-1249: the "+" menu's MCP servers + Computer use ─────────────────

    private val _mcpServers = MutableStateFlow<List<McpServerOption>>(emptyList())

    /** The team's MCP servers (`mcpServers.list`), the caller's readiness on each. */
    val mcpServers: StateFlow<List<McpServerOption>> = _mcpServers

    private val _mcpServerIds = MutableStateFlow<List<String>>(emptyList())

    /** The servers this run gets — seeded with the ready enabled-by-default rows. */
    val mcpServerIds: StateFlow<List<String>> = _mcpServerIds

    /** Toggle a READY server in or out of the pick (a not-ready row is inert). */
    fun toggleMcpServer(id: String) {
        val server = _mcpServers.value.firstOrNull { it.id == id } ?: return
        val current = _mcpServerIds.value
        _mcpServerIds.value = when {
            id in current -> current - id
            mcpServerReady(server) -> current + id
            else -> current
        }
    }

    /** Web M12: the person's explicit flip; null = untouched (the device's
     *  default shows and nothing rides the wire). Cleared per device. */
    private val _computerUsePick = MutableStateFlow<Boolean?>(null)

    /** The per-run computer use toggle: the flip, else the device's CURRENT
     *  `launch_defaults.computerUse`. */
    val computerUse: StateFlow<Boolean> = combine(_computerUsePick, device) { pick, d ->
        pick ?: (d?.launchDefaults?.computerUse == true)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, false)

    fun setComputerUse(value: Boolean) {
        _computerUsePick.value = value
    }

    init {
        // The pick reseeds per team; a refetch keeps what is still ready.
        viewModelScope.launch {
            teamId.collectLatest { team ->
                _mcpServers.value = emptyList()
                _mcpServerIds.value = emptyList()
                val accountId = auth.activeAccountId.value
                if (team == null || accountId == null) return@collectLatest
                val servers = runCatching { mcpServersApi.list(accountId, team) }
                    .onFailure { if (it is CancellationException) throw it }
                    .getOrNull() ?: return@collectLatest
                _mcpServers.value = servers
                _mcpServerIds.value = preselectMcpServerIds(servers, saved = null)
            }
        }
        // A per-DEVICE switch: changing the device clears the flip (an agent
        // switch keeps whatever the person set).
        viewModelScope.launch {
            device.map { it?.deviceId }.distinctUntilChanged().collect {
                _computerUsePick.value = null
            }
        }
    }

    private fun applyAgentSeed(agent: String, device: SteerDevice?) {
        val seed = agentSeed(device, agent)
        _launch.value = _launch.value.copy(
            agent = agent,
            model = seed.model,
            subagentModel = seed.subagentModel,
            effort = seed.effort,
            ultracode = seed.ultracode,
            // Profiles are per machine AND per agent — reset on either change.
            account = "",
        )
    }

    // ── Submit ──────────────────────────────────────────────────────────────

    /**
     * EXP-1291: the composer's runner pickers + subject as ONE `sessions.send`
     * NEW ROOT input (in the ACTIVE team, on the settled device). [text] is
     * the composed prompt (image embeds included), [attachmentIds] the
     * uploaded files. Ultracode and computer use ride only where the agent /
     * device reads them (S-3: no plan mode).
     */
    fun sendInput(
        teamId: String,
        deviceId: String,
        text: String?,
        attachmentIds: List<String>,
    ): SessionsSendInput {
        val draft = _launch.value
        val agent = draft.agent
        val subject = _subject.value
        return SessionsSendInput.root(
            teamId = teamId,
            deviceId = deviceId,
            text = text,
            attachmentIds = attachmentIds,
            issueIds = (subject as? ComposerSubject.Issues)?.ids,
            promptId = (subject as? ComposerSubject.Prompt)?.id,
            // EXP-993: a chat (no subject) runs in the picked repo, never a
            // scratch dir by accident; a subject resolves its own repo.
            repositoryId = _chatRepoId.value.takeIf { subject == null && it.isNotEmpty() },
            cli = agent,
            account = draft.wireAccount,
            model = draft.model.takeIf { it.isNotEmpty() },
            effort = draft.effort.takeIf { it.isNotEmpty() },
            ultracode = if (agent == DEFAULT_AGENT && draft.ultracode) true else null,
            // The row stores what was sent (NOT NULL, default off), so send the
            // SHOWN value, device default included, not only an explicit flip.
            computerUse = computerUse.value.takeIf { device.value?.canToggleComputerUse == true },
            mcpServerIds = _mcpServerIds.value,
        )
    }

    /**
     * Upload the pending images sequentially (each id stamped as it lands,
     * so a retry after a mid-batch failure only uploads the rest), compose
     * the text, then ONE `sessions.send` (a new thread root). The draft,
     * images and subject clear only once the send was ACCEPTED — a refused
     * start keeps everything to retry.
     */
    fun submit(@Suppress("UNUSED_PARAMETER") resumeOffered: Boolean = false) {
        if (_sending.value) return
        (_subject.value as? ComposerSubject.Agent)?.let { agent ->
            submitToAgent(agent.id)
            return
        }
        val target = device.value ?: return
        val teamId = teamId.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        val label = target.deviceLabel.ifBlank { target.deviceId }
        viewModelScope.launch {
            _sending.value = true
            timeoutToastId?.let { _dismissToast.value = it }
            timeoutToastId = null
            _starting.value = ComposerStarting(label, System.currentTimeMillis())
            var accepted = false
            try {
                try {
                    var current = _images.value
                    for ((index, image) in current.withIndex()) {
                        if (image.uploadedId == null) {
                            val uploaded = imagesApi.uploadTeamSessionImage(
                                accountId,
                                teamId,
                                image.bytes,
                                image.filename,
                                image.contentType,
                            )
                            current = current.mapIndexed { i, entry ->
                                if (i == index) entry.copy(uploadedId = uploaded.id, uploadedName = uploaded.filename) else entry
                            }
                            _images.value = current
                        }
                    }
                } catch (t: Throwable) {
                    if (t is CancellationException) throw t
                    _uploadFailure.value = trpcErrorMessage(t, "")
                    return@launch
                }
                val uploadedSet = _images.value
                val text = AgentComposerPrompt.build(
                    _draft.value,
                    uploadedSet.uploadedImageIds(),
                    uploadedSet.uploadedSteerFiles(),
                )
                val input = sendInput(
                    teamId = teamId,
                    deviceId = target.deviceId,
                    text = text,
                    attachmentIds = uploadedSet.mapNotNull { it.uploadedId },
                )
                accepted = steerLaunch.send(input, label) != null
            } finally {
                _sending.value = false
                if (!accepted) _starting.value = null
            }
        }
    }

    /**
     * NEW-3: the agent subject's send — `sessions.send({agentId, text,
     * attachmentIds})` into the caller's DM with it (created on first use; it
     * runs on the agent's device and login, so no runner picks). The pending
     * files upload against the team first; the accepted send lands on the DM
     * (the delegate's `startedSessionId`) and clears the draft.
     */
    private fun submitToAgent(agentId: String) {
        val teamId = teamId.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _sending.value = true
            try {
                try {
                    var current = _images.value
                    for ((index, image) in current.withIndex()) {
                        if (image.uploadedId == null) {
                            val uploaded = imagesApi.uploadTeamSessionImage(
                                accountId,
                                teamId,
                                image.bytes,
                                image.filename,
                                image.contentType,
                            )
                            current = current.mapIndexed { i, entry ->
                                if (i == index) entry.copy(uploadedId = uploaded.id, uploadedName = uploaded.filename) else entry
                            }
                            _images.value = current
                        }
                    }
                } catch (t: Throwable) {
                    if (t is CancellationException) throw t
                    _uploadFailure.value = trpcErrorMessage(t, "")
                    return@launch
                }
                val uploadedSet = _images.value
                val text = AgentComposerPrompt.build(
                    _draft.value,
                    uploadedSet.uploadedImageIds(),
                    uploadedSet.uploadedSteerFiles(),
                ).orEmpty()
                steerLaunch.send(
                    SessionsSendInput.toAgent(
                        agentId = agentId,
                        text = text,
                        attachmentIds = uploadedSet.mapNotNull { it.uploadedId },
                    ),
                )
            } finally {
                _sending.value = false
            }
        }
    }
}
