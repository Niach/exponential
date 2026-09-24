package com.exponential.app.ui.components.picker

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1029 contract, EXP-1021 implementation — THE picker primitive on
 * Android. One primitive per platform, typed pickers on top, the same names
 * everywhere: web `packages/ui/src/picker`, IDE `ui::picker`, iOS
 * `ExpUI/Sources/Picker` (`GlassPicker`).
 *
 * Presentation belongs to the primitive, never to the caller: on the phone
 * every picker is a bottom sheet ([com.exponential.app.ui.components.GlassSheet])
 * of PLAIN rows — no cards inside the sheet; multi-select marks rows by the
 * highlight colour, no circles; swipe down closes (the drag handle is the
 * sheet's only dismiss affordance, EXP-687). [search] adds the filter field at
 * the top. The trigger is whatever chip or button the caller hands in; the
 * primitive owns the sheet.
 *
 * That selection language IS what EXP-1021 asked for: the picker that links a
 * relation and the picker that batches issues finally look like one thing, so
 * the highlight lives HERE and no caller can invent a second "this is picked"
 * idiom.
 *
 * `PickerContractTest` carries the presentation rules and the typed-picker
 * gate, under the same case names web/IDE/iOS use. Android's unit suite is
 * plain JVM (no Robolectric), so everything a rule can be decided by lives in
 * [PickerRules] / [PickerDefaults] as pure values rather than inside the
 * composable.
 */

/** One row of a picker. */
data class PickerItem<T>(
    /** The stable identity of the row; also what search matches on. */
    val value: T,
    /** What the row reads as; also the default search keyword. */
    val label: String,
    /** A leading glyph — an `AppIcons` vector, never a raw material icon. */
    val icon: ImageVector? = null,
    /** A colour for the glyph (a board's hex, a label's dot, a status tone). */
    val color: Color? = null,
    /** A muted second line or trailing note (an email, a branch age). */
    val description: String? = null,
    /** Rendered, never pickable. */
    val disabled: Boolean = false,
    /** Extra search terms (an identifier, an email). */
    val keywords: List<String> = emptyList(),
    /**
     * Multi mode only: what THIS row reads as when membership in the picker's
     * `value` is not the whole story — a bulk edit over rows that disagree
     * marks a label [PickerChecked.All] on all of them, [PickerChecked.Some]
     * on only some. When set it WINS over membership; absent (the ordinary
     * case) the state is derived from the set.
     */
    val checked: PickerChecked? = null,
) {
    /** The keywords the row matches on: the explicit ones, else its label. */
    val searchKeywords: List<String> get() = keywords.ifEmpty { listOf(label) }
}

/**
 * What a row reads as, once tri-state exists: a bulk edit is the one caller
 * whose rows can DISAGREE, and the highlight has to say so without growing a
 * checkbox column (EXP-1021 — no circles, on any surface).
 */
enum class PickerChecked {
    /** Nothing: the row paints nothing at all. */
    None,

    /** Some of what this row covers: the active wash alone. */
    Some,

    /** All of it: the wash PLUS the inset active stroke. */
    All,
}

enum class PickerMode {
    /** Closes on a pick; `onChange` gets the one value. */
    Single,

    /** Toggles without closing; `onChange` gets the whole new set. */
    Multi,
}

/**
 * The picker's decisions, as pure functions — what a row matches, what a tap
 * does, and whether it closes the sheet. They are the primitive's only
 * behaviour, so the JVM contract test can pin them without a renderer.
 */
object PickerRules {

    /** A row matches when ANY of its keywords contains the query. */
    fun matches(item: PickerItem<*>, query: String): Boolean {
        val q = query.trim()
        if (q.isEmpty()) return true
        return item.searchKeywords.any { it.contains(q, ignoreCase = true) }
    }

    /** The rows a query leaves; a blank query leaves the caller's order. */
    fun <T> filter(items: List<PickerItem<T>>, query: String): List<PickerItem<T>> =
        if (query.isBlank()) items else items.filter { matches(it, query) }

    /**
     * What a row reads as: its own [PickerItem.checked] when it carries one
     * (the bulk edit's tri-state), else plain membership in [value].
     */
    fun <T> checked(item: PickerItem<T>, value: Set<T>): PickerChecked =
        item.checked ?: if (item.value in value) PickerChecked.All else PickerChecked.None

    /** Single picks are a decision and close the sheet; multi toggles stay. */
    fun closesOnPick(mode: PickerMode): Boolean = mode == PickerMode.Single

    /**
     * The whole new selection a tap produces, or null when the row cannot be
     * picked at all (disabled rows render but never pick). Single mode REPLACES
     * the set; multi toggles membership.
     */
    fun <T> select(mode: PickerMode, current: Set<T>, item: PickerItem<T>): Set<T>? {
        if (item.disabled) return null
        return when (mode) {
            PickerMode.Single -> setOf(item.value)
            PickerMode.Multi ->
                if (item.value in current) current - item.value else current + item.value
        }
    }
}

/**
 * The picker sheet's pinned chrome — the row metrics and the ONE paint a
 * picked row takes. iOS/desktop mirror these; `PickerContractTest` pins them
 * the way `GlassSheetDefaultsTest` pins the sheet shell.
 */
object PickerDefaults {
    /** The 44dp minimum touch target every sheet row already wears. */
    val RowHeight: Dp = 44.dp

    /** Rows line up with the sheet title's gutter (20dp = 8 outer + 12 inner). */
    val RowOuterPadding: Dp = 8.dp
    val RowInnerPadding: Dp = GlassSheetDefaults.HorizontalPadding - RowOuterPadding

    val IconSize: Dp = 18.dp

    /** A row with a colour but no glyph draws this dot — that IS a label row. */
    val DotSize: Dp = 10.dp

    /** The gap between the leading slot and the label. */
    val LeadingGap: Dp = 12.dp

    /** The row's corner — the app's list-row rung (`flatRow`'s own). */
    val RowRadius: Dp = GlassTokens.RowRadius

    /**
     * THE selection mark: a picked row takes the app's active row wash and
     * NOTHING else — no leading circle, no trailing check (EXP-1021). An
     * unpicked row paints nothing at all, which is what makes the list read as
     * plain rows on the sheet instead of a stack of cards.
     *
     * The wash IS what `Modifier.flatRow(active)` (the EXP-818 list row) lays
     * down; named here so the contract test can pin the paint without a
     * renderer.
     */
    fun rowBackground(checked: PickerChecked): Color =
        if (checked == PickerChecked.None) Color.Transparent else GlassTokens.RowFillActive

    /**
     * The second half of the mark, and the ONLY thing that tells a FULL row
     * from a partial one: the inset active stroke (web's
     * `ring-glass-stroke-active`). A [PickerChecked.Some] row wears the wash
     * alone — "some of these, not all", said in paint rather than in a dash
     * glyph.
     */
    fun rowStroke(checked: PickerChecked): Color =
        if (checked == PickerChecked.All) GlassTokens.StrokeActive else Color.Transparent

    fun labelColor(enabled: Boolean): Color =
        Color.White.copy(alpha = if (enabled) 0.9f else TextEmphasis.Quaternary)
}

/**
 * THE picker: [trigger] renders the caller's chip or button and receives the
 * `open` callback to wire to its click; the sheet is the primitive's.
 *
 * [open] / [onOpenChange] make the sheet CONTROLLED, for the callers whose
 * picker is a state machine rather than a chip — the issue screens open their
 * sheets from a properties sheet that has already closed (two stacked bottom
 * sheets is a dead end on Android), so there is no trigger in composition to
 * open them. Uncontrolled ([open] null) is the ordinary case.
 */
@Composable
fun <T : Any> Picker(
    // The value IS the row's identity (the LazyColumn keys on it), so it can
    // never be null.
    items: List<PickerItem<T>>,
    mode: PickerMode,
    /** The current selection (at most one value in [PickerMode.Single]). */
    value: Set<T>,
    /** The whole new selection (one value in single mode). */
    onChange: (Set<T>) -> Unit,
    search: Boolean = false,
    emptyText: String? = null,
    /** The sheet's title. */
    title: String? = null,
    enabled: Boolean = true,
    /** The filter field's placeholder. */
    searchPlaceholder: String = "Search",
    /** Controlled open state; null = the primitive owns it. */
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    /** Controlled filter text — for a caller that ranks with its own engine. */
    query: String? = null,
    onQueryChange: ((String) -> Unit)? = null,
    /**
     * False when [query] is ranked by the caller (EXP-892's engine) and the
     * rows arrive already filtered: the primitive then renders them verbatim.
     */
    filter: Boolean = true,
    /** Rendered under the rows — a "Create label" action row. */
    footer: (@Composable () -> Unit)? = null,
    /**
     * REPLACES the search field and the rows with an inline body (the icon
     * picker's swatch grid). The sheet, the trigger and the PICK stay the
     * primitive's: the body reports the value it was given through the `pick`
     * it receives, and the primitive folds it into the selection and closes
     * the sheet exactly as a row does. A panel can no more invent its own pick
     * than a row can invent its own highlight. [footer] still renders under it.
     */
    panel: (@Composable (pick: (T) -> Unit) -> Unit)? = null,
    /**
     * Replaces the row BODY, never its highlight or its click — so a custom
     * row can not invent a second "this is picked" idiom. The one caller is
     * the assignee picker's avatar (a picker glyph is an icon, never a photo).
     */
    renderItem: (@Composable RowScope.(item: PickerItem<T>) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    var internalOpen by remember { mutableStateOf(false) }
    var internalQuery by remember { mutableStateOf("") }
    val isOpen = open ?: internalOpen
    val setOpen: (Boolean) -> Unit = { next ->
        if (open == null) internalOpen = next
        // The query is the sheet's, not the picker's: a reopened sheet starts
        // on the full list rather than on whatever was typed last time.
        if (!next && query == null) internalQuery = ""
        onOpenChange?.invoke(next)
    }

    trigger { if (enabled) setOpen(true) }

    if (!isOpen) return

    val currentQuery = query ?: internalQuery
    val rows = remember(items, currentQuery, search, filter) {
        if (search && filter) PickerRules.filter(items, currentQuery) else items
    }

    GlassSheet(title = title, onDismiss = { setOpen(false) }) {
        if (panel != null) {
            // The panel is the ONE child here that may GROW, and only into
            // what the footer leaves: `GlassSheet` caps its column (85 % of
            // the screen on a fitted sheet), and an unweighted scrolling panel
            // is measured first against that whole cap — the footer beside it
            // then measures at maxHeight 0 and is simply not there. That is
            // how the icon picker's "No icon" reset vanished on every phone
            // (the board grid is taller than the cap on all of them).
            // `fill = false` so a SHORT panel still only takes what it needs.
            Box(modifier = Modifier.weight(1f, fill = false)) {
                panel { picked ->
                    // The panel hands back a VALUE; the rule that turns it into
                    // a selection is the same one a row goes through.
                    val next = PickerRules.select(mode, value, PickerItem(value = picked, label = ""))
                    if (next != null) {
                        onChange(next)
                        if (PickerRules.closesOnPick(mode)) setOpen(false)
                    }
                }
            }
            // A panel replaces the ROWS, never the footer: the icon picker's
            // "No icon" reset sits under its grid the way "Create label" sits
            // under the label rows.
            footer?.invoke()
            Spacer(Modifier.height(8.dp))
            return@GlassSheet
        }
        if (search) {
            GlassSheetSearchField(
                value = currentQuery,
                onValueChange = { next ->
                    if (query == null) internalQuery = next
                    onQueryChange?.invoke(next)
                },
                placeholder = searchPlaceholder,
            )
            Spacer(Modifier.height(4.dp))
        }
        LazyColumn(modifier = Modifier.fillMaxWidth()) {
            if (rows.isEmpty() && emptyText != null) {
                // The same text stands in for "nothing to pick" and "nothing
                // matched" — one empty state, like every other client.
                item {
                    Text(
                        emptyText,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(
                            horizontal = GlassSheetDefaults.HorizontalPadding,
                            vertical = 16.dp,
                        ),
                    )
                }
            }
            items(rows, key = { it.value }) { item ->
                PickerRow(
                    item = item,
                    checked = PickerRules.checked(item, value),
                    onClick = {
                        val next = PickerRules.select(mode, value, item) ?: return@PickerRow
                        onChange(next)
                        if (PickerRules.closesOnPick(mode)) setOpen(false)
                    },
                    body = renderItem,
                )
            }
            if (footer != null) item { footer() }
            item { Spacer(Modifier.height(8.dp)) }
        }
    }
}

/**
 * ONE picker row: the body over the row's own highlight. Plain on purpose — a
 * picker row is never a card, on any surface, and the ONLY mark a row carries
 * is [PickerDefaults.rowBackground] plus, when it is FULL rather than partial,
 * [PickerDefaults.rowStroke].
 */
@Composable
private fun <T> PickerRow(
    item: PickerItem<T>,
    checked: PickerChecked,
    onClick: () -> Unit,
    body: (@Composable RowScope.(PickerItem<T>) -> Unit)? = null,
) {
    val shape = RoundedCornerShape(PickerDefaults.RowRadius)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PickerDefaults.RowOuterPadding, vertical = 1.dp)
            // The 44dp minimum is the HIGHLIGHT's height, so it goes above the
            // inner padding (`GlassSheetRow`'s own order): below it the padding
            // is added ON TOP of the minimum and the row stands ~18dp taller
            // than the sheet rows beside it.
            .defaultMinSize(minHeight = PickerDefaults.RowHeight)
            .clip(shape)
            .background(PickerDefaults.rowBackground(checked), shape)
            .border(GlassTokens.Hairline, PickerDefaults.rowStroke(checked), shape)
            .clickable(enabled = !item.disabled, onClick = onClick)
            .padding(horizontal = PickerDefaults.RowInnerPadding, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (body != null) body(item) else PickerItemBody(item)
    }
}

/**
 * A picker FOOTER row — the primitive's own row idiom for the one thing in a
 * picker sheet that is NOT an option: "Create new label “x”", "No icon". It
 * takes the rows' geometry so it lines up with them, and it never takes the
 * highlight, because it picks nothing. Callers hand it to [Picker]'s `footer`
 * instead of reaching for a sheet row from another surface — a second row
 * idiom inside the picker sheet is exactly what EXP-1021 removed.
 */
@Composable
fun PickerActionRow(
    label: String,
    onClick: () -> Unit,
    icon: ImageVector? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PickerDefaults.RowOuterPadding, vertical = 1.dp)
            .defaultMinSize(minHeight = PickerDefaults.RowHeight)
            .clip(RoundedCornerShape(PickerDefaults.RowRadius))
            .clickable(onClick = onClick)
            .padding(horizontal = PickerDefaults.RowInnerPadding, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(
                icon,
                contentDescription = null,
                modifier = Modifier.size(PickerDefaults.IconSize),
                tint = Color.White.copy(alpha = TextEmphasis.Secondary),
            )
            Spacer(Modifier.width(PickerDefaults.LeadingGap))
        }
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            // An action reads muted beside the options it sits under.
            color = Color.White.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * THE row body: the dot or the tinted glyph, the label, the muted second line.
 * Public so a [Picker] `renderItem` can KEEP it and only prepend to it (the
 * assignee avatar), instead of redrawing a row of its own.
 */
@Composable
fun <T> RowScope.PickerItemBody(item: PickerItem<T>) {
    val glyph = item.icon
    if (glyph != null) {
        Icon(
            glyph,
            contentDescription = null,
            modifier = Modifier.size(PickerDefaults.IconSize),
            tint = item.color ?: Color.White.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(PickerDefaults.LeadingGap))
    } else if (item.color != null) {
        // A colour without a glyph draws the row's DOT — that is how a label
        // row reads, without the caller ever choosing a shape.
        Box(
            modifier = Modifier.size(PickerDefaults.IconSize),
            contentAlignment = Alignment.Center,
        ) {
            Box(Modifier.size(PickerDefaults.DotSize).background(item.color, CircleShape))
        }
        Spacer(Modifier.width(PickerDefaults.LeadingGap))
    }
    Column(modifier = Modifier.weight(1f)) {
        Text(
            item.label,
            style = MaterialTheme.typography.bodyMedium,
            color = PickerDefaults.labelColor(enabled = !item.disabled),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (item.description != null) {
            Text(
                item.description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
