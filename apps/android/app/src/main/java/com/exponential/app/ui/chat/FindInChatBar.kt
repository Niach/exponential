package com.exponential.app.ui.chat

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Chats
import com.exponential.app.domain.FindInChat
import com.exponential.app.domain.SearchList
import com.exponential.app.ui.components.HeaderEdgeChrome
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.delay

/**
 * #EDE-130 (fixtures/find-in-chat.json): find in chat on Android. The chat
 * screen owns the bar ([FindInChatState]: open, the field, the steps it asks
 * for, the counter and status line the transcript reports back); the root's
 * transcript (`ActivityFeed`, [LocalFindInChat]) counts its rows, merges the
 * older hits and lands on the current stop.
 */
@Stable
class FindInChatState internal constructor(private val chatKey: String) {
    var open by mutableStateOf(false)
        private set

    /** The field's text. */
    var raw by mutableStateOf("")
        private set

    /** The query the transcript searches (collapsed, debounced; "" = none). */
    var query by mutableStateOf("")
        internal set

    /** The last step the bar asked for ([StepRequest.n] bumps per press). */
    var step by mutableStateOf<StepRequest?>(null)
        private set

    /** The transcript's report: the counter ("" = none) and the status line. */
    var counter by mutableStateOf("")
        private set
    var status by mutableStateOf<String?>(null)
        private set

    /** Bumps whenever the bar (re)opens: the field remounts focused, its text selected. */
    var openNonce by mutableIntStateOf(0)
        private set

    data class StepRequest(val step: FindInChat.Step, val n: Int)

    /**
     * #EDE-99 (search-list.json A PICK): the global search's start stop,
     * re-resolved as answers arrive until found or the store's answer is in;
     * dropped at the first step or edit (and on close).
     */
    var target by mutableStateOf<SearchList.ChatTarget?>(null)
        private set

    /** #EDE-99: the bar opened by a global search pick on [raw], starting at [target]. */
    fun seed(raw: String, target: SearchList.ChatTarget) {
        LAST_QUERY[chatKey] = raw
        this.raw = raw
        query = FindInChat.query(raw)
        step = null
        this.target = target
        open = true
        openNonce += 1
    }

    /** The target resolved (found, or fell back to the newest stop). */
    fun dropTarget() {
        target = null
    }

    fun openBar() {
        if (!open) raw = LAST_QUERY[chatKey].orEmpty()
        open = true
        openNonce += 1
    }

    /** Escape, back: clears every mark, keeps the scroll and the opened groups. */
    fun close() {
        open = false
        query = ""
        step = null
        target = null
        counter = ""
        status = null
    }

    fun updateRaw(value: String) {
        LAST_QUERY[chatKey] = value
        if (value != raw) target = null
        raw = value
    }

    fun go(step: FindInChat.Step) {
        target = null
        this.step = StepRequest(step, (this.step?.n ?: 0) + 1)
    }

    fun report(counter: String, status: String?) {
        if (this.counter != counter) this.counter = counter
        if (this.status != status) this.status = status
    }

    /** The query the transcript reads (none while closed). */
    val activeQuery: String get() = if (open) query else ""

    internal companion object {
        /** The last query per chat, for the process (memory only). */
        val LAST_QUERY = HashMap<String, String>()
    }
}

/**
 * #EDE-99 (search-list.json A PICK): a global search pick waiting for its
 * session's screen (the chat for a root, the session view for a child),
 * which takes it on show and seeds its bar ([FindInChatState.seed]).
 */
object FindInChatSeeds {
    private val pending = HashMap<String, SearchList.ChatPick>()

    @Synchronized
    fun put(pick: SearchList.ChatPick) {
        pending.clear()
        pending[pick.sessionId] = pick
    }

    @Synchronized
    fun take(sessionId: String): SearchList.ChatPick? = pending.remove(sessionId)
}

/** The chat's find, for the ROOT's transcript under it (null = none). */
val LocalFindInChat = staticCompositionLocalOf<FindInChatState?> { null }

/** One [FindInChatState] per chat; typing re-searches after `debounceMs`. */
@Composable
fun rememberFindInChat(chatKey: String): FindInChatState {
    val state = remember(chatKey) { FindInChatState(chatKey) }
    val raw = state.raw
    val open = state.open
    LaunchedEffect(state, raw, open) {
        if (!open) return@LaunchedEffect
        val next = FindInChat.query(raw)
        if (next.isEmpty()) {
            state.query = ""
            return@LaunchedEffect
        }
        delay(FindInChat.DEBOUNCE_MS)
        state.query = next
    }
    return state
}

