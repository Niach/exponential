package at.exponential.ui.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.background
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layout
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.foundation.selection.selectable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.shownText
import at.exponential.ui.json.str
import at.exponential.ui.model.segmentedSelect
import at.exponential.ui.model.segmentedValues
import at.exponential.ui.theme.ResolvedTextStyle
import kotlinx.coroutines.delay
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * Button / Toggle content: the spinner (`loading`) or the icon (host icon,
 * else the placeholder circle; `size: icon` always shows one), then the
 * label, centred in the content box. The press is the node's (NodeView).
 */
@Composable
internal fun ButtonLeaf(cx: LeafContext) {
    // EXP-1259 (feedback-states.json `pending`): a `loading` Button refuses
    // presses at once (the core) but LOOKS idle for `PENDING_DELAY_MS`; then
    // the icon slot is the spinner and the label `pendingLabel`, in the box
    // the measurer sized for the wider face.
    val busy = cx.props.flag("loading")
    var shown by remember(cx.node.id) { mutableStateOf(false) }
    LaunchedEffect(busy) {
        shown = false
        if (busy) {
            delay(FeedbackTiming.PENDING_DELAY_MS)
            shown = true
        }
    }
    val loading = busy && shown
    val pendingLabel = cx.props.shownText("pendingLabel")
    val label = if (loading && pendingLabel.isNotEmpty()) pendingLabel else cx.props.shownText("label")
    val icon = cx.props.str("icon")
    val iconOnly = cx.props.str("size") == "icon"
    val iconSize = cx.part(cx.node.component, "icon").width ?: cx.control("iconSm", 16f)
    InnerBox(cx, Alignment.Center) {
        Row(horizontalArrangement = Arrangement.spacedBy(cx.style.gap.dp), verticalAlignment = Alignment.CenterVertically) {
            when {
                loading -> SpinnerView(iconSize, cx.ink)
                icon.isNotEmpty() -> IconView(icon, iconSize, cx.ink, cx.model)
                iconOnly -> IconView("", iconSize, cx.ink, cx.model)
            }
            if (!iconOnly && label.isNotEmpty()) LeafLine(cx, label, Modifier.weight(1f, fill = false))
        }
    }
}

/**
 * A Segmented (round 3, the old ToggleGroup + ButtonGroup + TabBar): its
 * items drawn from the `Segmented/item` recipe (with `selected` /
 * `disabled` states from the mirrored values); a tap selects through
 * `segmentedSelect` (single or multiple); `fill` shares the row. `bar`
 * (the old TabBar) = full-width bottom destinations, each item a COLUMN
 * of the `icon` part over a caption `label` part, navigation semantics
 * (a tab whose selected item is the current page).
 */
