package at.exponential.ui.compose

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.min

/**
 * The content of a measured leaf, dispatched by (component, part), the
 * SwiftUI painter's table. Each leaf fills the core's frame and paints
 * inside [LeafContext.inner]; geometry-mode natives without a painter are
 * bare boxes.
 */
@Composable
fun LeafContent(context: LeafContext) {
    val cx = context
    val n = cx.node
    when (n.component) {
        "Extension" -> ExtensionLeaf(cx)
        "Text" -> TextPart(cx)
        "Markdown" -> MarkdownLeaf(cx)
        "Button", "Toggle" -> ButtonLeaf(cx)
        "Link" -> LinkLeaf(cx)
        "Icon" -> IconLeaf(cx)
        "Avatar" -> AvatarLeaf(cx)
        "Image" -> ImageLeaf(cx)
        "Video" -> VideoLeaf(cx)
        "AudioPlayer" -> AudioLeaf(cx)
        "Spinner" -> LeafFrame {
            SpinnerView(min(cx.inner.width, cx.inner.height), cx.ink, Modifier.align(Alignment.Center))
        }
        "Ring" -> RingLeaf(cx)
        "Skeleton" -> SkeletonLeaf(cx)
        "Chart" -> ChartLeaf(cx)
        "TreeGuides" -> TreeGuidesLeaf(cx)
        "Unknown" -> UnknownLeaf(cx)
        "Box" -> if (n.part == "indicator") CarouselIndicatorLeaf(cx)
        "Input" -> if (n.part == "field") TextFieldLeaf(cx, multiline = false)
        "Textarea" -> if (n.part == "field") TextFieldLeaf(cx, multiline = true)
        "Composer" -> ComposerLeaf(cx)
        // Round 1: the pickers' `trigger` part, the inline fields' `input` / `search`.
        "Select", "DatePicker", "TimePicker", "DateRangePicker" -> when {
            n.isPickerTrigger -> PickerTriggerLeaf(cx)
            n.isInlineField -> InlineFieldLeaf(cx)
        }
        "NumberField", "ChipInput" -> if (n.isInlineField) InlineFieldLeaf(cx)
        "Checkbox" -> if (n.part == "box") CheckboxBox(cx)
        "Radio" -> if (n.part == "dot") RadioDot(cx)
        "Switch" -> if (n.part == "track") SwitchTrack(cx)
        "Slider" -> if (n.part == "track") SliderTrack(cx)
        "Segmented" -> SegmentedLeaf(cx)
        // Geometry mode (no theme) and parts without content: bare boxes.
        else -> Unit
    }
}

/** An `Extension` leaf: its registered painter (the surface's registry), else the kind name as a muted note. */
@Composable
private fun ExtensionLeaf(cx: LeafContext) {
    val kind = cx.node.extensionKind ?: ""
    val painter = cx.model.extensions.painter(kind)
    if (painter != null) {
        painter.Paint(
            ExtensionContext(
                node = cx.node,
                props = cx.props,
                style = cx.style,
                textStyle = cx.textStyle,
                ink = cx.ink,
                size = cx.size,
                theme = cx.model.effectiveTheme,
                mode = cx.model.mode,
                model = cx.model,
                children = null,
            ),
        )
    } else {
        Box(Modifier.fillMaxSize()) {
            BasicText(
                kind,
                style = cx.composeTextStyle(ts = ResolvedTextStyle(12f, 400, 16f, null), color = cx.ink.copy(alpha = cx.ink.alpha * 0.6f)),
            )
        }
    }
}
