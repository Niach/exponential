package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.ui.semantics.semantics
import com.exponential.app.domain.DeviceDoctor
import com.exponential.app.domain.DeviceLiveness
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.VoiceApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.AudioApp
import com.exponential.app.domain.DesktopAudioDoctor
import com.exponential.app.domain.Meeting
import com.exponential.app.domain.Voice
import com.exponential.app.domain.parseDeviceDoctor
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.picker.devicePickerItems
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.components.ShortcutKeys
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * NEW-67 (voice plan v3, V1): Settings → Personal → Voice (`voice.json`, the
 * same page on web, the IDE and iOS; on the phones it hangs off the account
 * page, the phone's Personal settings). ONE group (Voice device, Agent,
 * Language; Voice file + Transcript while a voice device is picked,
 * #NEW-73), then while a voice device is picked the tray group under
 * `trayHeader` (Push to talk, Wake word, Wake phrase). S-3: speak-back is
 * always on (no Speak replies row); Push to talk is the key binding, READ-ONLY
 * here (keys are recorded on the voice device); the Wake word row's detail
 * says why it is or is not listening (the voice device's doctor row `wake`,
 * else `wakeDetail`). Every switch
 * reads off and is disabled while the device or the agent is None. Each
 * change saves at once with `voice.update` (optimistic; a refusal reverts and
 * toasts, feedback-states.json).
 */
@HiltViewModel
class VoiceSettingsViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val voiceApi: VoiceApi,
) : ViewModel() {

    /** The account the page edits (`server/{accountId}/voice`); S-3: on
     *  the Assistant page (no `accountId` arg) the active account. */
    val accountId: String = savedStateHandle.get<String>("accountId")
        ?: auth.activeAccountId.value.orEmpty()

    private val db = holder.database(forAccountId = accountId)

    /** null until `voice.get` landed. */
    var settings by mutableStateOf<Voice.Settings?>(null)
        private set
    var loadFailed by mutableStateOf(false)
        private set

    /** The wake phrase as typed; saved a beat after typing stops. */
    var wakePhrase by mutableStateOf("")
        private set

    /**
     * #NEW-56: the Wake phrase's inline error (`wakePhraseError`), set when
     * the field is left or submitted with too few letters; cleared on the next edit.
     */
    var wakePhraseError by mutableStateOf<String?>(null)
        private set

    /** #NEW-73: the voice file and its transcript as typed, saved the same way. */
    var voiceFile by mutableStateOf("")
        private set
    var voiceTranscript by mutableStateOf("")
        private set

    /**
     * The one-line text rows (web `TEXT_FIELDS`): typed locally, saved
     * debounced, on Done or on focus loss, each reverting only ITSELF.
     */
    private enum class TextField(val display: String) {
        WakePhrase("the wake phrase"),
        VoiceFile("the voice file"),
        VoiceTranscript("the transcript"),
    }
    private val textTimers = mutableMapOf<TextField, Job>()
    private val textSeq = mutableMapOf<TextField, Int>()
    /** Last write wins: only the newest `save` may settle or revert the row. */
    private var saveSeq = 0

    /** FEED-79: a refused save, raised once as a toast. */
    var failure by mutableStateOf<FailedAction?>(null)

    private val devices: StateFlow<List<DeviceEntity>> = db.deviceDao().observeAll()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
    private val agents: StateFlow<List<AgentDefinitionEntity>> = db.agentDefinitionDao().observeAll()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
    private val teams: StateFlow<List<TeamEntity>> = db.teamDao().observeAll()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The page's vocabulary: picker options + the names the copy needs. */
    data class Vocabulary(
        val deviceOptions: List<Voice.Option> = Voice.voiceDeviceOptions(emptyList()),
        /** The non-None device options with their glyphs (web `devicePickerItems`). */
        val deviceRows: List<DevicePickerDevice> = emptyList(),
        val agentOptions: List<Voice.Option> = Voice.agentOptions(emptyList()),
        val deviceNames: Map<String, String> = emptyMap(),
        val agentNames: Map<String, String> = emptyMap(),
        /** NEW-57: each own device's `doctor.desktopAudio` (capability + live apps). */
        val desktopAudio: Map<String, DesktopAudioDoctor> = emptyMap(),
        /** S-3: each own device's doctor (its `wake` row) and whether it is online. */
        val doctors: Map<String, DeviceDoctor?> = emptyMap(),
        val online: Set<String> = emptySet(),
    )

    val vocabulary: StateFlow<Vocabulary> = combine(devices, agents, teams, DeviceLiveness.ticker()) { devices, agents, teams, now ->
        // The member's OWN devices; `voiceDeviceOptions` keeps the desktops.
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
        val teamNames = teams.associate { it.id to it.name }
        val own = devices.filter { it.userId == userId }
        val deviceOptions = Voice.voiceDeviceOptions(
            own.map { Voice.DeviceLike(deviceId = it.deviceId, name = it.label, kind = it.kind) },
        )
        val byId = own.associateBy { it.deviceId }
        Vocabulary(
            deviceOptions = deviceOptions,
            deviceRows = deviceOptions.mapNotNull { option ->
                val id = option.id ?: return@mapNotNull null
                val device = byId[id]
                DevicePickerDevice(id = id, name = option.label, icon = device?.icon, isServer = device?.kind == "server")
            },
            agentOptions = Voice.agentOptions(
                agents.map { Voice.AgentLike(id = it.id, name = it.name, team = teamNames[it.teamId].orEmpty()) },
            ),
            deviceNames = devices.associate { it.deviceId to it.label },
            agentNames = agents.associate { it.id to it.name },
            desktopAudio = own.associate { it.deviceId to (parseDeviceDoctor(it.doctor)?.desktopAudio ?: DesktopAudioDoctor()) },
            doctors = own.associate { it.deviceId to parseDeviceDoctor(it.doctor) },
            online = own.filter { DeviceLiveness.isOnline(it.lastSeenAt, now) }.map { it.deviceId }.toSet(),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), Vocabulary())

    init {
        load()
    }

    fun load() {
        loadFailed = false
        viewModelScope.launch {
            try {
                val loaded = voiceApi.get(accountId)
                settings = loaded
                wakePhrase = loaded.wakePhrase
                voiceFile = loaded.voiceFile
                voiceTranscript = loaded.voiceTranscript
            } catch (e: Exception) {
                if (settings == null) loadFailed = true
            }
        }
    }

    /**
     * Optimistic: the page reads `clearOnNone(change)` at once, takes the
     * server's row on success and reverts + toasts on a refusal. Only the
     * newest save settles or reverts the row (web `seqRef`), and the text
     * rows settle through their own saves, never through here.
     */
    fun save(change: Voice.Change) {
        val previous = settings ?: return
        val next = Voice.clearOnNone(previous, change)
        if (next == previous) return
        settings = next
        val seq = ++saveSeq
        viewModelScope.launch {
            try {
                val stored = voiceApi.update(accountId, change)
                if (seq == saveSeq) settings = keepText(stored)
            } catch (e: Exception) {
                if (seq == saveSeq) settings = keepText(previous)
                failure = FailedAction("save", "the voice settings", trpcErrorMessage(e, ""))
            }
        }
    }

    /**
     * NEW-57: THE consent dialog's Start ([on]) or the toggle turned off:
     * optimistic like [save] (the row reads the meeting at once, a refusal
     * reverts and toasts); only `voice.startMeeting` / `voice.stopMeeting`
     * write it, never `voice.update`.
     */
    fun setMeeting(on: Boolean) {
        val previous = settings ?: return
        if ((previous.meetingStartedAt != null) == on) return
        settings = previous.copy(meetingStartedAt = if (on) java.time.Instant.now().toString() else null)
        val seq = ++saveSeq
        viewModelScope.launch {
            try {
                val stored = if (on) voiceApi.startMeeting(accountId) else voiceApi.stopMeeting(accountId)
                if (seq == saveSeq) settings = keepText(stored)
            } catch (e: Exception) {
                if (seq == saveSeq) settings = keepText(previous)
                failure = FailedAction(if (on) "start" else "stop", "meeting mode", trpcErrorMessage(e, ""))
            }
        }
    }

    /** [base] with the text rows as the row currently holds them. */
    private fun keepText(base: Voice.Settings): Voice.Settings {
        val current = settings ?: return base
        return base.copy(
            wakePhrase = current.wakePhrase,
            voiceFile = current.voiceFile,
            voiceTranscript = current.voiceTranscript,
        )
    }

    /** One line, no control characters, at most `limits.wakePhrase`. */
    fun onWakePhraseChange(text: String) {
        wakePhraseError = null
        editText(TextField.WakePhrase, text, Voice.Limits.WAKE_PHRASE)
    }

    /**
     * Done on the keyboard (or leaving the field) saves at once, like Return
     * on iOS; a too-short phrase is not sent and shows inline instead.
     */
    fun saveWakePhrase() = saveText(TextField.WakePhrase, submitted = true)

    /** One line, no control characters, at most `limits.voiceFile`. */
    fun onVoiceFileChange(text: String) = editText(TextField.VoiceFile, text, Voice.Limits.VOICE_FILE)

    fun saveVoiceFile() = saveText(TextField.VoiceFile)

    /** One line, no control characters, at most `limits.voiceTranscript`. */
    fun onVoiceTranscriptChange(text: String) = editText(TextField.VoiceTranscript, text, Voice.Limits.VOICE_TRANSCRIPT)

    fun saveVoiceTranscript() = saveText(TextField.VoiceTranscript)

    private fun typed(field: TextField): String = when (field) {
        TextField.WakePhrase -> wakePhrase
        TextField.VoiceFile -> voiceFile
        TextField.VoiceTranscript -> voiceTranscript
    }

    private fun setTyped(field: TextField, value: String) {
        when (field) {
            TextField.WakePhrase -> wakePhrase = value
            TextField.VoiceFile -> voiceFile = value
            TextField.VoiceTranscript -> voiceTranscript = value
        }
    }

    private fun stored(settings: Voice.Settings, field: TextField): String = when (field) {
        TextField.WakePhrase -> settings.wakePhrase
        TextField.VoiceFile -> settings.voiceFile
        TextField.VoiceTranscript -> settings.voiceTranscript
    }

    private fun withText(settings: Voice.Settings, field: TextField, value: String): Voice.Settings = when (field) {
        TextField.WakePhrase -> settings.copy(wakePhrase = value)
        TextField.VoiceFile -> settings.copy(voiceFile = value)
        TextField.VoiceTranscript -> settings.copy(voiceTranscript = value)
    }

    private fun changeOf(field: TextField, value: String): Voice.Change = when (field) {
        TextField.WakePhrase -> Voice.Change(wakePhrase = value)
        TextField.VoiceFile -> Voice.Change(voiceFile = value)
        TextField.VoiceTranscript -> Voice.Change(voiceTranscript = value)
    }

    private fun editText(field: TextField, text: String, limit: Int) {
        // #NEW-75: THE input rule ×4 (control characters dropped, cut to the
        // server's UTF-16 limit without splitting a surrogate pair).
        setTyped(field, Voice.textInput(text, limit))
        textTimers[field]?.cancel()
        textTimers[field] = viewModelScope.launch {
            delay(TEXT_DEBOUNCE_MS)
            saveText(field, submitted = false)
        }
    }

    /**
     * Saves ONE text row (web `saveText`): trimmed, skipped while unchanged;
     * a refusal reverts that row alone, and only when no newer save of the
     * same row is in flight. An invalid wake phrase is never sent (the
     * stored one stays); only a [submitted] save (Done / focus loss) shows it.
     */
    private fun saveText(field: TextField, submitted: Boolean = true) {
        textTimers.remove(field)?.cancel()
        val current = settings ?: return
        val trimmed = typed(field).trim()
        val previous = stored(current, field)
        if (field == TextField.WakePhrase) {
            val error = Voice.wakePhraseError(trimmed)
            if (error != null) {
                if (submitted) wakePhraseError = error
                return
            }
        }
        if (trimmed == previous) return
        settings = withText(current, field, trimmed)
        val seq = (textSeq[field] ?: 0) + 1
        textSeq[field] = seq
        viewModelScope.launch {
            try {
                voiceApi.update(accountId, changeOf(field, trimmed))
            } catch (e: Exception) {
                if (textSeq[field] == seq) {
                    settings?.let { settings = withText(it, field, previous) }
                    setTyped(field, previous)
                }
                failure = FailedAction("save", field.display, trpcErrorMessage(e, ""))
            }
        }
    }

    /**
     * Back within the debounce must not lose an edit: viewModelScope is
     * already closed here, so a pending text save goes out on its own scope.
     */
    override fun onCleared() {
        for (field in TextField.entries) {
            textTimers.remove(field)?.cancel()
            val current = settings ?: continue
            val trimmed = typed(field).trim()
            if (trimmed == stored(current, field)) continue
            if (field == TextField.WakePhrase && Voice.wakePhraseError(trimmed) != null) continue
            voiceSettingsFlushScope.launch { runCatching { voiceApi.update(accountId, changeOf(field, trimmed)) } }
        }
        super.onCleared()
    }

    private companion object {
        const val TEXT_DEBOUNCE_MS = 600L
    }
}

