package at.exponential.ui.compose

import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.AbsoluteRoundedCornerShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import at.exponential.ui.catalog.CatalogConstants
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.keyboardDriven
import at.exponential.ui.model.selectPick
import at.exponential.ui.model.selectValue
import at.exponential.ui.theme.ResolvedTextStyle
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.roundToInt
import kotlin.math.sign

/** The core's anchored-overlay offset (eui-core `overlay::OVERLAY_OFFSET`) and viewport margin. */
private const val POPUP_OFFSET = 4f
private const val POPUP_MARGIN = 8f

/**
 * One shown Select option. #EXP-1765: a [mono] option's label is set in the
 * mono type and its [trailing] is its own muted sans run (null otherwise:
 * a plain option's trailing reads on inside [text]). #EXP-1629: a
 * [description] = the muted second line under its label.
 */
internal data class SelectOption(val value: JsonValue, val text: String, val icon: String?, val disabled: Boolean, val mono: Boolean = false, val trailing: String? = null, val description: String? = null)

/**
 * A Select's OBJECT options whose label contains [query] (lowercase; empty
 * = all); #EXP-1629: an option's `trailing` reads on after its label (the
 * core's item text), #EXP-1765: except on a `mono` option, where it stays
 * its own run. The search matches the label alone.
 */
internal fun selectOptions(owner: Props, query: String): List<SelectOption> = owner.list("options").mapNotNull { o ->
    if (o !is JsonValue.Obj) return@mapNotNull null
    val v = o["value"] ?: JsonValue.Null
    val label = o["label"]?.displayText ?: v.displayText
    if (query.isNotEmpty() && !label.lowercase().contains(query)) return@mapNotNull null
    val trailing = o["trailing"]?.displayText.orEmpty()
    val mono = o["mono"]?.bool == true
    val text = if (trailing.isEmpty() || mono) label else "$label $trailing"
    SelectOption(v, text, o["icon"]?.string, o["disabled"]?.bool == true, mono, trailing.takeIf { mono && it.isNotEmpty() }, o["description"]?.displayText?.takeIf { it.isNotEmpty() })
}

/** #EXP-1629: where the cursor starts: the first enabled option (a fresh pick, the IDE's picker); with [chosen] (a `multiple` Select, the IDE's multi picker) the first enabled chosen one; -1 = none. */
internal fun selectInitialCursor(options: List<SelectOption>, chosen: Collection<String>? = null): Int =
    if (chosen == null) options.indexOfFirst { !it.disabled } else options.indexOfFirst { !it.disabled && chosen.contains(it.value.displayText) }

/** #EXP-1629: the next enabled option from [from] by [step] (clamped at the ends; [from] when none). */
internal fun selectStep(options: List<SelectOption>, from: Int, step: Int): Int {
    var i = from + step
    while (i in options.indices) {
        if (!options[i].disabled) return i
        i += step
    }
    return from
}

/** #EXP-1629: the popup's alignment (`align`, else `start`: a row's under its value). */
internal fun selectPopupAlign(owner: Props): String = owner.str("align").ifEmpty { "start" }

/**
 * #EXP-1629: the popup's gap to its anchor and margin to the window's edges
 * (dp): a row's hangs off its value; #EXP-1765: a pill's border box
 * `selectPillPopupOffset` under it.
 */
internal fun selectPopupSpacing(row: Boolean, pill: Boolean = false, button: Boolean = false): Pair<Float, Float> = when {
    row -> CatalogConstants.SELECT_ROW_POPUP_OFFSET to CatalogConstants.SELECT_ROW_POPUP_MARGIN
    // #EXP-1778: a `button`'s (the IDE pointer menu) border box under the trigger.
    button -> CatalogConstants.SELECT_BUTTON_POPUP_OFFSET to CatalogConstants.SELECT_ROW_POPUP_MARGIN
    pill -> CatalogConstants.SELECT_PILL_POPUP_OFFSET to CatalogConstants.SELECT_ROW_POPUP_MARGIN
    else -> POPUP_OFFSET to POPUP_MARGIN
}

