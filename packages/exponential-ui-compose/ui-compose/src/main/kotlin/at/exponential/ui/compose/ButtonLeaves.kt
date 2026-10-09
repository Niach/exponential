package at.exponential.ui.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.shownText
import at.exponential.ui.json.str
import at.exponential.ui.model.toggleGroupSelect
import at.exponential.ui.model.toggleGroupValues
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
 * A ToggleGroup: its items as pills drawn from the `ToggleGroup/item`
 * recipe (with `selected` / `disabled` states from the mirrored values);
 * a tap selects through `toggleGroupSelect` (single or multiple); `fill`
 * shares the row.
 */
@Composable
internal fun ToggleGroupLeaf(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val props = cx.props
    val items = props.list("items")
    val values = cx.model.toggleGroupValues(cx.index)
    val item = cx.part("ToggleGroup", "item")
    val pad = item.px("paddingHorizontal") ?: item.px("padding") ?: 12f
    val h = item.height ?: 36f
    // Icon ↔ label: the recipe's gap (none = 0, as the web and gpui).
    val itemGap = item.px("gap") ?: 0f
    val ts = ResolvedTextStyle(
        item.px("fontSize") ?: cx.textStyle.fontSize,
        (item.props["fontWeight"]?.number ?: 500.0).toInt(),
        cx.textStyle.lineHeight,
        item.fontFamily,
    )
    val fill = props.flag("fill")
    val disabledAll = cx.model.isDisabled(cx.index)
    val model = cx.model
    val index = cx.index
    InnerBox(cx, Alignment.CenterStart) {
        Row(horizontalArrangement = Arrangement.spacedBy(cx.style.gap.dp), verticalAlignment = Alignment.CenterVertically) {
            for (it in items) {
                val value = it["value"] ?: JsonValue.Str(it["label"]?.displayText ?: "")
                val selected = values.contains(value.displayText)
                val disabled = disabledAll || it["disabled"]?.bool == true
                val states = buildList {
                    if (selected) add("selected")
                    if (disabled) add("disabled")
                }
                val st = cx.part("ToggleGroup", "item", states)
                val ink = st.color ?: cx.ink
                val label = it["label"]?.displayText ?: ""
                Box(
                    (if (fill) Modifier.weight(1f) else Modifier)
                        .height(h.dp)
                        .leafPartBox(st.style)
                        .selectable(selected = selected, enabled = !disabled, role = Role.Tab) { model.toggleGroupSelect(index, value) }
                        .semantics { contentDescription = label.ifEmpty { value.displayText } }
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