// A text save fired while the page is closing must outlive the ViewModel:
// viewModelScope is cancelled when navigation clears it. Process-lifetime,
// mirroring deviceSettingsFlushScope.
private val voiceSettingsFlushScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

/** The pickers carry a String value; None rides as the empty string. */
private const val NONE = ""

@Composable
fun VoiceSettingsScreen(
    onBack: () -> Unit,
    viewModel: VoiceSettingsViewModel = hiltViewModel(),
) {
    val vocabulary by viewModel.vocabulary.collectAsStateWithLifecycle()

    VoiceSettingsScaffold(onBack = onBack) { padding ->
        val settings = viewModel.settings
        when {
            settings != null -> VoiceSettingsForm(
                settings = settings,
                vocabulary = vocabulary,
                wakePhrase = viewModel.wakePhrase,
                wakePhraseError = viewModel.wakePhraseError,
                onChange = viewModel::save,
                onMeeting = viewModel::setMeeting,
                onWakePhraseChange = viewModel::onWakePhraseChange,
                onWakePhraseDone = viewModel::saveWakePhrase,
                voiceFile = viewModel.voiceFile,
                onVoiceFileChange = viewModel::onVoiceFileChange,
                onVoiceFileDone = viewModel::saveVoiceFile,
                voiceTranscript = viewModel.voiceTranscript,
                onVoiceTranscriptChange = viewModel::onVoiceTranscriptChange,
                onVoiceTranscriptDone = viewModel::saveVoiceTranscript,
                modifier = Modifier.padding(padding),
            )
            viewModel.loadFailed -> Box(
                Modifier.padding(padding).fillMaxSize(),
                contentAlignment = Alignment.Center,
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(
                        FeedbackStates.errorTitle("load", "the voice settings"),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    GlassPill(FeedbackStates.RETRY, icon = ExpIcons.uiRefresh, onClick = viewModel::load)
                }
            }
            else -> SkeletonRows(
                modifier = Modifier.padding(padding).padding(16.dp),
                rows = 3,
            )
        }
    }

    FailedActionToast(viewModel.failure, onShown = { viewModel.failure = null })
}