@Composable
internal fun SegmentedLeaf(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val props = cx.props
    val items = props.list("items")
    val values = cx.model.segmentedValues(cx.index)
    val bar = props.str("variant") == "bar"
    val item = cx.part("Segmented", "item")
    val pad = item.px("paddingHorizontal") ?: item.px("padding") ?: 12f
    val h = if (bar) cx.inner.height else item.height ?: 36f
    // Icon ↔ label: the recipe's gap (none = 0, as the web and gpui).
    val itemGap = item.px("gap") ?: 0f
    // The measurer's item style: the group's tracking rides along (gpui).
    val ts = ResolvedTextStyle(
        item.px("fontSize") ?: cx.textStyle.fontSize,
        (item.props["fontWeight"]?.number ?: 500.0).toInt(),
        cx.textStyle.lineHeight,
        item.fontFamily,
        cx.textStyle.letterSpacing,
    )
    val border = item.px("borderWidth") ?: 0f
    val fill = bar || props.flag("fill")
    // VAPP-162: `orientation: vertical` stacks the segments; a vertical
    // `bar` is THE navigation rail (items as wide as the rail, each
    // `$control.tabBar` tall, laid out by the core).
    val vertical = props.str("orientation") == "vertical"
    val railItem = if (bar && vertical) item.height ?: cx.control("tabBar", 56f) else h
    val disabledAll = cx.model.isDisabled(cx.index)
    val model = cx.model
    val index = cx.index
    val segment: @Composable (Int, JsonValue, Modifier) -> Unit = { k, it, slot ->
        val value = it["value"] ?: JsonValue.Str(it["label"]?.displayText ?: "")
        val selected = values.contains(value.displayText)
        val disabled = disabledAll || it["disabled"]?.bool == true
        val states = buildList {
            if (selected) add("selected")
            if (disabled) add("disabled")
        }
        val st = cx.part("Segmented", "item", states)
        val ink = st.color ?: cx.ink
        val label = it["label"]?.displayText ?: ""
        // A `bar` item's count badge reads after the label ("Chats, 3").
        val badge = if (bar) segmentedBadge(it) else null
        val select = Modifier
            .selectable(selected = selected, enabled = !disabled, role = Role.Tab) { model.segmentedSelect(index, value) }
            .semantics { contentDescription = segmentedItemLabel(label.ifEmpty { value.displayText }, badge) }
        key(k) {
            if (bar) {
                SegmentedBarItem(cx, it, label, states, ink, if (vertical) railItem else h, badge, slot.leafPartBox(st.style).then(select))
            } else {
                // VAPP-164: the item's width is the measurer's (set by
                // [SegmentRow]); its padding is the centring slack around
                // the content, never a px-rounded inset the label loses.
                Box(
                    slot
                        .height(h.dp)
                        .leafPartBox(st.style)
                        .then(select),
                    contentAlignment = Alignment.Center,
                ) {
                    Row(horizontalArrangement = Arrangement.spacedBy(itemGap.dp), verticalAlignment = Alignment.CenterVertically) {
                        it["icon"]?.string?.let { name -> IconView(name, 16f, ink, model) }
                        if (label.isNotEmpty()) LeafLine(cx, label, Modifier.weight(1f, fill = false), color = ink, ts = ts)
                    }
                }
            }
        }
    }
    if (vertical) {
        InnerBox(cx, Alignment.TopStart) {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(if (bar) 0.dp else cx.style.gap.dp)) {
                for ((k, it) in items.withIndex()) segment(k, it, Modifier.fillMaxWidth())
            }
        }
        return
    }
    if (bar) {
        InnerBox(cx, Alignment.CenterStart) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(cx.style.gap.dp), verticalAlignment = Alignment.CenterVertically) {
                for ((k, it) in items.withIndex()) segment(k, it, Modifier.weight(1f))
            }
        }
        return
    }
    // VAPP-164: each segment as wide as the measurer made it (2 × (padding
    // + border) + icon + gap + label, the core's gap between), laid out at
    // FRACTIONAL dp like the core and gpui. A Row of padded boxes rounded
    // every padding and gap to whole px and ellipsized the last label.
    val shaper = cx.model.textShaper()
    val widths = items.map {
        val label = cx.shown(it["label"]?.displayText ?: "")
        val lw = if (label.isEmpty()) 0f else shaper.maxContent(label, ts)
        val iw = if (it["icon"]?.string != null) 16f else 0f
        2 * (pad + border) + iw + (if (iw > 0f && lw > 0f) itemGap else 0f) + lw
    }
    InnerBox(cx, Alignment.CenterStart) {
        SegmentRow(widths, cx.style.gap, fill) {
            for ((k, it) in items.withIndex()) segment(k, it, Modifier)
        }
    }
}

/**
 * The horizontal segments at the measurer's widths ([widths], dp; `fill`
 * shares the row equally), [gap] dp apart: positions accumulate in dp and
 * each edge rounds to px once, so the row never drifts past its box.
 */
@Composable
private fun SegmentRow(widths: List<Float>, gap: Float, fill: Boolean, content: @Composable () -> Unit) {
    Layout(content, if (fill) Modifier.fillMaxWidth() else Modifier) { measurables, c ->
        val d = density
        val n = measurables.size
        val avail = if (c.hasBoundedWidth) c.maxWidth else Int.MAX_VALUE
        val share = if (fill && n > 0) (avail / d - gap * (n - 1)) / n else 0f
        var x = 0f
        val placed = measurables.mapIndexed { i, m ->
            val w = if (fill) share else widths.getOrElse(i) { 0f }
            val left = min((x * d).roundToInt(), avail)
            val right = min(((x + w) * d).roundToInt(), avail)
            x += w + gap
            val wp = max(right - left, 0)
            left to m.measure(Constraints(minWidth = wp, maxWidth = wp, maxHeight = c.maxHeight))
        }
        val width = if (fill) avail else placed.maxOfOrNull { (l, p) -> l + p.width } ?: 0
        val height = placed.maxOfOrNull { it.second.height } ?: 0
        layout(max(width, c.minWidth), max(height, c.minHeight)) {
            for ((l, p) in placed) p.placeRelative(l, (height - p.height) / 2)
        }
    }
}

