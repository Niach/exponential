package com.exponential.app.ui.agent

import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.builtinChatAction
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.ActionInputValues
import com.exponential.app.domain.AgentComposerPrompt
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.MAX_STEER_IMAGES
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.RunResumeTarget
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineImage
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.agentSeed
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.components.defaultAgentFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.markdown.IssueRefTarget
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
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

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

/** What the composer is about — issue chips OR one action chip, never both. */
sealed interface ComposerSubject {
    data class Issues(val ids: List<String>) : ComposerSubject

    /** [inputs] are the typed pick values keyed by def key; "" = cleared. */
    data class Action(val id: String, val inputs: Map<String, String>) : ComposerSubject
}

/**
 * The launch options as picked (EXP-437 seeding rules): [deviceId] is a
 * PREFERENCE resolved against the startable pool (null = the default machine),
 * [account] is a login profile id or "" for the machine's active login.
 */
data class LaunchDraft(
    val deviceId: String? = null,
    val agent: String = DEFAULT_AGENT,
    val model: String = "",
    val effort: String = "",
    val ultracode: Boolean = false,
    val planMode: Boolean = false,
    val account: String = "",
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentComposerViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val steerLaunch: SteerLaunchDelegate,
    private val imagesApi: IssueImagesApi,
) : ViewModel() {

    /** The route's one-shot preselection — applied once in `init`. */
    val seed: AgentComposerSeed = AgentComposerSeed.fromArgs(savedStateHandle)

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The selected team: the pools, the builtins' teamId, the upload scope. */
    val teamId: StateFlow<String?> = selection.selectedId

    // ── The shared launch rails ─────────────────────────────────────────────
    val steerEnabled: StateFlow<Boolean?> get() = steerLaunch.enabled
    val runState: StateFlow<ActionRunState> get() = steerLaunch.runState
    val startedSessionId: StateFlow<String?> get() = steerLaunch.startedSessionId
    val startCandidates: StateFlow<List<IssueOption>> get() = steerLaunch.startCandidates

    fun consumeStartedSession() = steerLaunch.consumeStartedSession()

    /** EXP-773: Resume an ended run from the Past list — the same rails. */
    fun resumeRun(target: RunResumeTarget) = steerLaunch.resumeRun(target)

    /** Every ONLINE machine, runnable or not — the signed-out caption's source. */
    val onlineDevices: StateFlow<List<SteerDevice>?> get() = steerLaunch.devices

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

    /** EXP-323: the PR an action's `pr` input should open pre-picked on —
     * consumed by the screen once the options pool resolves it. */
    private val _pendingPrIssueId = MutableStateFlow<String?>(null)
    val pendingPrIssueId: StateFlow<String?> = _pendingPrIssueId

    fun consumePendingPr() {
        _pendingPrIssueId.value = null
    }

    // EXP-349: the last action id whose repo inputs were seeded — the latch
    // keeps a manual re-pick (including clearing to "None") from being
    // re-seeded when the synced rows update.
    private var seededRepoActionId: String? = null

    // ── Draft ───────────────────────────────────────────────────────────────
    private val _draft = MutableStateFlow("")
    /** The text — the screen owns the caret (`TextFieldValue`), this the string. */
    val draft: StateFlow<String> = _draft

    private val _images = MutableStateFlow<List<PendingAttachment>>(emptyList())
    val images: StateFlow<List<PendingAttachment>> = _images

    private val _imageError = MutableStateFlow<String?>(null)
    val imageError: StateFlow<String?> = _imageError

    /** EXP-615/739: the chat's OPTIONAL repository — "" = none. */
    private val _chatRepoId = MutableStateFlow("")
    val chatRepoId: StateFlow<String> = _chatRepoId

    /** EXP-481: "Resume previous session" — ON whenever it is offerable; a
     * manual flip sticks for the page's lifetime. */
    private val _resume = MutableStateFlow(true)
    val resume: StateFlow<Boolean> = _resume

    private val _sending = MutableStateFlow(false)
    /** The upload + send phase is in flight. */
    val sending: StateFlow<Boolean> = _sending

    // ── Launch options (EXP-437 seeding) ────────────────────────────────────
    private val _launch = MutableStateFlow(LaunchDraft())
    val launch: StateFlow<LaunchDraft> = _launch

    /**
     * The settled machine: the preferred one when it is still startable, else
     * the caller's default (EXP-622), else the first candidate. Null while the
     * pool is unresolved or empty.
     */
    val device: StateFlow<SteerDevice?> = combine(candidateDevices, _launch) { devices, draft ->
        val pool = devices.orEmpty()
        pool.firstOrNull { it.deviceId == draft.deviceId }
            ?: pool.firstOrNull { it.isDefault }
            ?: pool.firstOrNull()
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // The machine whose defaults currently seed the options — a re-poll that
    // re-emits the SAME device must not stomp the user's edits (EXP-437).
    private var seededDeviceId: String? = null

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
                    dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
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
                        .map {
                            IssueRefTarget(
                                it.id,
                                it.identifier,
                                it.title,
                                IssueStatusResolver.resolve(it, statuses),
                            )
                        }
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

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
        steerLaunch.attach(viewModelScope)
        applySeed(seed)
        // A settled machine seeds every option ONCE (agent, then that agent's
        // advertised model/effort/toggles); the SAME machine re-emitting only
        // re-clamps the agent to what it can still run.
        viewModelScope.launch {
            device.collect { settled ->
                settled ?: return@collect
                if (seededDeviceId == settled.deviceId) {
                    val available = availableAgentsFor(settled)
                    if (_launch.value.agent !in available) {
                        applyAgentSeed(available.firstOrNull() ?: DEFAULT_AGENT, settled)
                    }
                    return@collect
                }
                seededDeviceId = settled.deviceId
                applyAgentSeed(defaultAgentFor(settled), settled)
            }
        }
    }

    // ── Seed ────────────────────────────────────────────────────────────────

    /**
     * Apply a preselection: `action` wins over `issues` (the hidden Chat
     * builtin is the no-subject state, not a chip); text lands only in an
     * EMPTY draft; a `pr` seed waits for the options pool; a device seed is a
     * PREFERENCE (sticky until the user picks). Also what an in-page "Fix
     * conflicts" tap calls.
     */
    fun applySeed(seed: AgentComposerSeed) {
        val actionId = seed.actionId
        if (actionId != null && actionId != DomainContract.builtinChatId) {
            seededRepoActionId = null
            _subject.value = ComposerSubject.Action(
                actionId,
                if (seed.icon.isNullOrEmpty()) emptyMap() else mapOf("icon" to seed.icon),
            )
            _pendingPrIssueId.value = seed.prIssueId
        } else if (seed.effectiveIssueIds.isNotEmpty()) {
            _subject.value = ComposerSubject.Issues(seed.effectiveIssueIds.distinct())
            _pendingPrIssueId.value = null
        }
        seed.deviceId?.let { id -> _launch.value = _launch.value.copy(deviceId = id) }
        val text = seed.text
        if (!text.isNullOrEmpty() && _draft.value.isBlank()) _draft.value = text
        reseedPlanMode()
    }

    // ── Subject actions ─────────────────────────────────────────────────────

    /** Picking an issue while an action is the subject SWAPS (EXP-825). */
    fun toggleIssue(issueId: String) {
        val current = _subject.value
        _subject.value = if (current !is ComposerSubject.Issues) {
            ComposerSubject.Issues(listOf(issueId))
        } else {
            val ids = if (issueId in current.ids) current.ids - issueId else current.ids + issueId
            if (ids.isEmpty()) null else ComposerSubject.Issues(ids)
        }
        _pendingPrIssueId.value = null
        reseedPlanMode()
    }

    fun pickAction(actionId: String) {
        if (actionId == DomainContract.builtinChatId) {
            _subject.value = null
        } else {
            val current = _subject.value
            // A different action has a different input schema — stale values
            // must not leak into the new one's payload.
            if (current !is ComposerSubject.Action || current.id != actionId) {
                _subject.value = ComposerSubject.Action(actionId, emptyMap())
            }
        }
        _pendingPrIssueId.value = null
        reseedPlanMode()
    }

    fun clearAction() {
        if (_subject.value is ComposerSubject.Action) _subject.value = null
        _pendingPrIssueId.value = null
        reseedPlanMode()
    }

    fun setInput(key: String, value: String) {
        val current = _subject.value as? ComposerSubject.Action ?: return
        _subject.value = current.copy(inputs = current.inputs + (key to value))
    }

    /**
     * EXP-349: pre-fill `repo` inputs with the action's bound repository once
     * per selection — current-wins, so a seeded icon, a `pr` pick or a manual
     * value is never stomped. The screen calls it whenever the resolved action
     * row is on hand (a seeded action's synced row may land later).
     */
    fun seedRepoInputs(action: ActionDto) {
        val current = _subject.value as? ComposerSubject.Action ?: return
        if (current.id != action.id || seededRepoActionId == action.id) return
        seededRepoActionId = action.id
        val repoId = action.repositoryId ?: return
        val seeds = action.inputs.orEmpty()
            .filter { it.type == "repo" && current.inputs[it.key] == null }
            .associate { it.key to repoId }
        if (seeds.isNotEmpty()) _subject.value = current.copy(inputs = seeds + current.inputs)
    }

    // ── Draft ───────────────────────────────────────────────────────────────

    fun setDraft(text: String) {
        _draft.value = text
    }

    fun setChatRepoId(repoId: String) {
        _chatRepoId.value = repoId
    }

    /** A team with exactly one repository pre-picks it for a chat (web parity). */
    fun defaultChatRepo(repoIds: List<String>) {
        if (_chatRepoId.value.isEmpty() && repoIds.size == 1) _chatRepoId.value = repoIds.first()
    }

    fun setResume(value: Boolean) {
        _resume.value = value
    }

    /**
     * Queue a picked image (EXP-511 semantics): the composer drops its
     * `[Image #k]` marker at the caret. Capped at [MAX_STEER_IMAGES]; anything
     * the inline-image pipeline rejects reports back through [imageError].
     */
    fun addImage(uri: Uri, bytes: ByteArray, filename: String, contentType: String) {
        _imageError.value = null
        if (_images.value.size >= MAX_STEER_IMAGES) {
            _imageError.value = "Up to $MAX_STEER_IMAGES images per message"
            return
        }
        val canonical = canonicalContentType(contentType)
        if (!isInlineImage(canonical)) {
            _imageError.value = "Only images can be attached"
            return
        }
        _images.value = _images.value + PendingAttachment(
            uri = uri,
            bytes = bytes,
            filename = filename,
            contentType = canonical,
            isImage = true,
        )
    }

    fun removeImage(index: Int) {
        _images.value = _images.value.filterIndexed { i, _ -> i != index }
    }

    // ── Options ─────────────────────────────────────────────────────────────

    fun setDevice(deviceId: String) {
        _launch.value = _launch.value.copy(deviceId = deviceId)
    }

    /** Every option follows the agent: the vocabularies differ per agent and
     * so do the settled machine's advertised defaults (EXP-437). */
    fun selectAgent(agent: String) {
        if (agent == _launch.value.agent) return
        applyAgentSeed(agent, device.value)
    }

    fun setModel(value: String) {
        _launch.value = _launch.value.copy(model = value)
    }

    fun setEffort(value: String) {
        _launch.value = _launch.value.copy(effort = value)
    }

    fun setUltracode(value: Boolean) {
        _launch.value = _launch.value.copy(ultracode = value)
    }

    fun setPlanMode(value: Boolean) {
        _launch.value = _launch.value.copy(planMode = value)
    }

    fun setAccount(value: String) {
        _launch.value = _launch.value.copy(account = value)
    }

    private fun applyAgentSeed(agent: String, device: SteerDevice?) {
        val seed = agentSeed(device, agent)
        _launch.value = _launch.value.copy(
            agent = agent,
            model = seed.model,
            effort = seed.effort,
            ultracode = seed.ultracode,
            // EXP-772: a chat (no subject) starts in build mode; a subject
            // takes the machine's plan-mode default.
            planMode = if (_subject.value == null) false else seed.planMode,
            // Profiles are per machine AND per agent — reset on either change.
            account = "",
        )
    }

    // EXP-772 regression guard: the always-open composer reseeds plan mode
    // when the subject flips null↔set, or a chat would inherit the device
    // default and a subject would inherit the chat's build mode.
    private var hadSubject = false
    private fun reseedPlanMode() {
        val hasSubject = _subject.value != null
        if (hasSubject == hadSubject) return
        hadSubject = hasSubject
        val current = _launch.value
        val planMode = if (hasSubject) agentSeed(device.value, current.agent).planMode else false
        _launch.value = current.copy(planMode = planMode && supportsPlanMode(current.agent))
    }

    // ── Submit ──────────────────────────────────────────────────────────────

    /**
     * The chosen options in wire form. [resume] is the composer's call:
     * single-issue starts only, and a resume never re-enters plan mode
     * (EXP-202 — clamped like the desktop dialog).
     */
    fun buildOptions(resume: Boolean): SteerStartOptions {
        val draft = _launch.value
        val agent = draft.agent
        return SteerStartOptions(
            model = draft.model,
            effort = draft.effort,
            ultracode = if (agent == DEFAULT_AGENT) draft.ultracode else null,
            planMode = when {
                resume -> false
                supportsPlanMode(agent) -> draft.planMode
                else -> null
            },
            agent = agent,
            resume = if (resume) true else null,
            account = draft.account.takeIf { it.isNotEmpty() && it != SYSTEM_PROFILE_ID },
        )
    }

    /**
     * Upload the pending images sequentially (each id stamped as it lands, so
     * a retry after a mid-batch failure only uploads the rest), compose the
     * `prompt`, then dispatch the subject: the hidden Chat builtin with no
     * subject, a single/batch issue start, or the action with its filled
     * inputs. The draft, images and subject clear only once the send was
     * ACCEPTED — a refused start keeps everything to retry.
     *
     * [action] is the resolved row for an action subject (null while its
     * synced row is missing — the gate already blocks that); [resumeOffered]
     * whether the single issue's worktree exists on the picked machine.
     */
    fun submit(action: ActionDto?, resumeOffered: Boolean) {
        if (_sending.value) return
        val target = device.value ?: return
        val teamId = teamId.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        val subject = _subject.value
        viewModelScope.launch {
            _sending.value = true
            try {
                val ids = ArrayList<String>()
                try {
                    var current = _images.value
                    for ((index, image) in current.withIndex()) {
                        var uploadedId = image.uploadedId
                        if (uploadedId == null) {
                            val uploaded = imagesApi.uploadTeamSessionImage(
                                accountId,
                                teamId,
                                image.bytes,
                                image.filename,
                                image.contentType,
                            )
                            uploadedId = uploaded.id
                            current = current.mapIndexed { i, entry ->
                                if (i == index) entry.copy(uploadedId = uploaded.id) else entry
                            }
                            _images.value = current
                        }
                        ids.add(uploadedId)
                    }
                } catch (t: Throwable) {
                    if (t is CancellationException) throw t
                    _imageError.value = trpcErrorMessage(t, "Couldn't upload image")
                    return@launch
                }
                val prompt = AgentComposerPrompt.build(_draft.value, ids)
                val resume = resumeOffered && _resume.value &&
                    (subject as? ComposerSubject.Issues)?.ids?.size == 1
                val options = buildOptions(resume)
                val accepted = when (subject) {
                    null -> {
                        // The hidden builtin has no DB row: constructed here,
                        // its one optional input riding only when picked
                        // (EXP-756 — an empty repo is NO repo).
                        val chat = builtinChatAction(teamId)
                        steerLaunch.runAction(
                            target,
                            chat,
                            options,
                            ActionInputValues.wireValues(
                                chat.inputs.orEmpty(),
                                mapOf("repo" to _chatRepoId.value),
                            ),
                            prompt,
                        )
                    }
                    is ComposerSubject.Issues ->
                        steerLaunch.startIssues(target, subject.ids, options, prompt)
                    is ComposerSubject.Action -> {
                        val row = action ?: return@launch
                        steerLaunch.runAction(
                            target,
                            row,
                            options,
                            ActionInputValues.wireValues(row.inputs.orEmpty(), subject.inputs),
                            prompt,
                        )
                    }
                }
                if (accepted) {
                    _images.value = emptyList()
                    _draft.value = ""
                    _subject.value = null
                    _pendingPrIssueId.value = null
                    reseedPlanMode()
                }
            } finally {
                _sending.value = false
            }
        }
    }

    /** The run inputs the composer would send for [defs] right now. */
    fun inputValues(): Map<String, String> =
        (_subject.value as? ComposerSubject.Action)?.inputs.orEmpty()

    /** Whether every required input of [defs] is filled. */
    fun requiredInputsFilled(defs: List<ActionInputDto>): Boolean =
        ActionInputValues.requiredFilled(defs, inputValues())
}
