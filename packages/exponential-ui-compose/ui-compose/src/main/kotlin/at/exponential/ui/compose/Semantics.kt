package at.exponential.ui.compose

import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.semantics.traversalIndex
import androidx.compose.ui.state.ToggleableState
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.checked
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

/**
 * The accessibility shape of a node (the SwiftUI `AccessibilityModifier`):
 * pressable containers become ONE button named by their leaves
 * (`combinedLabel`, the children cleared, like SwiftUI's
 * `.accessibilityElement(children: .ignore)`), Card / Group / Alert /
 * `.content` containers a labelled group, leaves carry their role, label
 * and state. Text fields, picker triggers and ToggleGroups keep the
 * semantics their own composables set. Called during
 * composition: every value it reads is captured there.
 */
internal fun Modifier.nodeSemantics(node: NodeInfo, model: SurfaceModel): Modifier {
    if (!node.isLeaf) {
        if (node.pressable) {
            val label = model.combinedLabel(node.index)
            return clearAndSetSemantics {
                contentDescription = label
                role = Role.Button
            }
        }
        if (node.macroName == "Card" || node.macroName == "Group" || node.macroName == "Alert" || node.part == "content") {
            val label = node.accessibilityLabel
            if (!label.isNullOrEmpty()) return semantics { contentDescription = label }
        }
        return this
    }
    return leafSemantics(node, model)
}

private fun Modifier.leafSemantics(node: NodeInfo, model: SurfaceModel): Modifier {
    val label = node.accessibilityLabel
    val c = node.component
    val p = node.part
    return when {
        node.isTextField -> this
        c == "Image" || c == "Avatar" || c == "Video" || c == "Chart" -> clearAndSetSemantics {
            contentDescription = label ?: "image"
            role = Role.Image
        }
        c == "Icon" -> if (label != null) {
            clearAndSetSemantics {
                contentDescription = label
                role = Role.Image
            }
        } else {
            hiddenFromAccessibility()
        }
        c == "Markdown" -> {
            val text = Markdown.plainText(node.props.str("text"))
            clearAndSetSemantics { contentDescription = text }
        }
        c == "Skeleton" || c == "TreeGuides" || (c == "List" && p == "divider") -> hiddenFromAccessibility()
        c == "Text" && p == "tab" -> {
            val isSelected = node.selected
            clearAndSetSemantics {
                contentDescription = label ?: ""
                role = Role.Tab
                selected = isSelected
            }
        }
        c == "Text" && p == "trigger" -> {
            val open = node.open
            clearAndSetSemantics {
                contentDescription = label ?: ""
                role = Role.Button
                stateDescription = if (open) "expanded" else "collapsed"
            }
        }
        (c == "Checkbox" && p == "box") || (c == "Switch" && p == "track") -> {
            val on = model.checked(node.index)
            val name = model.ownerProps(node.index).str("label")
            val disabledNow = model.isDisabled(node.index)
            clearAndSetSemantics {
                contentDescription = name
                role = if (c == "Switch") Role.Switch else Role.Checkbox
                toggleableState = ToggleableState(on)
                if (disabledNow) disabled()
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
        c == "Spinner" || c == "Ring" -> clearAndSetSemantics { contentDescription = label ?: "Loading" }
        node.isPickerTrigger || c == "ToggleGroup" -> this
        c == "Link" -> clearAndSetSemantics {
            contentDescription = label ?: ""
            role = Role.Button
        }
        label != null -> {
            val button = node.pressable || c == "Button" || c == "Toggle"
            clearAndSetSemantics {
                contentDescription = label
                if (button) role = Role.Button
            }
        }
        else -> hiddenFromAccessibility()
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
        stateDescription = value.toInt().toString()
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
