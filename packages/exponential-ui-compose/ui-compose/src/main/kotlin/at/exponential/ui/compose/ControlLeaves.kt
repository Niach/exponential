package at.exponential.ui.compose

import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.DpSize
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.BasicText
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.num
import at.exponential.ui.json.shownText
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.checked
import at.exponential.ui.model.press
import at.exponential.ui.model.radioChecked
import at.exponential.ui.model.selectPick
import at.exponential.ui.model.selectValue
import at.exponential.ui.model.sliderDrag
import at.exponential.ui.model.sliderRelease
import at.exponential.ui.model.sliderValue
import at.exponential.ui.primitives.DrawnSwitch
import at.exponential.ui.primitives.DrawnSwitchColors
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.max
import kotlin.math.min

/** A Checkbox `box`: the `Checkbox/check` glyph while checked (the box chrome is the node's; the press is NodeView's). */
@Composable
internal fun CheckboxBox(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    if (!cx.model.checked(cx.index)) return
    val check = cx.part("Checkbox", "check", listOf("checked"))
    LeafFrame {
        GlyphView(Glyph.Check, check.width ?: 12f, check.color ?: cx.ink, Modifier.align(Alignment.Center), GlyphWeight.Bold)
    }
}

/** A Radio `dot`: the inner dot (`Radio/dot` with `checked`) while chosen. */
@Composable
internal fun RadioDot(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    if (!cx.model.radioChecked(cx.index)) return
    val dot = cx.part("Radio", "dot", listOf("checked"))
    val d = dot.width ?: (min(cx.size.width, cx.size.height) / 2f)
    LeafFrame {
        Box(Modifier.align(Alignment.Center).size(d.dp).background(dot.style.background ?: cx.ink, CircleShape))
    }
}

/**
 * A Switch `track`. Drawn (`Switch/track` recipe `native: false`, the
 * built-in themes; geometry mode): the primitives' `DrawnSwitch` in the
 * track frame with the recipe's track (checked / unchecked) and
 * `Switch/thumb` colours, an indicator only (the press is NodeView's).
 * Native (the recipe silent on `native`): the Material 3 `Switch` scaled
 * into the track frame, toggling through `press`.
 */
@Composable
internal fun SwitchTrack(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val model = cx.model
    val index = cx.index
    val checked = model.checked(index)
    val disabled = model.isDisabled(index)
    val track = cx.part("Switch", "track")
    if (track.native == false || model.theme == null) {
        fun withChecked(on: Boolean): Props = cx.ownerProps + ("checked" to JsonValue.Bool(on))
        val trackOn = cx.part("Switch", "track", withChecked(true), listOf("checked"))
        val trackOff = cx.part("Switch", "track", withChecked(false))
        val thumbOn = cx.part("Switch", "thumb", withChecked(true), listOf("checked"))
        val thumbOff = cx.part("Switch", "thumb", withChecked(false))
        val thumb = if (checked) thumbOn else thumbOff
        val thumbSize = thumb.width ?: max(4f, cx.size.height - 4f)
        val primary = cx.themeColor("primary") ?: cx.ink
        val colors = DrawnSwitchColors(
            trackOn = trackOn.style.background ?: primary,
            trackOff = trackOff.style.background ?: cx.themeColor("input") ?: cx.ink.copy(alpha = 0.2f),
            thumb = thumbOff.style.background ?: cx.themeColor("background") ?: Color.White,
            thumbOn = thumbOn.style.background ?: cx.themeColor("background") ?: Color.White,
            trackStroke = trackOff.style.borderColor?.takeIf { trackOff.style.borderWidth > 0f },
            trackStrokeOn = trackOn.style.borderColor?.takeIf { trackOn.style.borderWidth > 0f },
        )
        LeafFrame {
            DrawnSwitch(
                checked = checked,
                onCheckedChange = null,
                modifier = Modifier.align(Alignment.TopStart),
                enabled = !disabled,
                colors = colors,
                trackWidth = cx.size.width.dp,
                trackHeight = cx.size.height.dp,
                thumbSize = thumbSize.dp,
                trackStrokeWidth = max(trackOn.style.borderWidth, trackOff.style.borderWidth).dp,
            )
        }
    } else {
        val scale = cx.size.height / 32f
        val primary = cx.themeColor("primary")
        LeafFrame {
            Switch(
                checked = checked,
                onCheckedChange = { model.press(index) },
                enabled = !disabled,
                colors = if (primary != null) SwitchDefaults.colors(checkedTrackColor = primary) else SwitchDefaults.colors(),
                modifier = Modifier
                    .align(Alignment.Center)
                    .wrapContentSize(unbounded = true)
                    .requiredSize(52.dp, 32.dp)
                    .graphicsLayer(scaleX = scale, scaleY = scale),
            )
        }
    }
}