/**
 * COMPACT (find-in-chat.json WHERE THE BAR SITS, phones): the bar REPLACES
 * the chat header while open: back (closes), THE search field, the counter
 * (`counterCompact`, muted), previous (chevron up), next (chevron down); the
 * status line under it. Existing components only: the header's ghost
 * buttons, the BORDERLESS glass field (iOS `GlassTextField(bordered: false)`:
 * no outline, glyph or clear button), muted Text. Enter = previous (older).
 */
@Composable
fun FindInChatHeader(state: FindInChatState, modifier: Modifier = Modifier) {
    BackHandler(onBack = state::close)
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    HeaderEdgeChrome {
        Column(modifier = modifier.fillMaxWidth().statusBarsPadding().testTag(FindInChat.TestIds.BAR)) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = Chats.Shell.ChatHeader.PADDING_X.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ChatHeaderButton(
                    ExpIcons.uiBack,
                    FindInChat.Copy.CLOSE,
                    onClick = state::close,
                    modifier = Modifier.testTag(FindInChat.TestIds.CLOSE),
                )
                androidx.compose.runtime.key(state.openNonce) {
                    FindInChatField(state, Modifier.weight(1f))
                }
                if (state.counter.isNotEmpty()) {
                    Text(
                        state.counter,
                        style = MaterialTheme.typography.labelMedium.copy(fontFeatureSettings = "tnum"),
                        color = muted,
                        maxLines = 1,
                        modifier = Modifier.testTag(FindInChat.TestIds.COUNTER),
                    )
                    Spacer(Modifier.width(Chats.Shell.ChatHeader.GAP.dp))
                }
                ChatHeaderButton(
                    ExpIcons.uiChevronUp,
                    FindInChat.Copy.PREVIOUS,
                    onClick = { state.go(FindInChat.Step.Previous) },
                    modifier = Modifier.testTag(FindInChat.TestIds.PREVIOUS),
                )
                ChatHeaderButton(
                    ExpIcons.uiChevronDown,
                    FindInChat.Copy.NEXT,
                    onClick = { state.go(FindInChat.Step.Next) },
                    modifier = Modifier.testTag(FindInChat.TestIds.NEXT),
                )
            }
            val status = state.status
            if (status != null) {
                Text(
                    status,
                    style = MaterialTheme.typography.labelMedium,
                    color = muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier
                        .padding(horizontal = 16.dp)
                        .padding(bottom = 6.dp)
                        .testTag(FindInChat.TestIds.STATUS),
                )
            }
        }
    }
}

/**
 * THE find field: the borderless [GlassTextField] (the composer's), its
 * text opening SELECTED (a reopened bar shows the last query, typing
 * replaces it) and focused. Enter / the IME's search = previous (older).
 */
@Composable
private fun FindInChatField(state: FindInChatState, modifier: Modifier = Modifier) {
    var field by remember { mutableStateOf(TextFieldValue(state.raw, TextRange(0, state.raw.length))) }
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    var focused by remember { mutableStateOf(false) }
    // #EDE-99: a seeded bar (a global search pick) keeps the keyboard down:
    // the landed row stays in view.
    val autoFocus = remember { state.target == null }
    LaunchedEffect(Unit) {
        if (!autoFocus) return@LaunchedEffect
        // The bar is placed a frame or two after it composes; requestFocus on
        // an unplaced node no-ops, so retry until it lands.
        var attempts = 0
        while (attempts < 10 && !focused) {
            runCatching { focusRequester.requestFocus() }
            withFrameNanos { }
            attempts++
        }
        keyboard?.show()
    }
    GlassTextField(
        value = field,
        onValueChange = { next ->
            field = next
            if (next.text != state.raw) state.updateRaw(next.text)
        },
        modifier = modifier
            .focusRequester(focusRequester)
            .onFocusChanged { focused = it.isFocused }
            .onPreviewKeyEvent { event ->
                if (event.type == KeyEventType.KeyDown && (event.key == Key.Enter || event.key == Key.NumPadEnter)) {
                    state.go(FindInChat.Step.Previous)
                    true
                } else {
                    false
                }
            }
            .testTag(FindInChat.TestIds.INPUT),
        placeholder = FindInChat.Copy.PLACEHOLDER,
        singleLine = true,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search, autoCorrectEnabled = false),
        keyboardActions = KeyboardActions(onSearch = { state.go(FindInChat.Step.Previous) }),
        textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
        containerColor = Color.Transparent,
        bordered = false,
    )
}