/**
 * One `bar` Segmented item (gpui `segmented_bar_item`): a column, the
 * `Segmented/icon` part (`$control.iconMd`) over the `Segmented/label`
 * part in the caption text style, `xxs` apart, `xs` vertical padding.
 */
@Composable
private fun SegmentedBarItem(cx: LeafContext, item: JsonValue, label: String, states: List<String>, ink: Color, h: Float, badge: String?, modifier: Modifier) {
    val iconInk = cx.part("Segmented", "icon", states).color ?: ink
    val labelPart = cx.part("Segmented", "label", states)
    val labelInk = labelPart.color ?: ink
    val caption = cx.part("Text", "root", mapOf("variant" to JsonValue.Str("caption")))
    val ts = ResolvedTextStyle(
        caption.px("fontSize") ?: 12f,
        labelPart.props["fontWeight"]?.number?.toInt() ?: cx.textStyle.fontWeight,
        caption.px("lineHeight") ?: 16f,
        cx.textStyle.fontFamily,
    )
    val iconSize = cx.control("iconMd", 20f)
    Column(
        modifier.height(h.dp).padding(vertical = cx.spacing("xs").dp),
        verticalArrangement = Arrangement.spacedBy(cx.spacing("xxs").dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        val icon = item["icon"]?.string
        if (icon != null) {
            Box(Modifier.size(iconSize.dp)) {
                IconView(icon, iconSize, iconInk, cx.model)
                if (badge != null) SegmentedBadge(cx, badge, Modifier.align(if (cx.rtl) Alignment.TopStart else Alignment.TopEnd))
            }
        }
        if (label.isNotEmpty()) LeafLine(cx, label, color = labelInk, ts = ts)
    }
}

/**
 * VAPP-162 (round-5 §3): a `bar` item's count badge, its CENTRE on the
 * icon box's top-end corner ([modifier] aligns the zero-size anchor there;
 * mirrored under RTL), the `Segmented/badge` part: min width 16, height 16,
 * xxs horizontal padding, a full radius, the primary fill, xs medium text.
 */
@Composable
private fun SegmentedBadge(cx: LeafContext, text: String, modifier: Modifier) {
    val part = cx.part("Segmented", "badge")
    val minW = part.px("minWidth") ?: 16f
    val bh = part.height ?: 16f
    val padH = part.px("paddingHorizontal") ?: part.px("padding") ?: cx.spacing("xxs")
    val ts = ResolvedTextStyle(
        part.px("fontSize") ?: 12f,
        (part.props["fontWeight"]?.number ?: 500.0).toInt(),
        part.px("lineHeight") ?: 16f,
        part.fontFamily ?: cx.textStyle.fontFamily,
    )
    val fill = part.style.background ?: cx.themeColor("primary") ?: cx.ink
    val ink = part.color ?: cx.themeColor("primaryForeground") ?: Color.White
    Box(
        modifier
            .layout { measurable, _ ->
                // A zero-size anchor at the corner: the badge is centred on it.
                val placeable = measurable.measure(Constraints())
                layout(0, 0) { placeable.place(-placeable.width / 2, -placeable.height / 2) }
            },
    ) {
        Box(
            Modifier
                .widthIn(min = minW.dp)
                .height(bh.dp)
                .background(fill, RoundedCornerShape(50))
                .padding(horizontal = padH.dp),
            contentAlignment = Alignment.Center,
        ) {
            BasicText(text, style = cx.composeTextStyle(ts = ts, color = ink, align = TextAlign.Center), softWrap = false, maxLines = 1)
        }
    }
}