/**
 * A Slider `track`. Drawn (`Slider/track` `native: false`; geometry mode):
 * the track bar, the `Slider/range` fill and the `Slider/thumb` at the
 * value, with a drag that snaps to `step` (`sliderDrag`, `sliderRelease`
 * on lift). Native: the Material 3 `Slider` laid over the track frame
 * (its 48 dp touch height overflows the frame).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SliderTrack(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val model = cx.model
    val index = cx.index
    val lo = cx.props.num("min") ?: 0.0
    val hi = cx.props.num("max") ?: 100.0
    val step = cx.props.num("step") ?: 1.0
    val value = model.sliderValue(index)
    val track = cx.part("Slider", "track")
    val disabled = model.isDisabled(index)
    if (track.native == false || model.theme == null) {
        val range = cx.part("Slider", "range")
        val thumb = cx.part("Slider", "thumb", cx.states.filter { it == "focus" || it == "hover" })
        val w = cx.size.width
        val barH = min(track.height ?: 6f, max(cx.size.height, 1f))
        val t = thumb.width ?: 16f
        val f = if (hi > lo) ((value - lo) / (hi - lo)).coerceIn(0.0, 1.0).toFloat() else 0f
        val trackColor = track.style.background ?: cx.themeColor("muted") ?: cx.ink.copy(alpha = 0.15f)
        val rangeColor = range.style.background ?: cx.themeColor("primary") ?: cx.ink
        LeafFrame {
            Box(
                Modifier
                    .fillMaxSize()
                    .pointerInput(index, lo, hi, step, disabled) {
                        if (disabled) return@pointerInput
                        fun at(x: Float) {
                            val frac = if (size.width > 0) (x / size.width).coerceIn(0f, 1f) else 0f
                            model.sliderDrag(index, SurfaceModel.snap(lo + frac * (hi - lo), lo, hi, step))
                        }
                        awaitEachGesture {
                            val down = awaitFirstDown()
                            at(down.position.x)
                            down.consume()
                            drag(down.id) { change ->
                                at(change.position.x)
                                change.consume()
                            }
                            model.sliderRelease(index)
                        }
                    },
            ) {
                Box(Modifier.align(Alignment.CenterStart).width(w.dp).height(barH.dp).background(trackColor, CircleShape))
                Box(Modifier.align(Alignment.CenterStart).width((w * f).dp).height(barH.dp).background(rangeColor, CircleShape))
                val border = thumb.style.borderColor
                Box(
                    Modifier
                        .align(Alignment.CenterStart)
                        .absoluteOffset(x = ((w - t) * f).dp)
                        .size(t.dp)
                        .then(if (thumb.style.shadows.isNotEmpty()) Modifier.shadow(2.dp, CircleShape) else Modifier)
                        .background(thumb.style.background ?: Color.White, CircleShape)
                        .then(if (border != null && thumb.style.borderWidth > 0f) Modifier.border(thumb.style.borderWidth.dp, border, CircleShape) else Modifier),
                )
            }
        }
    } else {
        val primary = cx.themeColor("primary")
        // The unfilled bar: the track recipe's background, else the theme's muted colour.
        val inactive = cx.style.background ?: cx.themeColor("muted")
        val top = max(hi, lo + 1)
        val colors = SliderDefaults.colors(
            thumbColor = primary ?: Color.Unspecified,
            activeTrackColor = primary ?: Color.Unspecified,
            inactiveTrackColor = inactive ?: Color.Unspecified,
        )
        val interactions = remember(cx.node.id) { MutableInteractionSource() }
        LeafFrame {
            Slider(
                value = value.toFloat().coerceIn(lo.toFloat(), top.toFloat()),
                onValueChange = { model.sliderDrag(index, SurfaceModel.snap(it.toDouble(), lo, hi, step)) },
                onValueChangeFinished = { model.sliderRelease(index) },
                valueRange = lo.toFloat()..top.toFloat(),
                enabled = !disabled,
                colors = colors,
                interactionSource = interactions,
                // M3's default 44 dp bar thumb would cover the next row: a
                // 16 dp one stays inside the slider's own row, like web/iOS.
                thumb = {
                    SliderDefaults.Thumb(
                        interactionSource = interactions,
                        colors = colors,
                        enabled = !disabled,
                        thumbSize = DpSize(4.dp, 16.dp),
                    )
                },
                // The track frame is the 6 dp bar: the slider keeps its width
                // and overflows vertically (48 dp touch height).
                modifier = Modifier.align(Alignment.Center).requiredWidth(cx.size.width.dp).requiredHeight(48.dp),
            )
        }
    }
}
/**
 * A picker trigger's content (round 1: the core's `trigger` part paints the
 * `<Component>/trigger` recipe and carries the resolved `text` and
 * `placeholder` flag): the text (muted, `<Component>/placeholder`, while a
 * placeholder) and the chevrons / calendar / clock glyph, in the leaf's
 * content box (what the measurer counted).
 */
