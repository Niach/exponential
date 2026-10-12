package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SearchList
import com.exponential.app.ui.components.picker.PickerDefaults
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerItemBody
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.PickerRow
import com.exponential.app.ui.components.picker.PickerRules
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import at.exponential.ui.primitives.EmptyStateSize

/**
 * SLOP-28 (EXP-1259 D4/D5): THE search list on Android
 * (`packages/domain-contract/fixtures/search-list.json`; the pure half is
 * `domain/SearchList.kt`). ONE component for every place that searches a list
 * and picks from it: the `#` issue typeahead's bottom sheet in every editor
 * and composer, the "+" menu's Implement issue › and Run action › pages, the
 * picker sheets' search and the Search screen. Anatomy: an INPUT row (40dp,
 * the nav-search glyph, the field, the muted result COUNT once a query is
 * typed) over the hairline, then the PICKER rows (no new row type) at the
 * phone height (44dp), at most eight visible, the rest scrolling under a
 * 24dp bottom fade. The top row is preselected: Enter (or Tab on a hardware
 * keyboard) picks it, ↑/↓ move it without animation.
 */
object SearchListDefaults {
    val InputHeight: Dp = SearchList.INPUT_HEIGHT.dp
    val RowHeight: Dp = SearchList.ROW_HEIGHT_PHONE.dp
    val FadeHeight: Dp = SearchList.FADE_HEIGHT.dp

    /** The list's cap: [SearchList.MAX_VISIBLE_ROWS] rows (+ the rows' 1dp gaps). */
    val MaxListHeight: Dp = (SearchList.ROW_HEIGHT_PHONE + 2).dp * SearchList.MAX_VISIBLE_ROWS
}

/**
 * The search list's INPUT row. [autoFocus] focuses it (and raises the
 * keyboard) on mount: the `#` sheet and the pushed pages open ready to type.
 * [onBackspaceEmpty] = Backspace in an EMPTY input (the `#` sheet closes and
 * deletes the `#`); [onSubmit] = Enter / Tab; [onMove] = ↑ (-1) / ↓ (+1).
 */
@Composable
fun SearchListInput(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    count: Int? = null,
    autoFocus: Boolean = false,
    onBackspaceEmpty: (() -> Unit)? = null,
    onSubmit: (() -> Unit)? = null,
    onMove: ((Int) -> Unit)? = null,
) {
    // The field keeps its own caret: a prefilled token opens with the caret
    // AFTER it, ready to keep typing.
    var field by remember { mutableStateOf(TextFieldValue(value, TextRange(value.length))) }
    LaunchedEffect(value) {
        if (field.text != value) field = TextFieldValue(value, TextRange(value.length))
    }
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    var focused by remember { mutableStateOf(false) }
    if (autoFocus) {
        LaunchedEffect(Unit) {
            // A sheet's content is placed a frame or two after it composes;
            // requestFocus on an unplaced node no-ops, so retry until it lands.
            var attempts = 0
            while (attempts < 10 && !focused) {
                runCatching { focusRequester.requestFocus() }
                withFrameNanos { }
                attempts++
            }
            keyboard?.show()
        }
    }
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(SearchListDefaults.InputHeight)
                .padding(horizontal = GlassSheetDefaults.HorizontalPadding),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                ExpIcons.navSearch,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = muted,
            )
            Spacer(Modifier.width(10.dp))
            Box(modifier = Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                if (field.text.isEmpty()) {
                    Text(placeholder, style = MaterialTheme.typography.bodyLarge, color = muted, maxLines = 1)
                }
                BasicTextField(
                    value = field,
                    onValueChange = { next ->
                        field = next
                        if (next.text != value) onValueChange(next.text)
                    },
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
                    cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
                    keyboardActions = KeyboardActions(onGo = { onSubmit?.invoke() }),
                    modifier = Modifier
                        .fillMaxWidth()
                        .focusRequester(focusRequester)
                        .onFocusChanged { focused = it.isFocused }
                        .onPreviewKeyEvent { event ->
                            if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                            when (event.key) {
                                Key.Backspace -> if (field.text.isEmpty() && onBackspaceEmpty != null) {
                                    onBackspaceEmpty()
                                    true
                                } else {
                                    false
                                }
                                Key.DirectionUp -> onMove?.let { it(-1); true } ?: false
                                Key.DirectionDown -> onMove?.let { it(1); true } ?: false
                                Key.Enter, Key.NumPadEnter, Key.Tab -> onSubmit?.let { it(); true } ?: false
                                else -> false
                            }
                        }
                        .testTag(SearchList.INPUT_TEST_ID),
                )
            }
            if (count != null && value.isNotEmpty()) {
                Spacer(Modifier.width(8.dp))
                Text(
                    count.toString(),
                    style = MaterialTheme.typography.labelMedium,
                    color = muted,
                    modifier = Modifier.testTag(SearchList.COUNT_TEST_ID),
                )
            }
        }
        HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
    }
}

