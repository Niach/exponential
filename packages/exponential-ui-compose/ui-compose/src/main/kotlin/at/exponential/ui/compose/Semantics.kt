package at.exponential.ui.compose

import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.CollectionItemInfo
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsPropertyReceiver
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.collectionItemInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.error
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.semantics.traversalIndex
import androidx.compose.ui.state.ToggleableState
import at.exponential.ui.json.Props
import at.exponential.ui.json.num
import at.exponential.ui.json.flag
import at.exponential.ui.json.str
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.checked
import at.exponential.ui.model.nodeKey
import at.exponential.ui.model.press
import at.exponential.ui.model.radioChecked
import at.exponential.ui.model.sliderDrag
import at.exponential.ui.model.sliderRelease
import at.exponential.ui.model.sliderValue
import at.exponential.ui.primitives.Markdown

/**
 * The reading order of a node (the VAPP-4 TalkBack fix): EVERY node is a
 * traversal group ordered by its pre-order index, so TalkBack reads the
 * surface in the core's order whatever the frames' geometry says.
 */
internal fun Modifier.traversal(index: Int): Modifier = semantics {
    traversalIndex = index.toFloat()
    isTraversalGroup = true
}

/** A node TalkBack skips (decoration, an unlabelled icon, the scrims). */
internal fun Modifier.hiddenFromAccessibility(): Modifier = clearAndSetSemantics { }

/** The node's `accessibility` (macro `$a11y`, the author's, the core's defaults: round 1 §6). */
private class A11y(val p: Props?) {
    val role: String? get() = p?.get("role")?.string
    val label: String? get() = p?.get("label")?.string?.takeIf { it.isNotEmpty() }
    val description: String? get() = p?.get("description")?.string?.takeIf { it.isNotEmpty() }
    val hidden: Boolean get() = p?.get("hidden")?.bool == true || role == "hidden"
    val level: Int? get() = p?.get("level")?.number?.toInt()
    val expanded: Boolean? get() = p?.get("expanded")?.bool
    val pressed: Boolean? get() = p?.get("pressed")?.bool
    val checked: Boolean? get() = p?.get("checked")?.bool
    val selected: Boolean? get() = p?.get("selected")?.bool
    val current: Boolean get() = p?.get("current")?.let { it.string != null || it.bool == true } ?: false
    val valueNow: Double? get() = p?.get("valueNow")?.number
    val valueMin: Double? get() = p?.get("valueMin")?.number
    val valueMax: Double? get() = p?.get("valueMax")?.number
    val posInSet: Int? get() = p?.get("posInSet")?.number?.toInt()
    val setSize: Int? get() = p?.get("setSize")?.number?.toInt()
}

/** A label with its description (Compose has no separate description: TalkBack reads them together). */
private fun named(label: String?, description: String?): String? = when {
    label == null -> description
    description == null -> label
    else -> "$label, $description"
}

/** The semantics every node's `accessibility` adds on top of its component's (headings, states, values, live regions, set positions). */
private fun SemanticsPropertyReceiver.applyA11y(a: A11y, live: String?, model: SurfaceModel) {
    when (a.role) {
        "heading" -> heading()
        "button", "link" -> role = Role.Button
        "checkbox" -> role = Role.Checkbox
        "switch" -> role = Role.Switch
        "radio" -> role = Role.RadioButton
        "img" -> role = Role.Image
        "combobox" -> role = Role.DropdownList
        "dialog", "alertdialog" -> a.label?.let { paneTitle = it }
    }
    a.pressed?.let { toggleableState = ToggleableState(it) }
    a.checked?.let { toggleableState = ToggleableState(it) }
    a.selected?.let { selected = it }
    if (a.current) selected = true
    val now = a.valueNow
    if (now != null && (a.role == "progressbar" || a.role == "meter" || a.role == "slider" || a.role == "separator")) {
        val lo = (a.valueMin ?: 0.0).toFloat()
        val hi = maxOf((a.valueMax ?: 100.0).toFloat(), lo)
        progressBarRangeInfo = ProgressBarRangeInfo(now.toFloat().coerceIn(lo, hi), lo..hi)
    }
    when (live ?: if (a.role == "alert") "assertive" else if (a.role == "status") "polite" else null) {
        "assertive" -> liveRegion = LiveRegionMode.Assertive
        "polite" -> liveRegion = LiveRegionMode.Polite
    }
    val pos = a.posInSet
    if (pos != null) collectionItemInfo = CollectionItemInfo(rowIndex = pos - 1, rowSpan = 1, columnIndex = 0, columnSpan = 1)
}