@Composable
internal fun TriggerContent(cx: LeafContext, modifier: Modifier = Modifier) {
    val component = cx.node.recipeComponent
    val ph = cx.part(component, "placeholder")
    val placeholder = cx.props.flag("placeholder")
    val muted = ph.color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = 0.6f)
    val glyph = when (component) {
        "Select" -> Glyph.ChevronsUpDown
        "TimePicker" -> Glyph.Clock
        else -> Glyph.Calendar
    }
    val r = cx.inner
    LeafFrame {
        Row(
            modifier.fillMaxSize().padding(start = r.left.dp, end = (cx.size.width - r.right).coerceAtLeast(0f).dp),
            horizontalArrangement = Arrangement.spacedBy(max(cx.spacing("sm"), 0f).dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f), color = if (placeholder) muted else cx.ink)
            GlyphView(glyph, 16f, cx.ink.copy(alpha = cx.ink.alpha * 0.6f))
        }
    }
}

/** The display strings of a Select value (an array = multiple). */
internal fun selectValues(v: JsonValue): List<String> = when (v) {
    is JsonValue.Arr -> v.v.map { it.displayText }
    JsonValue.Null -> emptyList()
    else -> listOf(v.displayText)
}

/** The tap and the ONE TalkBack node of a picker trigger (its text is not a stop of its own). */
private fun Modifier.triggerGestures(cx: LeafContext): Modifier {
    val model = cx.model
    val index = cx.index
    val owner = cx.ownerProps
    val disabled = model.isDisabled(index)
    val text = cx.props.str("text")
    return pointerInput(index, disabled) { detectTapGestures { if (!disabled) model.press(index) } }
        .clearAndSetSemantics {
            contentDescription = owner.str("label").ifEmpty { owner.str("placeholder") }
            stateDescription = text
            role = Role.DropdownList
            if (disabled) disabled()
            onClick {
                if (!disabled) model.press(index)
                true
            }
        }
}

/**
 * A picker `trigger` (Select, DatePicker, TimePicker, DateRangePicker): the
 * core's text and the glyph; a tap presses it. Under native overlays a
 * Select whose recipe does not paint its popup opens a Material 3
 * `DropdownMenu` anchored here ([SelectMenu]) and a DatePicker the M3 date
 * dialog (`DatePopup`); otherwise the core opens the owner's popup layer.
 */
@Composable
internal fun PickerTriggerLeaf(cx: LeafContext) {
    LeafFrame {
        TriggerContent(cx, Modifier.triggerGestures(cx))
        if (cx.node.component == "Select" && cx.model.nativePicker(cx.node)) SelectMenu(cx)
    }
}

/**
 * The native Select menu: expanded while `model.popup` names this trigger;
 * `options` (`searchable` = a filter field on top; `multiple` = check
 * marks, the menu stays open per pick); a pick → `selectPick`.
 */
@Composable
private fun SelectMenu(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val model = cx.model
    val index = cx.index
    val owner = cx.ownerProps
    val values = selectValues(model.selectValue(index))
    val multiple = owner.flag("multiple")
    val searchable = owner.flag("searchable")
    val expanded = model.popup == cx.node.id
    var query by remember(cx.node.id) { mutableStateOf("") }
    val content = cx.part("Select", "content")
    val itemPart = cx.part("Select", "item")
    val itemInk = itemPart.color ?: content.color ?: cx.themeColor("popoverForeground") ?: cx.ink
    val muted = cx.themeColor("mutedForeground") ?: itemInk.copy(alpha = 0.6f)
    val ts = cx.composeTextStyle(color = itemInk)
    val close = {
        query = ""
        if (model.popup == cx.node.id) model.popup = null
    }
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = close,
        containerColor = content.style.background ?: cx.themeColor("popover") ?: MenuDefaults.containerColor,
        shape = RoundedCornerShape((content.style.radius.takeIf { it > 0f } ?: 6f).dp),
    ) {
        if (searchable) {
            Row(
                Modifier.padding(horizontal = 12.dp, vertical = 6.dp).width(max(cx.size.width - 24f, 120f).dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                GlyphView(Glyph.Search, 16f, muted)
                BasicTextField(
                    value = query,
                    onValueChange = { query = it },
                    singleLine = true,
                    textStyle = ts,
                    cursorBrush = SolidColor(itemInk),
                    modifier = Modifier.weight(1f),
                    decorationBox = { inner ->
                        Box(contentAlignment = Alignment.CenterStart) {
                            if (query.isEmpty()) BasicText(cx.string("search"), style = ts.copy(color = muted), maxLines = 1)
                            inner()
                        }
                    },
                )
            }
        }
        val q = query.trim().lowercase()
        for (option in owner.list("options")) {
            // A null (or any non-object) entry is skipped, never painted as an empty row.
            if (option !is JsonValue.Obj) continue
            val v = option["value"] ?: JsonValue.Null
            val text = option["label"]?.displayText ?: v.displayText
            if (q.isNotEmpty() && !text.lowercase().contains(q)) continue
            val chosen = values.contains(v.displayText)
            DropdownMenuItem(
                text = { BasicText(text, style = ts, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                onClick = {
                    model.selectPick(index, v)
                    if (!multiple) close()
                },
                enabled = option["disabled"]?.bool != true,
                leadingIcon = if (values.isEmpty()) null else ({
                    Box(Modifier.size(16.dp)) { if (chosen) GlyphView(Glyph.Check, 16f, itemInk) }
                }),
            )
        }
    }
}