/** #EXP-1765: how far OUTSIDE the anchor's aligned edge the popup starts (dp): a pill's `selectPillPopupInset`, else 0. */
internal fun selectPopupInset(pill: Boolean, button: Boolean = false): Float = if (pill || button) CatalogConstants.SELECT_PILL_POPUP_INSET else 0f

/**
 * #EXP-1765: the popup's border-box x (px) for [align] along the anchor
 * [left]..[right], [inset] past the aligned PHYSICAL edge (start in LTR =
 * left: `left - inset`; at the right edge: `right + inset - w`), before the
 * window clamp.
 */
internal fun selectPopupX(align: String, rtl: Boolean, left: Int, right: Int, w: Int, inset: Int): Int = when (align) {
    "center" -> left + (right - left - w) / 2
    else -> if ((align == "start") != rtl) left - inset else right + inset - w
}

/** #EXP-1629: does the list have options below its visible end (the IDE picker's `has_more_below`)? */
internal fun selectMoreBelow(offset: Int, max: Int): Boolean = max > 0 && offset < max - 1

/**
 * #EXP-1629: a `multiple` Select's run of adjacent picked SHOWN rows is ONE
 * block (web's Picker, the IDE's `multi_row_edges`): row [index]'s
 * `joinTop` (the row above is picked too) and `joinBottom` (the row below).
 */
internal fun selectItemJoins(multiple: Boolean, picked: List<Boolean>, index: Int): Pair<Boolean, Boolean> {
    if (!multiple || picked.getOrNull(index) != true) return false to false
    return (index > 0 && picked[index - 1]) to (picked.getOrNull(index + 1) == true)
}

/** #EXP-1629: the popup's fixed width (`popupWidth`, else a row's layout width); null = at least the trigger's. */
internal fun selectPopupWidth(owner: Props, row: Boolean): Float? =
    owner.num("popupWidth")?.toFloat()?.takeIf { it > 0f } ?: if (row && selectPopupMinWidth(owner) == null) CatalogConstants.SELECT_ROW_POPUP_WIDTH else null

/** #EXP-1629: `popupMinWidth`: the popup fits its options, at least this wide (the IDE's non-searchable picker). */
internal fun selectPopupMinWidth(owner: Props): Float? = owner.num("popupMinWidth")?.toFloat()?.takeIf { it > 0f }
    // #EXP-1778: a `button`'s pointer menu: at least `selectButtonPopupMinWidth`.
    ?: if (owner.str("triggerVariant") == "button") CatalogConstants.SELECT_BUTTON_POPUP_MIN_WIDTH else null

/**
 * #EXP-1765: where a Select popup's border box lands (px): [align]ed along
 * the anchor ([selectPopupX], [inset] outside its aligned edge FIRST), under
 * it (above when it does not fit) and clamped [margin] inside the window;
 * with an [inset] (a pill's) the content's offset from the anchor's origin
 * then rounds to whole dp, half away from zero (the IDE Popover's,
 * `overlay-geometry.json` `inset`, the core's `place_inset_overlay`).
 */
internal fun selectPopupOrigin(align: String, rtl: Boolean, offset: Int, margin: Int, inset: Int, density: Float, anchor: IntRect, window: IntSize, w: Int, h: Int): IntOffset {
    val x = selectPopupX(align, rtl, anchor.left, anchor.right, w, inset)
    val below = anchor.bottom + offset
    val above = anchor.top - offset - h
    val y = if (below + h <= window.height - margin || above < margin) below else above
    val clamped = x.coerceIn(margin, max(margin, window.width - w - margin))
    if (inset <= 0) return IntOffset(clamped, y)
    fun snap(at: Int, origin: Int): Int {
        val dp = (at + inset - origin) / density
        return origin + (sign(dp) * floor(abs(dp) + 0.5f) * density).roundToInt() - inset
    }
    return IntOffset(snap(clamped, anchor.left), snap(y, anchor.top))
}

/**
 * Under the anchor (above when it does not fit), aligned along it by
 * `start | center | end` (logical), inside the window ([selectPopupOrigin]).
 * [bleed] = the transparent room around the box its shadow paints into (px).
 */