/**
 * The accessibility shape of a node: `accessibility.hidden` (and
 * `visibility: hidden`) leave the tree; pressable containers become ONE
 * button named by `accessibility.label` else their leaves
 * (`combinedLabel`, the children cleared); landmarks, groups, headings,
 * progress bars and live regions carry their label, role, level, value
 * and live mode; leaves their component's role, label and state. Text
 * fields, picker triggers and Segmented controls keep the semantics their own
 * composables set. Called during composition: every value it reads is
 * captured there.
 */
internal fun Modifier.nodeSemantics(node: NodeInfo, model: SurfaceModel): Modifier {
    val a = A11y(node.accessibility)
    if (a.hidden || model.style(node.index).visibilityHidden) return hiddenFromAccessibility()
    val live = node.live?.takeIf { it != "off" }
    if (!node.isLeaf) {
        if (node.pressable) {
            val label = named(a.label ?: model.combinedLabel(node.index), a.description)
            val index = node.index
            val disabledNow = model.isDisabled(index)
            val menuKind = if (node.ownerComponent == "Menu" && node.part == "item") node.props.str("kind") else ""
            val menuChecked = node.checked || node.props.flag("checked")
            val menuOpen = node.open
            return clearAndSetSemantics {
                contentDescription = label ?: ""
                role = Role.Button
                // The press (the clickable's own action is cleared with its children's).
                if (disabledNow) disabled() else onClick { model.press(index); true }
                applyA11y(a, live, model)
                if (a.role == "none" || a.role == null) role = Role.Button
                when (a.expanded) {
                    true -> collapse { model.press(index); true }
                    false -> expand { model.press(index); true }
                    null -> {}
                }
                // Round 3 Menu rows: a checkbox row is a checkbox (its state),
                // a submenu row expands / collapses its submenu (gpui: menuitemcheckbox).
                if (menuKind == "checkbox") {
                    role = Role.Checkbox
                    toggleableState = ToggleableState(menuChecked)
                } else if (menuKind == "submenu") {
                    if (menuOpen) collapse { model.press(index); true } else expand { model.press(index); true }
                }
            }
        }
        if (node.ownerComponent == "Resizable" && node.part == "handle") return resizeHandleSemantics(node, model, a)
        val label = named(a.label, a.description) ?: if (node.macroName == "Card" || node.macroName == "Group" || node.macroName == "Alert" || node.part == "content") node.accessibilityLabel else null
        if (label == null && node.accessibility == null && live == null) return this
        return semantics {
            label?.let { contentDescription = it }
            applyA11y(a, live, model)
        }
    }
    return leafSemantics(node, model, a, live)
}

