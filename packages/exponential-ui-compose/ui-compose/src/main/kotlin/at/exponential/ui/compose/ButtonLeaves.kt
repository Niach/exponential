package at.exponential.ui.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
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

/**
 * Button / Toggle content: the spinner (`loading`) or the icon (host icon,
 * else the placeholder circle; `size: icon` always shows one), then the
 * label, centred in the content box. The press is the node's (NodeView).
 */
@Composable
internal fun ButtonLeaf(cx: LeafContext) {
    val label = cx.props.shownText("label")
    val icon = cx.props.str("icon")
    val loading = cx.props.flag("loading")
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
    val ts = ResolvedTextStyle(
        item.px("fontSize") ?: cx.textStyle.fontSize,
        (item.props["fontWeight"]?.number ?: 500.0).toInt(),
        cx.textStyle.lineHeight,
        item.fontFamily,
    )
    val fill = bar || props.flag("fill")
    val disabledAll = cx.model.isDisabled(cx.index)
    val model = cx.model
    val index = cx.index
    InnerBox(cx, Alignment.CenterStart) {
        Row(
            if (fill) Modifier.fillMaxWidth() else Modifier,
            horizontalArrangement = Arrangement.spacedBy(cx.style.gap.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            for ((k, it) in items.withIndex()) {
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
                val select = Modifier
                    .selectable(selected = selected, enabled = !disabled, role = Role.Tab) { model.segmentedSelect(index, value) }
                    .semantics { contentDescription = label.ifEmpty { value.displayText } }
                key(k) {
                    if (bar) {
                        SegmentedBarItem(cx, it, label, states, ink, h, Modifier.weight(1f).leafPartBox(st.style).then(select))
                    } else {
                        Box(
                            (if (fill) Modifier.weight(1f) else Modifier)
                                .height(h.dp)
                                .leafPartBox(st.style)
                                .then(select)
                                .padding(horizontal = pad.dp),
                            contentAlignment = Alignment.Center,
                        ) {
                            Row(horizontalArrangement = Arrangement.spacedBy(itemGap.dp), verticalAlignment = Alignment.CenterVertically) {
                                it["icon"]?.string?.let { name -> IconView(name, 16f, ink, model) }
                                if (label.isNotEmpty()) LeafLine(cx, label, color = ink, ts = ts)
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * One `bar` Segmented item (gpui `segmented_bar_item`): a column, the
 * `Segmented/icon` part (`$control.iconMd`) over the `Segmented/label`
 * part in the caption text style, `xxs` apart, `xs` vertical padding.
 */
@Composable
private fun SegmentedBarItem(cx: LeafContext, item: JsonValue, label: String, states: List<String>, ink: Color, h: Float, modifier: Modifier) {
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
        item["icon"]?.string?.let { name -> IconView(name, iconSize, iconInk, cx.model) }
        if (label.isNotEmpty()) LeafLine(cx, label, color = labelInk, ts = ts)
    }
}
