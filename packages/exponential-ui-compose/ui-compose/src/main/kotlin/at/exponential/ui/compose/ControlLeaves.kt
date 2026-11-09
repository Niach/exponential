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
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.num
import at.exponential.ui.json.shownText
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.checked
import at.exponential.ui.model.mixed
import at.exponential.ui.model.press
import at.exponential.ui.model.radioChecked
import at.exponential.ui.model.sliderDrag
import at.exponential.ui.model.sliderRelease
import at.exponential.ui.model.sliderValue
import at.exponential.ui.primitives.DrawnSwitch
import at.exponential.ui.primitives.DrawnSwitchColors
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.max
import kotlin.math.min

/**
 * A Checkbox `box`: the `Checkbox/check` glyph while checked, the dash
 * (round 8: `Checkbox.check.indeterminate` = `ui-minus`) while mixed, which
 * wins over checked (the box chrome is the node's; the press is NodeView's).
 */
@Composable
internal fun CheckboxBox(cx: LeafContext) {
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val mixed = cx.model.mixed(cx.index)
    if (!mixed && !cx.model.checked(cx.index)) return
    val check = cx.part("Checkbox", "check", listOf("checked"))
    LeafFrame {
        GlyphView(if (mixed) Glyph.Minus else Glyph.Check, check.width ?: 12f, check.color ?: cx.ink, Modifier.align(Alignment.Center), GlyphWeight.Bold)
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

/** The platform slider's range and M3 `steps` (the stops BETWEEN the ends). */
internal data class SliderRange(val range: ClosedFloatingPointRange<Float>, val steps: Int)

/**
 * The author's real range (only an empty or inverted one widens to
 * `min..min+1`); `step <= 0` = continuous (`steps = 0`); a step that divides
 * the range into at most [MAX_SLIDER_STOPS] stops = `stops - 1` M3 steps, any
 * other step stays continuous on the platform and snaps through
 * `SurfaceModel.snap` (the last stop is `max`, not a multiple of `step`).
 */
internal fun sliderRange(min: Double, max: Double, step: Double): SliderRange {
    val hi = if (max > min) max else min + 1
    var steps = 0
    if (step > 0) {
        val n = (hi - min) / step
        val stops = Math.round(n)
        if (stops in 1..MAX_SLIDER_STOPS && kotlin.math.abs(n - stops) < 1e-6) steps = (stops - 1).toInt()
    }
    return SliderRange(min.toFloat()..hi.toFloat(), steps)
}

internal const val MAX_SLIDER_STOPS = 1000L

/**
 * A Slider `track`, drawn from the web recipe (×4): the thin `Slider/track`
 * bar, the `Slider/range` fill and the hollow `Slider/thumb` ring at the
 * value, with a drag that snaps to `step` (`sliderDrag`, `sliderRelease`
 * on lift). TalkBack adjusts it through `sliderSemantics`.
 */
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
                    // The thumb is centred on the value (web), overflowing
                    // the thin track frame vertically.
                    .absoluteOffset(x = (w * f - t / 2f).dp)
                    .wrapContentSize(unbounded = true)
                    .requiredSize(t.dp)
                    .then(if (thumb.style.shadows.isNotEmpty()) Modifier.shadow(2.dp, CircleShape) else Modifier)
                    .background(thumb.style.background ?: Color.White, CircleShape)
                    .then(if (border != null && thumb.style.borderWidth > 0f) Modifier.border(thumb.style.borderWidth.dp, border, CircleShape) else Modifier),
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
    if (component == "Select") return SelectTriggerContent(cx, modifier, if (placeholder) muted else null)
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

/**
 * #EXP-1629: a Select trigger. `field`: the `value` part (else the muted
 * placeholder) and the `chevron` part (`Select.trigger`). `row` (a Group
 * row): the `label` part leading, the value end-aligned and truncating,
 * then the `Select.chevron` glyph; the parts' opacity and size are the
 * recipe's.
 */
@Composable
private fun SelectTriggerContent(cx: LeafContext, modifier: Modifier, placeholderInk: Color?) {
    val row = selectRowVariant(cx.props, cx.ownerProps)
    val value = cx.part("Select", "value")
    val chevron = cx.part("Select", "chevron")
    val glyphSize = chevron.width ?: 16f
    // A field's glyph keeps its 60 % ink unless the recipe says otherwise.
    val glyphInk = (chevron.color ?: cx.ink).let { it.copy(alpha = it.alpha * (chevron.style.opacity ?: if (row) 1f else 0.6f)) }
    val valueInk = (placeholderInk ?: value.color ?: cx.ink).let { it.copy(alpha = it.alpha * (value.style.opacity ?: 1f)) }
    val r = cx.inner
    LeafFrame {
        Row(
            modifier.fillMaxSize().padding(start = r.left.dp, end = (cx.size.width - r.right).coerceAtLeast(0f).dp),
            horizontalArrangement = Arrangement.spacedBy(selectTriggerGap(cx.style.gap, cx.spacing("sm")).dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (row) {
                val label = cx.part("Select", "label")
                LeafLine(cx, selectRowLabel(cx.props, cx.ownerProps), color = label.color ?: cx.ink, ts = selectRowLabelStyle(cx.textStyle, label))
                LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f), color = valueInk, align = if (cx.rtl) TextAlign.Left else TextAlign.Right)
                IconOrGlyph(cx.props.str("icon").ifEmpty { builtinIcon("Select.chevron", "ui-chevron-right") }, Glyph.ChevronRight, glyphSize, cx, glyphInk)
            } else {
                LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f), color = valueInk)
                GlyphView(Glyph.ChevronsUpDown, glyphSize, glyphInk)
            }
        }
    }
}

/** #EXP-1629: is this Select a `row` trigger (the trigger's props, else the owner's)? */
internal fun selectRowVariant(props: Props, ownerProps: Props = emptyMap()): Boolean =
    props.str("triggerVariant").ifEmpty { ownerProps.str("triggerVariant") } == "row"

/** #EXP-1629: a `row` trigger's leading label (the core's, else the owner's `label`). */
internal fun selectRowLabel(props: Props, ownerProps: Props = emptyMap()): String =
    props.str("label").ifEmpty { ownerProps.str("label") }

/** #EXP-1629: the row label's text style: the trigger's at the `label` part's weight (regular in a row). */
internal fun selectRowLabelStyle(ts: ResolvedTextStyle, label: at.exponential.ui.theme.PartStyle): ResolvedTextStyle =
    ts.copy(fontWeight = label.style.fontWeight ?: 400)

/** The gap between a Select trigger's text and glyph (the recipe's, at least `$spacing.sm`; the measurer's rule). */
internal fun selectTriggerGap(recipeGap: Float, sm: Float): Float = max(recipeGap, max(sm, 0f))

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
 * Select whose recipe does not paint its popup opens our own popup
 * anchored here ([SelectPopup], #EXP-1629) and a DatePicker the M3 date
 * dialog (`DatePopup`); otherwise the core opens the owner's popup layer.
 */
@Composable
internal fun PickerTriggerLeaf(cx: LeafContext) {
    LeafFrame {
        TriggerContent(cx, Modifier.triggerGestures(cx))
        if (cx.node.component == "Select" && cx.model.nativePicker(cx.node)) SelectPopup(cx)
    }
}