/** The page's chrome (title + back), shared with the styleguide specimen. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun VoiceSettingsScaffold(
    onBack: () -> Unit,
    content: @Composable (PaddingValues) -> Unit,
) {
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { TopBarTitle(Voice.Copy.GROUP) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
        modifier = Modifier.testTag(Voice.TestIds.PAGE),
        content = content,
    )
}

@Composable
internal fun VoiceSettingsForm(
    settings: Voice.Settings,
    vocabulary: VoiceSettingsViewModel.Vocabulary,
    wakePhrase: String,
    wakePhraseError: String?,
    onChange: (Voice.Change) -> Unit,
    onMeeting: (Boolean) -> Unit,
    onWakePhraseChange: (String) -> Unit,
    onWakePhraseDone: () -> Unit,
    voiceFile: String,
    onVoiceFileChange: (String) -> Unit,
    onVoiceFileDone: () -> Unit,
    voiceTranscript: String,
    onVoiceTranscriptChange: (String) -> Unit,
    onVoiceTranscriptDone: () -> Unit,
    modifier: Modifier = Modifier,
    /** S-3 (agent-settings.json `assistant`): the block MOVED onto the
     *  Assistant page — its rows unchanged MINUS the Voice device and Agent
     *  pickers (the device is Runs on's, the agent is the Assistant), laid
     *  into the page's own scroll. */
    assistantBlock: Boolean = false,
) {
    // voice.json: every switch is off and disabled while either is None.
    val armed = settings.voiceDeviceId != null && settings.agentId != null

    Column(
        modifier = if (assistantBlock) {
            modifier.fillMaxWidth()
        } else {
            modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(vertical = 8.dp)
        },
    ) {
        OptionGroup {
          if (!assistantBlock) {
            // THE device picker's rows (glyph + name), None first.
            val deviceItems = remember(vocabulary.deviceRows) {
                listOf(PickerItem(value = NONE, label = Voice.Copy.VOICE_DEVICE_NONE)) +
                    devicePickerItems(vocabulary.deviceRows)
            }
            OptionPickerRow(
                label = Voice.Copy.VOICE_DEVICE,
                options = vocabulary.deviceOptions,
                selected = settings.voiceDeviceId,
                noneLabel = Voice.Copy.VOICE_DEVICE_NONE,
                onSelect = { onChange(Voice.Change(voiceDeviceId = Voice.Pick(it))) },
                testTag = Voice.TestIds.VOICE_DEVICE,
                items = deviceItems,
            )
            GroupDivider()
            OptionPickerRow(
                label = Voice.Copy.AGENT,
                options = vocabulary.agentOptions,
                selected = settings.agentId,
                noneLabel = Voice.Copy.AGENT_NONE,
                onSelect = { onChange(Voice.Change(agentId = Voice.Pick(it))) },
                testTag = Voice.TestIds.AGENT,
            )
            GroupDivider()
          }
            // Every language carries its GPU note (#NEW-74), in the row and in the menu.
            val languageDevice = settings.voiceDeviceId?.let { vocabulary.deviceNames[it] ?: Voice.Copy.ELLIPSIS }
            ChoicePickerRow(
                label = Voice.Copy.LANGUAGE,
                choices = Voice.languages.map { it.value },
                selected = settings.language,
                choiceLabel = { Voice.languageLabel(it, languageDevice) },
                onSelect = { onChange(Voice.Change(language = it)) },
                testTag = Voice.TestIds.LANGUAGE,
            )
            if (settings.voiceDeviceId != null) {
                GroupDivider()
                TextFieldRow(
                    label = Voice.Copy.VOICE_FILE,
                    value = voiceFile,
                    onValueChange = onVoiceFileChange,
                    placeholder = Voice.Copy.VOICE_FILE_PLACEHOLDER,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { onVoiceFileDone() }),
                    onFocusLost = onVoiceFileDone,
                    modifier = Modifier.testTag(Voice.TestIds.VOICE_FILE),
                )
                GroupDivider()
                TextFieldRow(
                    label = Voice.Copy.VOICE_TRANSCRIPT,
                    value = voiceTranscript,
                    onValueChange = onVoiceTranscriptChange,
                    placeholder = Voice.Copy.VOICE_TRANSCRIPT_PLACEHOLDER,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { onVoiceTranscriptDone() }),
                    onFocusLost = onVoiceTranscriptDone,
                    modifier = Modifier.testTag(Voice.TestIds.VOICE_TRANSCRIPT),
                )
            }
        }

        val deviceId = settings.voiceDeviceId
        if (deviceId != null) {
            Spacer(Modifier.height(12.dp))
            SectionHeader(
                Voice.trayHeader(vocabulary.deviceNames[deviceId] ?: Voice.Copy.ELLIPSIS),
                modifier = Modifier.padding(horizontal = 16.dp),
            )
            OptionGroup {
                // S-3: the binding, read-only (recorded on the voice device's IDE).
                KeyBindingRow(
                    label = Voice.Copy.PUSH_TO_TALK,
                    value = settings.pttShortcut,
                    modifier = Modifier.testTag(Voice.TestIds.SHORTCUT),
                )
                GroupDivider()
                val wakeOn = armed && settings.wakeWord
                val wakeDetail = Voice.wakeRowDetail(
                    vocabulary.doctors[deviceId],
                    Voice.WakeDetailContext(
                        wakeWord = wakeOn,
                        agent = settings.agentId != null,
                        online = deviceId in vocabulary.online,
                        device = vocabulary.deviceNames[deviceId] ?: Voice.Copy.ELLIPSIS,
                        state = null,
                    ),
                )
                SwitchRow(
                    title = Voice.Copy.WAKE_WORD,
                    checked = wakeOn,
                    onCheckedChange = { onChange(Voice.Change(wakeWord = it)) },
                    enabled = armed,
                    description = wakeDetail,
                    modifier = Modifier.testTag(Voice.TestIds.WAKE_WORD),
                )
                GroupDivider()
                // The input row (label + trailing field, the board form's
                // Prefix row): saved a beat after typing stops, on Done or on leaving it.
                TextFieldRow(
                    label = Voice.Copy.WAKE_PHRASE,
                    value = wakePhrase,
                    onValueChange = onWakePhraseChange,
                    placeholder = Voice.wakePhrasePlaceholder(vocabulary.agentNames[settings.agentId] ?: Voice.Copy.AGENT),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { onWakePhraseDone() }),
                    onFocusLost = onWakePhraseDone,
                    modifier = Modifier.testTag(Voice.TestIds.WAKE_PHRASE),
                )
            }
            // #NEW-56: invalid input, inline under the field (never a toast).
            FieldError(wakePhraseError, modifier = Modifier.padding(start = 20.dp, top = 6.dp))
            // NEW-57: the meeting rows, their own group with no header, right under the tray group.
            Spacer(Modifier.height(12.dp))
            MeetingRows(
                settings = settings,
                armed = armed,
                doctor = vocabulary.desktopAudio[deviceId] ?: DesktopAudioDoctor(),
                agentName = vocabulary.agentNames[settings.agentId] ?: Voice.Copy.AGENT,
                onChange = onChange,
                onMeeting = onMeeting,
            )
        }
    }
}