private fun Modifier.leafSemantics(node: NodeInfo, model: SurfaceModel, a: A11y, live: String?): Modifier {
    val label = a.label ?: node.accessibilityLabel
    val c = node.component
    val p = node.part
    val owner = node.ownerComponent
    return when {
        node.isTextField -> fieldErrorSemantics(node, model)
        c == "Image" || c == "Avatar" || c == "Video" || c == "Chart" -> {
            val alt = label ?: chartSummary(node, model)
            if (c == "Image" && alt.isNullOrEmpty()) hiddenFromAccessibility() else clearAndSetSemantics {
                contentDescription = alt ?: ""
                role = Role.Image
            }
        }
        c == "Icon" -> if (node.props["label"] != null || a.label != null) {
            clearAndSetSemantics {
                contentDescription = label ?: ""
                role = Role.Image
            }
        } else {
            hiddenFromAccessibility()
        }
        c == "Markdown" -> {
            val text = Markdown.plainText(node.props.str("text"))
            clearAndSetSemantics {
                contentDescription = text
                applyA11y(a, live, model)
            }
        }
        c == "Skeleton" || c == "TreeGuides" || (c == "List" && p == "divider") -> hiddenFromAccessibility()
        owner == "Tabs" && p == "tab" -> {
            val isSelected = node.selected
            val index = node.index
            clearAndSetSemantics {
                contentDescription = label ?: ""
                role = Role.Tab
                selected = isSelected
                onClick { model.press(index); true }
            }
        }
        owner == "Accordion" && p == "trigger" -> {
            val open = node.open
            val index = node.index
            clearAndSetSemantics {
                contentDescription = named(label, node.props["count"]?.displayText) ?: ""
                role = Role.Button
                if (open) collapse { model.press(index); true } else expand { model.press(index); true }
            }
        }
        (c == "Checkbox" && (p == "box" || p == "checkbox")) || (c == "Switch" && p == "track") -> {
            val on = model.checked(node.index)
            val name = a.label ?: node.props.str("label").ifEmpty { model.ownerProps(node.index).str("label") }
            val disabledNow = model.isDisabled(node.index)
            val index = node.index
            clearAndSetSemantics {
                contentDescription = name
                role = if (c == "Switch") Role.Switch else Role.Checkbox
                toggleableState = ToggleableState(on)
                if (disabledNow) disabled() else onClick { model.press(index); true }
            }
        }
        c == "Radio" && p == "dot" -> {
            val on = model.radioChecked(node.index)
            val name = radioLabel(node, model)
            clearAndSetSemantics {
                contentDescription = name
                role = Role.RadioButton
                selected = on
            }
        }
        c == "Slider" && p == "track" -> sliderSemantics(node, model)
        c == "Spinner" || c == "Ring" -> clearAndSetSemantics {
            contentDescription = label ?: model.string("loading")
            if (c == "Ring") node.props.num("value")?.let { v -> progressBarRangeInfo = ProgressBarRangeInfo(v.toFloat().coerceIn(0f, 1f), 0f..1f) } else progressBarRangeInfo = ProgressBarRangeInfo.Indeterminate
        }
        node.isPickerTrigger || c == "Segmented" -> this
        c == "Link" -> {
            val index = node.index
            clearAndSetSemantics {
                contentDescription = label ?: ""
                role = Role.Button
                onClick { model.press(index); true }
            }
        }
        label != null -> {
            val button = node.pressable || c == "Button" || c == "Toggle"
            val desc = named(label, a.description)
            val disabledNow = model.isDisabled(node.index)
            val index = node.index
            val pressable = node.pressable
            clearAndSetSemantics {
                contentDescription = desc ?: ""
                if (button) role = Role.Button
                if (pressable && !disabledNow) onClick { model.press(index); true }
                if (c == "Toggle") toggleableState = ToggleableState(node.props["pressed"]?.bool == true)
                applyA11y(a, live, model)
                if (button && disabledNow) disabled()
            }
        }
        else -> hiddenFromAccessibility()
    }
}

/** A text field's failing checks (`<owner>.error` nodes, round 1 §3 Form) as its error. */
private fun Modifier.fieldErrorSemantics(node: NodeInfo, model: SurfaceModel): Modifier {
    val owner = model.owner(node.index) ?: return this
    val errors = model.nodes.filter { it.id == "${owner.id}.error" || it.id.startsWith("${owner.id}.error.") }.mapNotNull { it.props["text"]?.displayText?.takeIf { t -> t.isNotEmpty() } }
    if (errors.isEmpty()) return this
    // One error per line: TalkBack pauses between them in any language
    // (no Latin punctuation).
    val text = errors.joinToString("\n")
    return semantics { error(text) }
}

