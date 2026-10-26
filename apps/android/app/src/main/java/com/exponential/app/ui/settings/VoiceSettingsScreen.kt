package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
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
import com.exponential.app.domain.Voice
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * NEW-67 (voice plan v3, V1): Settings → Personal → Voice (`voice.json`, the
 * same page on web, the IDE and iOS; on the phones it hangs off the account
 * page, the phone's Personal settings). ONE group (Voice device, Agent, Speak
 * replies, Language; Voice file + Transcript while a voice device is picked,
 * #NEW-73), then while a voice device is picked the tray group under
 * `trayHeader` (Push to talk, Shortcut, Wake word, Wake phrase). Every switch
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

    /** The account the page edits (`server/{accountId}/voice`). */
    val accountId: String = savedStateHandle.get<String>("accountId").orEmpty()

    private val db = holder.database(forAccountId = accountId)

    /** null until `voice.get` landed. */
    var settings by mutableStateOf<Voice.Settings?>(null)
        private set
    var loadFailed by mutableStateOf(false)
        private set

    /** The wake phrase as typed; saved a beat after typing stops. */
    var wakePhrase by mutableStateOf("")
        private set
    private var wakePhraseSave: Job? = null

    /** #NEW-73: the voice file and its transcript as typed, saved the same way. */
    var voiceFile by mutableStateOf("")
        private set
    private var voiceFileSave: Job? = null
    var voiceTranscript by mutableStateOf("")
        private set
    private var voiceTranscriptSave: Job? = null

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
        val agentOptions: List<Voice.Option> = Voice.agentOptions(emptyList()),
        val deviceNames: Map<String, String> = emptyMap(),
        val agentNames: Map<String, String> = emptyMap(),
    )

    val vocabulary: StateFlow<Vocabulary> = combine(devices, agents, teams) { devices, agents, teams ->
        // The member's OWN devices; `voiceDeviceOptions` keeps the desktops.
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
        val teamNames = teams.associate { it.id to it.name }
        Vocabulary(
            deviceOptions = Voice.voiceDeviceOptions(
                devices.filter { it.userId == userId }
                    .map { Voice.DeviceLike(deviceId = it.deviceId, name = it.label, kind = it.kind) },
            ),
            agentOptions = Voice.agentOptions(
                agents.map { Voice.AgentLike(id = it.id, name = it.name, team = teamNames[it.teamId].orEmpty()) },
            ),
            deviceNames = devices.associate { it.deviceId to it.label },
            agentNames = agents.associate { it.id to it.name },
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
     * server's row on success and reverts + toasts on a refusal.
     */
    fun save(change: Voice.Change) {
        val previous = settings ?: return
        val next = Voice.clearOnNone(previous, change)
        if (next == previous) return
        settings = next
        viewModelScope.launch {
            try {
                val stored = voiceApi.update(accountId, change)
                if (settings == next) settings = stored
            } catch (e: Exception) {
                settings = previous
                wakePhrase = previous.wakePhrase
                voiceFile = previous.voiceFile
                voiceTranscript = previous.voiceTranscript
                failure = FailedAction("save", "the voice settings", trpcErrorMessage(e, ""))
            }
        }
    }

    fun onWakePhraseChange(text: String) {
        wakePhrase = text
        wakePhraseSave?.cancel()
        wakePhraseSave = viewModelScope.launch {
            delay(WAKE_PHRASE_DEBOUNCE_MS)
            saveWakePhrase()
        }
    }

    /** Done on the keyboard saves at once, like Return on iOS. */
    fun saveWakePhrase() {
        wakePhraseSave?.cancel()
        val trimmed = wakePhrase.trim()
        if (trimmed == settings?.wakePhrase) return
        save(Voice.Change(wakePhrase = trimmed))
    }

    /** One line, no control characters, at most `limits.voiceFile`. */
    fun onVoiceFileChange(text: String) {
        voiceFile = text.filterNot { it.isISOControl() }.take(Voice.Limits.VOICE_FILE)
        voiceFileSave?.cancel()
        voiceFileSave = viewModelScope.launch {
            delay(WAKE_PHRASE_DEBOUNCE_MS)
            saveVoiceFile()
        }
    }

    fun saveVoiceFile() {
        voiceFileSave?.cancel()
        val trimmed = voiceFile.trim()
        if (trimmed == settings?.voiceFile) return
        save(Voice.Change(voiceFile = trimmed))
    }

    /** One line, no control characters, at most `limits.voiceTranscript`. */
    fun onVoiceTranscriptChange(text: String) {
        voiceTranscript = text.filterNot { it.isISOControl() }.take(Voice.Limits.VOICE_TRANSCRIPT)
        voiceTranscriptSave?.cancel()
        voiceTranscriptSave = viewModelScope.launch {
            delay(WAKE_PHRASE_DEBOUNCE_MS)
            saveVoiceTranscript()
        }
    }

    fun saveVoiceTranscript() {
        voiceTranscriptSave?.cancel()
        val trimmed = voiceTranscript.trim()
        if (trimmed == settings?.voiceTranscript) return
        save(Voice.Change(voiceTranscript = trimmed))
    }

    private companion object {
        const val WAKE_PHRASE_DEBOUNCE_MS = 600L
    }
}

/** The pickers carry a String value; None rides as the empty string. */
private const val NONE = ""

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VoiceSettingsScreen(
    onBack: () -> Unit,
    viewModel: VoiceSettingsViewModel = hiltViewModel(),
) {
    val vocabulary by viewModel.vocabulary.collectAsStateWithLifecycle()

    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(Voice.Copy.GROUP) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
        modifier = Modifier.testTag(Voice.TestIds.PAGE),
    ) { padding ->
        val settings = viewModel.settings
        when {
            settings != null -> VoiceSettingsForm(
                settings = settings,
                vocabulary = vocabulary,
                wakePhrase = viewModel.wakePhrase,
                onChange = viewModel::save,
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

@Composable
private fun VoiceSettingsForm(
    settings: Voice.Settings,
    vocabulary: VoiceSettingsViewModel.Vocabulary,
    wakePhrase: String,
    onChange: (Voice.Change) -> Unit,
    onWakePhraseChange: (String) -> Unit,
    onWakePhraseDone: () -> Unit,
    voiceFile: String,
    onVoiceFileChange: (String) -> Unit,
    onVoiceFileDone: () -> Unit,
    voiceTranscript: String,
    onVoiceTranscriptChange: (String) -> Unit,
    onVoiceTranscriptDone: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // voice.json: every switch is off and disabled while either is None.
    val armed = settings.voiceDeviceId != null && settings.agentId != null

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(vertical = 8.dp),
    ) {
        OptionGroup {
            OptionPickerRow(
                label = Voice.Copy.VOICE_DEVICE,
                options = vocabulary.deviceOptions,
                selected = settings.voiceDeviceId,
                noneLabel = Voice.Copy.VOICE_DEVICE_NONE,
                onSelect = { onChange(Voice.Change(voiceDeviceId = Voice.Pick(it))) },
                testTag = Voice.TestIds.VOICE_DEVICE,
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
            SwitchRow(
                title = Voice.Copy.SPEAK_REPLIES,
                checked = armed && settings.speakReplies,
                onCheckedChange = { onChange(Voice.Change(speakReplies = it)) },
                enabled = armed,
                modifier = Modifier.testTag(Voice.TestIds.SPEAK_REPLIES),
            )
            GroupDivider()
            // Every language carries its GPU note (#NEW-74), in the row and in the menu.
            val languageDevice = settings.voiceDeviceId?.let { vocabulary.deviceNames[it] ?: it }
            OptionPickerRow(
                label = Voice.Copy.LANGUAGE,
                options = Voice.languages.map { Voice.Option(id = it.value, label = Voice.languageLabel(it.value, languageDevice)) },
                selected = settings.language,
                noneLabel = settings.language,
                onSelect = { picked -> picked?.let { onChange(Voice.Change(language = it)) } },
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
                    modifier = Modifier.testTag(Voice.TestIds.VOICE_TRANSCRIPT),
                )
            }
        }

        val deviceId = settings.voiceDeviceId
        if (deviceId != null) {
            Spacer(Modifier.height(12.dp))
            SectionHeader(
                Voice.trayHeader(vocabulary.deviceNames[deviceId] ?: deviceId),
                modifier = Modifier.padding(horizontal = 16.dp),
            )
            OptionGroup {
                SwitchRow(
                    title = Voice.Copy.PUSH_TO_TALK,
                    checked = armed && settings.pushToTalk,
                    onCheckedChange = { onChange(Voice.Change(pushToTalk = it)) },
                    enabled = armed,
                    modifier = Modifier.testTag(Voice.TestIds.PUSH_TO_TALK),
                )
                GroupDivider()
                OptionPickerRow(
                    label = Voice.Copy.SHORTCUT,
                    options = Voice.shortcuts.map { Voice.Option(id = it.value, label = it.label) },
                    selected = settings.pttShortcut,
                    noneLabel = settings.pttShortcut,
                    onSelect = { picked -> picked?.let { onChange(Voice.Change(pttShortcut = it)) } },
                    testTag = Voice.TestIds.SHORTCUT,
                )
                GroupDivider()
                SwitchRow(
                    title = Voice.Copy.WAKE_WORD,
                    checked = armed && settings.wakeWord,
                    onCheckedChange = { onChange(Voice.Change(wakeWord = it)) },
                    enabled = armed,
                    modifier = Modifier.testTag(Voice.TestIds.WAKE_WORD),
                )
                GroupDivider()
                // The input row (label + trailing field, the board form's
                // Prefix row): saved a beat after typing stops, or on Done.
                TextFieldRow(
                    label = Voice.Copy.WAKE_PHRASE,
                    value = wakePhrase,
                    onValueChange = onWakePhraseChange,
                    placeholder = Voice.wakePhrasePlaceholder(vocabulary.agentNames[settings.agentId].orEmpty()),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { onWakePhraseDone() }),
                    modifier = Modifier.testTag(Voice.TestIds.WAKE_PHRASE),
                )
            }
        }
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
) {
    val items = remember(options) { options.map { PickerItem(value = it.id ?: NONE, label = it.label) } }
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
        )
    }
}