/**
 * NEW-57 (`meeting.json`): Meeting mode, Desktop audio and (while it picks
 * or leaves out apps, and the device can pick apps) Apps: their own group,
 * no header, directly under the tray group. Turning
 * Meeting mode on only opens THE consent prompt (the doctor's Import
 * confirm, [GlassAlert]); Start = `voice.startMeeting`, off = `voice.stopMeeting`.
 */
@Composable
private fun MeetingRows(
    settings: Voice.Settings,
    armed: Boolean,
    doctor: DesktopAudioDoctor,
    agentName: String,
    onChange: (Voice.Change) -> Unit,
    onMeeting: (Boolean) -> Unit,
) {
    var consent by remember { mutableStateOf(false) }
    val desktopAudio = Meeting.DesktopAudio.of(settings.desktopAudio)
    val who = Meeting.who(desktopAudio, settings.desktopAudioApps, doctor.capability)
    val meetingOn = armed && settings.meetingStartedAt != null
    val audioOptions = Meeting.desktopAudioOptions(doctor.capability)
    // Apps only while the stored value is one the device offers AND picks apps.
    val showApps = desktopAudio in audioOptions &&
        (desktopAudio == Meeting.DesktopAudio.Include || desktopAudio == Meeting.DesktopAudio.Exclude)
    OptionGroup {
        SwitchRow(
            title = Meeting.Copy.MEETING,
            checked = meetingOn,
            onCheckedChange = { on -> if (on) consent = true else onMeeting(false) },
            enabled = armed,
            description = if (meetingOn) who else null,
            modifier = Modifier.testTag(Meeting.TestIds.MEETING),
        )
        GroupDivider()
        // #FEED-163: Stop after, right under the toggle, shown whether or not a meeting runs.
        val autoStopHours = Meeting.autoStopHours(settings.meetingAutoStopHours)
        OptionPickerRow(
            label = Meeting.Copy.AUTO_STOP_AFTER,
            options = Meeting.autoStopOptions().map { Voice.Option(id = it.value.toString(), label = it.label) },
            selected = autoStopHours.toString(),
            noneLabel = Meeting.autoStopOption(autoStopHours),
            onSelect = { picked -> picked?.toIntOrNull()?.let { onChange(Voice.Change(meetingAutoStopHours = it)) } },
            testTag = Meeting.TestIds.AUTO_STOP,
            enabled = armed,
        )
        GroupDivider()
        OptionPickerRow(
            label = Meeting.Copy.DESKTOP_AUDIO,
            options = audioOptions.map { Voice.Option(id = it.wire, label = it.label) },
            selected = settings.desktopAudio,
            noneLabel = desktopAudio.label,
            onSelect = { picked -> picked?.let { onChange(Voice.Change(desktopAudio = it)) } },
            testTag = Meeting.TestIds.DESKTOP_AUDIO,
        )
        if (showApps) {
            GroupDivider()
            val stored = settings.desktopAudioApps
            val options = remember(doctor.apps, stored) { Meeting.appOptions(doctor.apps, stored) }
            Picker(
                items = options.map { PickerItem(value = it.value, label = it.label) },
                mode = PickerMode.Multi,
                value = stored.map { it.id }.toSet(),
                onChange = { picked ->
                    // The stored order stays; newly picked apps follow in the options' order.
                    val kept = stored.filter { it.id in picked }
                    val added = options
                        .filter { it.value in picked && kept.none { app -> app.id == it.value } }
                        .map { AudioApp(id = it.value, name = it.label.take(Meeting.Limits.APP_NAME)) }
                    onChange(Voice.Change(desktopAudioApps = (kept + added).take(Meeting.Limits.APPS)))
                },
                title = Meeting.Copy.APPS,
            ) { open ->
                PickerValueRow(
                    label = Meeting.Copy.APPS,
                    value = stored.joinToString(", ") { it.name }.ifEmpty { Meeting.Copy.APPS_NONE },
                    onClick = open,
                    modifier = Modifier.testTag(Meeting.TestIds.APPS),
                )
            }
        }
    }
    if (consent) {
        val dismiss = { consent = false }
        GlassAlert(
            title = Meeting.Copy.CONSENT_TITLE,
            body = Meeting.consentBody(who, agentName),
            onDismiss = dismiss,
            trailing = listOf(
                GlassAlertAction(Meeting.Copy.CANCEL, onClick = dismiss),
                GlassAlertAction(
                    Meeting.Copy.START,
                    primary = true,
                    testTag = Meeting.TestIds.CONSENT_START,
                    onClick = {
                        consent = false
                        onMeeting(true)
                    },
                ),
            ),
            defaultAction = 0,
        )
    }
}