/** A Resizable handle (round 2 §1): a separator named `$string.resize` whose value is the panel before it; TalkBack's adjust steps it. */
private fun Modifier.resizeHandleSemantics(node: NodeInfo, model: SurfaceModel, a: A11y): Modifier {
    val index = node.index
    val now = (a.valueNow ?: node.props.num("valueNow") ?: 0.0).toFloat()
    val lo = (a.valueMin ?: node.props.num("valueMin") ?: 0.0).toFloat()
    val hi = maxOf((a.valueMax ?: node.props.num("valueMax") ?: 100.0).toFloat(), lo)
    val horizontalLine = node.props["orientation"]?.string == "horizontal"
    val rtl = model.direction == "rtl"
    return clearAndSetSemantics {
        contentDescription = a.label ?: node.props.str("label").ifEmpty { model.string("resize") }
        progressBarRangeInfo = ProgressBarRangeInfo(now.coerceIn(lo, hi), lo..hi)
        setProgress { target ->
            val grow = target > now
            val key = when {
                horizontalLine -> if (grow) "ArrowDown" else "ArrowUp"
                grow != rtl -> "ArrowRight"
                else -> "ArrowLeft"
            }
            model.nodeKey(index, key)
            true
        }
    }
}

/**
 * A Chart without a title: the core's summary strings (`chartSummary` /
 * `sparklineSummary`) filled as the TS reference fills them
 * (`chartSummaryParams`: the series names ", "-joined, the finite extent),
 * every number through the surface formatter.
 */
private fun chartSummary(node: NodeInfo, model: SurfaceModel): String? {
    val chart = at.exponential.ui.paint.ChartModel(node.props)
    val values = chart.series.flatMap { it.values }.filter { it.isFinite() }
    if (values.isEmpty()) return null
    val fmt = { v: Double -> at.exponential.ui.paint.ChartModel.format(v, model.formatter) }
    return if (chart.spark) {
        model.string("sparklineSummary", mapOf("first" to fmt(values.first()), "last" to fmt(values.last()), "min" to fmt(values.min()), "max" to fmt(values.max())))
    } else {
        val (series, min, max) = chart.summaryParams()
        model.string("chartSummary", mapOf("series" to series, "min" to fmt(min), "max" to fmt(max)))
    }
}

/** A Slider track: the range + value, adjustable by TalkBack (snapped, then released = `change`). */
private fun Modifier.sliderSemantics(node: NodeInfo, model: SurfaceModel): Modifier {
    val min = node.props.num("min") ?: 0.0
    val max = node.props.num("max") ?: 100.0
    val step = node.props.num("step") ?: 1.0
    val value = model.sliderValue(node.index)
    val name = model.ownerProps(node.index).str("label")
    val steps = if (step > 0 && max > min) ((max - min) / step).toInt() - 1 else 0
    val index = node.index
    val disabledNow = model.isDisabled(index)
    return clearAndSetSemantics {
        contentDescription = name
        stateDescription = at.exponential.ui.paint.ChartModel.format(value, model.formatter)
        progressBarRangeInfo = ProgressBarRangeInfo(
            current = value.toFloat(),
            range = min.toFloat()..kotlin.math.max(min, max).toFloat(),
            steps = steps.coerceAtLeast(0),
        )
        if (disabledNow) {
            disabled()
        } else {
            setProgress { target ->
                val v = SurfaceModel.snap(target.toDouble(), min, max, step)
                model.sliderDrag(index, v)
                model.sliderRelease(index)
                true
            }
        }
    }
}

/** A Radio dot's name: the text of its row's label (`<owner>.label.<suffix>`). */
private fun radioLabel(node: NodeInfo, model: SurfaceModel): String {
    val suffix = node.id.split('.').lastOrNull() ?: ""
    val owner = model.owner(node.index)
    return model.indexOf("${owner?.id ?: ""}.label.$suffix")?.let(model::node)?.props?.str("text") ?: ""
}