private class SelectPopupPosition(val align: String, val rtl: Boolean, val offset: Int, val margin: Int, val bleed: Int, val inset: Int = 0, val density: Float = 1f) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val w = popupContentSize.width - 2 * bleed
        val h = popupContentSize.height - 2 * bleed
        val at = selectPopupOrigin(align, rtl, offset, margin, inset, density, anchorBounds, windowSize, w, h)
        return IntOffset(at.x - bleed, at.y - bleed)
    }
}

/**
 * #EXP-1629: a Select's popup under native overlays, our own (the IDE
 * picker, never the M3 menu): expanded while `model.popup` names this
 * trigger, anchored under the composable it sits in (a field: the trigger;
 * a row: its VALUE part, `selectRowPopupOffset` under it, the window's
 * `selectRowPopupMargin` around it) by `align` (start), `popupWidth` wide
 * (a row: layout `selectRowPopupWidth`; a field: at least the trigger),
 * painted from the `content` (its shadows, `backgroundClip`) / `search` /
 * `item` / `check` / `empty` / `fade` recipes. It opens at the top with
 * the cursor on the first enabled option (a `multiple` one's on its first
 * chosen) and focus in its search field
 * (else the list); the bottom `fade` shows while options sit below; the cursor
 * (the arrows, or the pointer moving over an option) is the item's ONE
 * `hover`, a chosen option is `selected` and shows the check. A pick →
 * `selectPick` (a single pick closes it).
 *
 * #EXP-1765: a `pill`'s popup is `popupWidth` (else `selectRowPopupWidth`)
 * wide, its border box `selectPillPopupOffset` under the pill and
 * `selectPillPopupInset` outside its aligned edge; opened by the POINTER it
 * shows no cursor until the pointer moves over an option or a key goes
 * (↓ lands on the first enabled option); from the keyboard it keeps it.
 * `loading` swaps the options (and `empty`) for `selectLoadingRows`
 * Skeleton rows; `searchPlaceholder` names the search field.
 */