/**
 * S-3: the Push to talk row on a phone: the binding's label as keycaps
 * (`shortcutLabel`), `None` while no key is bound; no picker (keys are
 * recorded on the voice device).
 */
@Composable
private fun KeyBindingRow(label: String, value: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) {}
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        ShortcutKeys(value)
    }
}

/**
 * THE picker row (label left, value + chevron right) over [Voice.Option]s,
 * the shared `Picker` sheet behind it; a null id (None) rides as [NONE].
 */
@Composable
private fun OptionPickerRow(
    label: String,
    options: List<Voice.Option>,
    selected: String?,
    noneLabel: String,
    onSelect: (String?) -> Unit,
    testTag: String,
    /** The sheet's rows when they carry more than a label (the device glyphs). */
    items: List<PickerItem<String>> = remember(options) { options.map { PickerItem(value = it.id ?: NONE, label = it.label) } },
    /** #FEED-163: Stop after is disabled while the device or the agent is None. */
    enabled: Boolean = true,
) {
    val value = options.firstOrNull { it.id == selected }?.label ?: noneLabel
    Picker(
        items = items,
        mode = PickerMode.Single,
        value = setOf(selected ?: NONE),
        onChange = { picked -> picked.firstOrNull()?.let { onSelect(it.ifEmpty { null }) } },
        title = label,
    ) { open ->
        PickerValueRow(
            label = label,
            value = value,
            onClick = open,
            modifier = Modifier.testTag(testTag),
            enabled = enabled,
        )
    }
}

/**
 * #NEW-75: THE picker row over a choice that is never None (Language): the same picker row + `Picker` sheet as [OptionPickerRow], the
 * row's value = the picked choice's label, read through [choiceLabel] in the
 * row and in the sheet alike.
 */
@Composable
private fun ChoicePickerRow(
    label: String,
    choices: List<String>,
    selected: String,
    choiceLabel: (String) -> String,
    onSelect: (String) -> Unit,
    testTag: String,
) {
    Picker(
        items = choices.map { PickerItem(value = it, label = choiceLabel(it)) },
        mode = PickerMode.Single,
        value = setOf(selected),
        onChange = { picked -> picked.firstOrNull()?.let(onSelect) },
        title = label,
    ) { open ->
        PickerValueRow(
            label = label,
            value = choiceLabel(selected),
            onClick = open,
            modifier = Modifier.testTag(testTag),
        )
    }
}
