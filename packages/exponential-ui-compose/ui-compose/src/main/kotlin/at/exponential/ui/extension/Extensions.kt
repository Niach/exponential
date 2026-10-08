package at.exponential.ui.extension

import androidx.compose.runtime.Composable
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.measure.LeafRequest
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.fire
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.ResolvedTextStyle
import at.exponential.ui.theme.ThemeHandle

/** A leaf an extension painter measures. */
class ExtensionLeaf(val request: LeafRequest, val theme: ThemeHandle?, val mode: Mode) {
    /** The node's props. */
    val props: Props get() = request.props

    /** The resolved text style. */
    val textStyle: ResolvedTextStyle get() = request.textStyle
}

/**
 * What an extension painter receives to paint one node: the node, its
 * resolved props, the box visual, the frame size (dp), the theme and an
 * emitter for the node's `on` handlers.
 */
class ExtensionContext(
    val node: NodeInfo,
    val props: Props,
    val style: PaintStyle,
    val textStyle: ResolvedTextStyle,
    val ink: Color,
    val size: Size,
    val theme: ThemeHandle?,
    val mode: Mode,
    val model: SurfaceModel,
    /** The node's children, already rendered (container extensions). */
    val children: (@Composable () -> Unit)?,
) {
    /** Fire one of the node's `on` handlers. */
    fun emit(event: String, payload: JsonValue? = null) {
        model.fire(node.index, event, payload)
    }
}

/** A painter for one extension native (`extension_kind`). */
interface ExtensionPainter {
    /**
     * The border box (dp) of the leaf at a wrap width (null wrap =
     * max-content, 0 = min-content). null = 0×0. Called on the main thread
     * from inside the layout pass.
     */
    fun measure(leaf: ExtensionLeaf, wrap: Float?): Size?

    /** The content drawn INSIDE the frame. */
    @Composable
    fun Paint(context: ExtensionContext)
}

/**
 * The extension registry: catalog definitions (registered on every new
 * surface) and painters per kind.
 */
class ExtensionRegistry {
    private val defs = ArrayList<String>()
    private val painters = HashMap<String, ExtensionPainter>()

    /** The registered extension definitions (JSON). */
    val definitions: List<String> get() = synchronized(this) { defs.toList() }

    /** Add a definition. */
    fun register(definition: String) {
        synchronized(this) { defs.add(definition) }
    }

    /** Set the painter of `kind`. */
    fun register(kind: String, painter: ExtensionPainter) {
        synchronized(this) { painters[kind] = painter }
    }

    /** The painter of `kind`. */
    fun painter(kind: String): ExtensionPainter? = synchronized(this) { painters[kind] }

    /** Forget everything (tests). */
    fun reset() {
        synchronized(this) {
            defs.clear()
            painters.clear()
        }
    }

    companion object {
        /** The process-wide registry new surfaces read. */
        val shared = ExtensionRegistry()
    }
}