/**
 * The search list's ROWS: the picker's own rows ([PickerRow]) at the search
 * list's one height, capped at eight visible with the bottom fade while more
 * sit below. [highlighted] is the keyboard row (the top one by default);
 * [onPick] gets the tapped (or Enter-picked) row.
 */
@Composable
fun <T : Any> SearchListRows(
    items: List<PickerItem<T>>,
    onPick: (PickerItem<T>) -> Unit,
    modifier: Modifier = Modifier,
    mode: PickerMode = PickerMode.Single,
    value: Set<T> = emptySet(),
    highlighted: Int = 0,
    emptyText: String? = null,
    /** Fill the space the host leaves instead of capping at eight rows (a whole screen). */
    capHeight: Boolean = true,
    /** The rows are still loading: skeleton rows in their geometry (EXP-1275). */
    loading: Boolean = false,
    fadeColor: Color = GlassSheetDefaults.ContainerColor,
    renderItem: (@Composable RowScope.(item: PickerItem<T>) -> Unit)? = null,
) {
    val state = rememberLazyListState()
    LaunchedEffect(highlighted) {
        if (highlighted in items.indices) state.scrollToItem(highlighted.coerceAtLeast(0))
    }
    Box(modifier = modifier.fillMaxWidth().testTag(SearchList.LIST_TEST_ID)) {
        LazyColumn(
            state = state,
            modifier = Modifier
                .fillMaxWidth()
                .then(if (capHeight) Modifier.heightIn(max = SearchListDefaults.MaxListHeight) else Modifier),
            contentPadding = PaddingValues(vertical = 4.dp),
        ) {
            if (items.isEmpty() && loading) {
                item { SkeletonRows(rowHeight = SearchListDefaults.RowHeight) }
            } else if (items.isEmpty() && emptyText != null) {
                item { EmptyState(title = emptyText, size = EmptyStateSize.List) }
            }
            itemsIndexed(items, key = { _, item -> item.value }) { index, item ->
                Box(Modifier.testTag(SearchList.ROW_TEST_ID)) {
                    PickerRow(
                        item = item,
                        mark = PickerDefaults.mark(mode, PickerRules.checked(item, value)),
                        onClick = { if (!item.disabled) onPick(item) },
                        body = renderItem ?: { PickerItemBody(it) },
                        // The phone row height (44) IS the picker row's.
                        highlighted = index == highlighted,
                    )
                }
            }
        }
        if (state.canScrollForward) {
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .height(SearchListDefaults.FadeHeight)
                    .background(Brush.verticalGradient(listOf(Color.Transparent, fadeColor))),
            )
        }
    }
}

/**
 * The input + rows together, single-pick, with the keyboard row and Enter
 * wired: the `#` sheet and the "+" menu's pushed pages.
 */
@Composable
fun <T : Any> SearchListPanel(
    query: String,
    onQueryChange: (String) -> Unit,
    placeholder: String,
    items: List<PickerItem<T>>,
    onPick: (PickerItem<T>) -> Unit,
    modifier: Modifier = Modifier,
    mode: PickerMode = PickerMode.Single,
    value: Set<T> = emptySet(),
    emptyText: String? = null,
    loading: Boolean = false,
    autoFocus: Boolean = true,
    onBackspaceEmpty: (() -> Unit)? = null,
    renderItem: (@Composable RowScope.(item: PickerItem<T>) -> Unit)? = null,
) {
    var highlighted by remember { mutableIntStateOf(0) }
    LaunchedEffect(items) { highlighted = 0 }
    Column(modifier = modifier.fillMaxWidth()) {
        SearchListInput(
            value = query,
            onValueChange = onQueryChange,
            placeholder = placeholder,
            count = items.size,
            autoFocus = autoFocus,
            onBackspaceEmpty = onBackspaceEmpty,
            onSubmit = { items.getOrNull(highlighted)?.let(onPick) },
            onMove = { delta ->
                if (items.isNotEmpty()) highlighted = (highlighted + delta + items.size) % items.size
            },
        )
        SearchListRows(
            items = items,
            onPick = onPick,
            mode = mode,
            value = value,
            highlighted = highlighted,
            emptyText = emptyText,
            loading = loading,
            renderItem = renderItem,
        )
    }
}
