package at.exponential.ui.model

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import at.exponential.ui.ffi.FfiFrame
import at.exponential.ui.ffi.FfiLayer
import at.exponential.ui.ffi.FfiList
import at.exponential.ui.ffi.FfiNode
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.str

/** A core frame as a Compose [Rect] in dp, surface coordinates. */
fun FfiFrame.toRect(): Rect = Rect(Offset(x, y), Size(w, h))

/**
 * One placed node of the surface (the facade's `FfiNode`, props parsed):
 * static per structure version, re-read after every interaction (the core
 * may re-resolve props and `hidden` without a version bump).
 */
class NodeInfo(n: FfiNode) {
    val index: Int = n.index.toInt()
    val id: String = n.id
    val component: String = n.component
    val part: String? = n.part
    val owner: String? = n.owner
    val ownerComponent: String? = n.ownerComponent
    val catalogId: String? = n.catalogId
    val extensionKind: String? = n.extensionKind
    val depth: Int = n.depth.toInt()
    val parent: Int? = n.parent?.toInt()
    val layer: Int = n.layer.toInt()
    val isLeaf: Boolean = n.isLeaf
    val props: Props = JsonValue.parse(n.propsJson).obj ?: emptyMap()
    val lines: Int? = n.lines?.toInt()
    val pressable: Boolean = n.pressable
    val hidden: Boolean = n.hidden
    val triggerFor: String? = n.triggerFor
    val accessibility: Props? = n.accessibilityJson?.let { JsonValue.parse(it).obj }

    /** Part states the core resolved (`selected`, `open`, `checked`). */
    val partStates: List<String> = n.partStates
    val macroName: String? = n.macroName

    /** Not a leaf. */
    val isContainer: Boolean get() = !isLeaf

    /** The component whose recipe this node paints with (the owner of a part). */
    val recipeComponent: String get() = ownerComponent ?: component
    val selected: Boolean get() = partStates.contains("selected")
    val open: Boolean get() = partStates.contains("open")
    val checked: Boolean get() = partStates.contains("checked")
    val disabled: Boolean get() = props.flag("disabled")

    /** Is this a host-owned text field (`Input`/`Textarea` `.field`, `Composer`)? */
    val isTextField: Boolean
        get() = (component == "Input" && part == "field") || (component == "Textarea" && part == "field") || (component == "Composer" && part == null)

    /** Keyboard-focusable: pressables, controls, fields, carousel dots. */
    val isFocusable: Boolean
        get() {
            if (hidden || disabled) return false
            if (pressable) return true
            return when (component) {
                "Button", "Link", "Toggle", "ToggleGroup", "Composer" -> true
                "Box" -> part == "indicator"
                else -> false
            }
        }

    /** The accessible name: `accessibility.label`, else text / label / alt. */
    val accessibilityLabel: String?
        get() {
            accessibility?.get("label")?.string?.takeIf { it.isNotEmpty() }?.let { return it }
            fun s(k: String): String? = props.str(k).ifEmpty { null }
            return when (component) {
                "Image", "Video" -> s("alt") ?: s("title")
                "Avatar" -> s("name")
                "Box", "Extension" -> s("label") ?: s("title")
                "Spinner" -> s("label") ?: "Loading"
                "Link" -> s("label") ?: s("href")
                "Chart" -> s("title") ?: s("kind")
                "Markdown" -> props.str("text").ifEmpty { null }
                else -> s("text") ?: s("label") ?: s("title") ?: s("placeholder") ?: s("alt")
            }
        }

    override fun toString(): String = "NodeInfo($index, $id, $component${part?.let { ".$it" } ?: ""})"
}

/** An open overlay layer (the facade's `FfiLayer`); frames in dp, surface coordinates. */
data class LayerInfo(
    val layer: Int,
    /** `Dialog` | `Drawer` | `Popover` | `Tooltip` | `DropdownMenu`. */
    val kind: String,
    val owner: String,
    val root: Int,
    val anchorFrame: Rect?,
    val placementSide: String?,
    val flipped: Boolean,
    /** `centered`, a viewport edge, or the side an anchored layer landed on. */
    val position: String,
    /** The root's frame. */
    val frame: Rect,
) {
    /** From the facade's record. */
    constructor(l: FfiLayer) : this(
        layer = l.layer.toInt(),
        kind = l.kind,
        owner = l.owner,
        root = l.root.toInt(),
        anchorFrame = l.anchorFrame?.toRect(),
        placementSide = l.placement?.side,
        flipped = l.placement?.flipped ?: false,
        position = l.position,
        frame = l.frames.firstOrNull { it.index == l.root }?.toRect() ?: Rect.Zero,
    )

    /** The layer id (= its owner). */
    val id: String get() = owner

    /** Dialog / Drawer. */
    val isModal: Boolean get() = kind == "Dialog" || kind == "Drawer"
}

/** A windowed list (the facade's `FfiList`). */
data class ListInfo(
    val id: String,
    val node: Int,
    /** The full content height (dp). */
    val contentHeight: Float,
    val start: Int,
    val end: Int,
    val count: Int,
    val windowed: Boolean,
) {
    /** From the facade's record. */
    constructor(l: FfiList) : this(l.id, l.node.toInt(), l.contentHeight, l.start.toInt(), l.end.toInt(), l.count.toInt(), l.windowed)
}

/** What one layout pass cost. */
data class PassStats(
    val layoutNs: Long = 0,
    val wallNs: Long = 0,
    val upcalls: Int = 0,
    val measureRounds: Int = 0,
    val measureCalls: Int = 0,
    val nodes: Int = 0,
)

/** The interaction states of one node (reported to the core as `states`). */
internal data class InteractionState(val hover: Boolean = false, val pressed: Boolean = false, val focus: Boolean = false) {
    val states: List<String>
        get() = buildList {
            if (hover) add("hover")
            if (pressed) add("pressed")
            if (focus) add("focus")
        }
}

/**
 * A local mirror of an UNBOUND control value (the core reflects only bound
 * ones): the local value stands while the prop keeps its external value.
 */
internal data class Mirror(val external: JsonValue, val local: JsonValue)