@Composable
internal fun SelectPopup(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val model = cx.model
    if (model.popup != cx.node.id) return
    val index = cx.index
    val owner = cx.ownerProps
    val chosen = selectValues(model.selectValue(index))
    val multiple = owner.flag("multiple")
    val searchable = owner.flag("searchable")
    val row = selectRowVariant(cx.props, owner)
    val pill = selectPillVariant(cx.props, owner)
    // #EXP-1778: a `button` popup is the IDE's pointer menu.
    val button = selectButtonVariant(cx.props, owner)
    val loading = owner.flag("loading")
    var query by remember(cx.node.id) { mutableStateOf("") }
    // #EXP-1765: a pointer-opened pill starts without a cursor.
    var cursorless by remember(cx.node.id) { mutableStateOf((pill || button) && !model.keyboardDriven) }
    val q = query.trim().lowercase()
    val options = remember(owner, q) { if (loading) emptyList() else selectOptions(owner, q) }
    var cursor by remember(q) { mutableIntStateOf(if (cursorless) -1 else selectInitialCursor(options, if (multiple && q.isEmpty()) chosen else null)) }

    val content = cx.part("Select", "content")
    val cs = content.style
    val popupInk = cs.color ?: cx.themeColor("popoverForeground") ?: cx.ink
    val muted = cx.themeColor("mutedForeground") ?: popupInk.copy(alpha = 0.6f)
    val pad = content.px("padding") ?: cs.padding?.firstOrNull() ?: cx.spacing("xs")
    val shape = RoundedCornerShape(cs.radius.dp)
    // #EXP-1629: an item is `minHeight` + `paddingVertical` (a described one grows).
    val itemH = cx.part("Select", "item").let { it.height ?: it.px("minHeight") } ?: cx.control("buttonSm", 32f)
    val maxH = content.px("maxHeight") ?: 320f
    val width = selectPopupWidth(owner, row || pill)
    // The popup's type is the surface's, never the trigger recipe's own family (a pill's mono).
    val popupTs = if (cx.part("Select", "trigger").fontFamily != null) cx.textStyle.copy(fontFamily = null) else cx.textStyle
    val density = LocalDensity.current
    val scroll = rememberScrollState()
    val scope = rememberCoroutineScope()
    val focus = remember { FocusRequester() }
    val searchFocus = remember { FocusRequester() }

    val close = {
        query = ""
        if (model.popup == cx.node.id) model.popup = null
    }
    val pick = { o: SelectOption ->
        model.selectPick(index, o.value)
        if (!multiple) close()
    }
    // Keeps option [i] in the list's view (nearest edge).
    fun reveal(i: Int) {
        if (i < 0) return
        scope.launch {
            val view = snapshotFlow { scroll.viewportSize }.first { it > 0 }
            val top = with(density) { (i * itemH).dp.toPx() }
            val bottom = with(density) { ((i + 1) * itemH).dp.toPx() }
            val y = when {
                top < scroll.value -> top
                bottom > scroll.value + view -> bottom - view
                else -> return@launch
            }
            scroll.scrollTo(y.roundToInt())
        }
    }
    LaunchedEffect(q) { reveal(cursor) }
    LaunchedEffect(Unit) { runCatching { (if (searchable) searchFocus else focus).requestFocus() } }

    // The shadow paints into transparent room around the box.
    val bleed = cs.shadows.maxOfOrNull { abs(it.x) + abs(it.y) + it.blur + it.spread }?.coerceAtLeast(0f) ?: 0f
    val position = remember(row, pill, button, owner, cx.rtl, density, bleed) {
        val (offset, margin) = selectPopupSpacing(row, pill, button)
        SelectPopupPosition(selectPopupAlign(owner), cx.rtl, with(density) { offset.dp.roundToPx() }, with(density) { margin.dp.roundToPx() }, with(density) { bleed.dp.roundToPx() }, with(density) { selectPopupInset(pill, button).dp.roundToPx() }, density.density)
    }
    val fill = cs.copy(background = cs.background ?: cx.themeColor("popover"))
    val fadeInk = cx.part("Select", "fade").style.background ?: fill.background
    Popup(popupPositionProvider = position, onDismissRequest = close, properties = PopupProperties(focusable = true)) {
        Column(
            Modifier
                .padding(bleed.dp)
                .then(if (width != null) Modifier.width(width.dp) else Modifier.widthIn(min = selectPopupMinWidth(owner)?.dp ?: cx.size.width.dp))
                // #EXP-1778: a `button`'s pointer menu fits its options (the core's rule).
                .then(if (width == null && button) Modifier.width(androidx.compose.foundation.layout.IntrinsicSize.Max) else Modifier)
                .heightIn(max = maxH.dp)
                .paintedSurface(fill)
                .clip(shape)
                .padding(pad.dp)
                .onPreviewKeyEvent { e ->
                    if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    cursorless = false
                    val next = when (e.key) {
                        Key.DirectionDown -> if (cursor < 0) selectInitialCursor(options) else selectStep(options, cursor, 1)
                        Key.DirectionUp -> selectStep(options, cursor, -1)
                        Key.MoveHome -> selectStep(options, -1, 1)
                        Key.MoveEnd -> selectStep(options, options.size, -1).takeIf { it < options.size } ?: cursor
                        Key.Enter, Key.NumPadEnter -> {
                            options.getOrNull(cursor)?.takeIf { !it.disabled }?.let(pick)
                            return@onPreviewKeyEvent true
                        }
                        Key.Escape -> {
                            close()
                            return@onPreviewKeyEvent true
                        }
                        else -> return@onPreviewKeyEvent false
                    }
                    cursor = next
                    reveal(next)
                    true
                }
                .focusRequester(focus)
                .focusable(),
        ) {
            if (searchable) {
                SelectSearch(cx, query, owner.str("searchPlaceholder").ifEmpty { cx.string("search") }, pill, popupTs, popupInk, muted, searchFocus) {
                    cursorless = false
                    query = it
                }
            }
            Box(Modifier.weight(1f, fill = false).fillMaxWidth()) {
                // #EXP-1778: a `button`'s menu rows sit `selectButtonItemGap` apart.
                Column(Modifier.fillMaxWidth().verticalScroll(scroll), verticalArrangement = Arrangement.spacedBy((if (button) CatalogConstants.SELECT_BUTTON_ITEM_GAP else 0f).dp)) {
                    if (loading) SelectSkeletonRows(cx)
                    val picked = options.map { chosen.contains(it.value.displayText) }
                    for ((i, o) in options.withIndex()) {
                        SelectOptionRow(cx, o, i == cursor, picked[i], selectItemJoins(multiple, picked, i), itemH, popupTs, popupInk, checkOnChosenOnly = button, onHover = {
                            cursorless = false
                            cursor = i
                        }, onPick = { pick(o) })
                    }
                }
                // The bottom fade while more options sit below (SearchList's).
                if (fadeInk != null && selectMoreBelow(scroll.value, scroll.maxValue)) {
                    Box(
                        Modifier
                            .align(Alignment.BottomStart)
                            .fillMaxWidth()
                            .height(CatalogConstants.SEARCH_LIST_FADE.dp)
                            .background(Brush.verticalGradient(listOf(fadeInk.copy(alpha = 0f), fadeInk.copy(alpha = 1f)))),
                    )
                }
            }
            if (options.isEmpty() && !loading) {
                val empty = cx.part("Select", "empty")
                BasicText(
                    owner["emptyText"]?.displayText ?: cx.string("noResults"),
                    (empty.px("padding") ?: cx.spacing("sm")).let { p -> Modifier.fillMaxWidth().padding(horizontal = (empty.px("paddingHorizontal") ?: p).dp, vertical = (empty.px("paddingVertical") ?: p).dp) },
                    // #EXP-1765: the recipe's `textAlign` (a pill's centred).
                    style = cx.composeTextStyle(ts = popupTs.copy(fontSize = empty.style.fontSize ?: popupTs.fontSize), color = empty.color ?: muted, align = empty.style.textAlign?.let { leafTextAlign(it, cx.rtl) }),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/**
 * The `search` part: the glyph and the filter field, its recipe height, a
 * hairline under it; #EXP-1765: [hint] (`searchPlaceholder`), the
 * `searchIcon` part's glyph size, the text and glyph centred above the
 * recipe's `paddingBottom` (below its `paddingTop`); a [pill]'s ends in
 * the clear button while it holds text ([SelectSearchClearButton]: empties
 * the query, the focus stays in the field).
 */
@Composable
private fun SelectSearch(cx: LeafContext, query: String, hint: String, pill: Boolean, base: ResolvedTextStyle, popupInk: Color, muted: Color, focus: FocusRequester, onQuery: (String) -> Unit) {
    val search = cx.part("Select", "search")
    val s = search.style
    val ink = search.color ?: popupInk
    val ts = cx.composeTextStyle(ts = base.copy(fontSize = s.fontSize ?: base.fontSize), color = ink)
    val glyph = cx.part("Select", "searchIcon").width ?: 16f
    val line = s.borderColor ?: cx.themeColor("border")
    val hairline = s.borderWidths?.getOrNull(2) ?: search.px("borderBottomWidth") ?: 0f
    Row(
        Modifier
            .fillMaxWidth()
            .height((search.height ?: cx.control("buttonSm", 32f)).dp)
            .drawBehind {
                if (hairline > 0f && line != null) {
                    val w = hairline.dp.toPx()
                    drawLine(line, Offset(0f, size.height - w / 2f), Offset(size.width, size.height - w / 2f), w)
                }
            }
            .padding(top = (search.px("paddingTop") ?: 0f).dp, bottom = (search.px("paddingBottom") ?: 0f).dp)
            .padding(horizontal = (search.px("paddingHorizontal") ?: s.paddingHorizontal).dp),
        horizontalArrangement = Arrangement.spacedBy((search.px("gap") ?: cx.spacing("sm")).dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The placeholder's ink at full opacity (the IDE's search rows).
        IconOrGlyph(builtinIcon("Select.search", "search"), Glyph.Search, glyph, cx, muted)
        BasicTextField(
            value = query,
            onValueChange = onQuery,
            singleLine = true,
            textStyle = ts,
            cursorBrush = SolidColor(ink),
            modifier = Modifier.weight(1f).focusRequester(focus),
            decorationBox = { inner ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (query.isEmpty()) BasicText(hint, style = ts.copy(color = muted), maxLines = 1)
                    inner()
                }
            },
        )
        if (pill && query.isNotEmpty()) {
            SelectSearchClearButton(cx, glyph) {
                onQuery("")
                runCatching { focus.requestFocus() }
            }
        }
    }
}

/** One option: the `item` recipe under its states (`hover` = the cursor, `selected` = chosen), icon, label, the check slot. */
@Composable
private fun SelectOptionRow(cx: LeafContext, o: SelectOption, cursor: Boolean, chosen: Boolean, joins: Pair<Boolean, Boolean>, height: Float, base: ResolvedTextStyle, popupInk: Color, checkOnChosenOnly: Boolean = false, onHover: () -> Unit, onPick: () -> Unit) {
    val states = buildList {
        if (cursor) add("hover")
        if (chosen) add("selected")
        if (o.disabled) add("disabled")
    }
    // #EXP-1629: a joined edge's recipe drops its stroke and corners and
    // pads it (`paddingTop` / `paddingBottom`).
    val joinProps = buildMap {
        if (joins.first) put("joinTop", JsonValue.Bool(true))
        if (joins.second) put("joinBottom", JsonValue.Bool(true))
    }
    val item = if (joinProps.isEmpty()) cx.part("Select", "item", states) else cx.part("Select", "item", cx.ownerProps + joinProps, states)
    val hover by rememberUpdatedState(onHover)
    val s = item.style
    val ink = item.color ?: popupInk
    val check = cx.part("Select", "check", states)
    val checkSize = check.width ?: 16f
    val sans = base.copy(fontSize = s.fontSize ?: base.fontSize, lineHeight = s.lineHeight ?: base.lineHeight, fontFamily = item.fontFamily ?: base.fontFamily)
    // #EXP-1765: a mono option's label in the theme's mono type.
    val ts = cx.composeTextStyle(ts = if (o.mono) sans.copy(fontFamily = cx.model.effectiveTheme?.monoFamily ?: "ui-monospace") else sans, color = ink)
    val (tl, tr, br, bl) = s.corners
    val shape = AbsoluteRoundedCornerShape(tl.dp, tr.dp, br.dp, bl.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = height.dp)
            .alpha(s.opacity ?: 1f)
            .clip(shape)
            // The fill and the stroke per side and corner (a joined edge has neither).
            .paintedSurface(s)
            .pointerInput(o.value) {
                awaitPointerEventScope {
                    while (true) {
                        val e = awaitPointerEvent(PointerEventPass.Initial)
                        if (e.type == PointerEventType.Enter || e.type == PointerEventType.Move) hover()
                    }
                }
            }
            .selectable(selected = chosen, interactionSource = remember { MutableInteractionSource() }, indication = null, enabled = !o.disabled, role = Role.DropdownList, onClick = onPick)
            .padding(start = (item.px("paddingHorizontal") ?: s.paddingHorizontal).dp, end = (item.px("paddingHorizontal") ?: s.paddingHorizontal).dp, top = (item.px("paddingTop") ?: item.px("paddingVertical") ?: 0f).dp, bottom = (item.px("paddingBottom") ?: item.px("paddingVertical") ?: 0f).dp),
        horizontalArrangement = Arrangement.spacedBy((s.gap.takeIf { it > 0f } ?: cx.spacing("sm")).dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // #EXP-1629: the `itemIcon` recipe's size; the `itemDescription` under the label.
        o.icon?.let { IconView(it, cx.part("Select", "itemIcon", states).width ?: 16f, ink, cx.model, rtl = cx.rtl) }
        if (o.description != null) {
            val dp = cx.part("Select", "itemDescription", states)
            val dts = cx.composeTextStyle(ts = cx.textStyle.copy(fontSize = dp.style.fontSize ?: cx.textStyle.fontSize, lineHeight = dp.style.lineHeight ?: cx.textStyle.lineHeight), color = dp.color ?: cx.themeColor("mutedForeground") ?: ink)
            Column(Modifier.weight(1f)) {
                BasicText(o.text, style = ts, maxLines = 1, overflow = TextOverflow.Ellipsis)
                BasicText(o.description, style = dts, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        } else if (o.trailing == null) BasicText(o.text, Modifier.weight(1f), style = ts, maxLines = 1, overflow = TextOverflow.Ellipsis)
        else Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(selectTrailingGap(s.gap).dp), verticalAlignment = Alignment.CenterVertically) {
            // The trailing: its own muted run in the surface's sans type.
            BasicText(o.text, Modifier.weight(1f, fill = false), style = ts, maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicText(o.trailing, style = cx.composeTextStyle(ts = base.copy(fontSize = sans.fontSize, lineHeight = sans.lineHeight, fontFamily = null), color = cx.themeColor("mutedForeground") ?: ink), maxLines = 1, softWrap = false)
        }
        // #EXP-1778: a `button` menu row holds the check on the chosen row only.
        if (chosen || !checkOnChosenOnly) Box(Modifier.size(checkSize.dp)) {
            if (chosen) IconOrGlyph(builtinIcon("Select.check", "ui-check"), Glyph.Check, checkSize, cx, check.color ?: ink)
        }
    }
}

/** #EXP-1765: the gap between a mono option's label and its `trailing` (the item recipe's, at least 4; the measurer's rule). */
internal fun selectTrailingGap(recipeGap: Float): Float = max(recipeGap, 4f)

/** gpui / the core's `SKELETON_BARS`: each Skeleton row's (lead, title) bar widths, cycling. */
private val SKELETON_BARS = listOf(56f to 168f, 56f to 120f, 56f to 192f)

/**
 * #EXP-1765: a `loading` Select's placeholder in our own popup: the core's
 * Skeleton rows (`selectLoadingRows` of them, `$control.row` high,
 * borderless, the dot and the two bars) from the `Skeleton/row|dot|bar`
 * recipes, each block pulsing like the painted Skeleton's.
 */
@Composable
private fun SelectSkeletonRows(cx: LeafContext) {
    val none: Props = emptyMap()
    val rowPart = cx.part("Skeleton", "row", none)
    val dot = cx.part("Skeleton", "dot", none)
    val bar = cx.part("Skeleton", "bar", none)
    val fill = cx.themeColor("muted") ?: Color.Transparent
    val h = cx.control("row", 32f)
    val reduced = cx.model.reducedMotion
    for (r in 0 until CatalogConstants.SELECT_LOADING_ROWS) {
        val (lead, title) = SKELETON_BARS[r % SKELETON_BARS.size]
        Row(
            Modifier
                .fillMaxWidth()
                .height(h.dp)
                .padding(horizontal = (rowPart.px("paddingHorizontal") ?: rowPart.style.paddingHorizontal).dp),
            horizontalArrangement = Arrangement.spacedBy((rowPart.px("gap") ?: rowPart.style.gap).dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SkeletonBlock(dot, dot.width ?: 12f, fill, reduced)
            SkeletonBlock(bar, lead, fill, reduced)
            SkeletonBlock(bar, title, fill, reduced, Modifier.weight(1f, fill = false))
        }
    }
}

/** One Skeleton dot / bar of [SelectSkeletonRows]: its recipe's height, radius and fill (else [fill]), pulsing. */
@Composable
private fun SkeletonBlock(p: at.exponential.ui.theme.PartStyle, w: Float, fill: Color, reduced: Boolean, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(p.style.radius.dp)
    Box(modifier.width(w.dp).height((p.height ?: 8f).dp).skeletonPulse(true, reduced).background(p.style.background ?: fill, shape))
}
