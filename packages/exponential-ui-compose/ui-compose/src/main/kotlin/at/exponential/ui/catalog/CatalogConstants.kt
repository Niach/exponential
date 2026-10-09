package at.exponential.ui.catalog

import at.exponential.ui.ExponentialUICatalog

/**
 * The catalog numbers and lists this painter reads, typed views over the
 * generated catalog object ([ExponentialUICatalog], compiled in from
 * `packages/exponential-ui/generated/ExponentialUICatalog.generated.kt`):
 * a catalog change reaches the painter on the next build.
 */
object CatalogConstants {
    /** `catalog/layout.json` by name (round 2 §7). */
    val layout: Map<String, Float> = ExponentialUICatalog.layoutConstantNames.zip(ExponentialUICatalog.layoutConstantValues.map { it.toFloat() }).toMap()

    private fun named(name: String): Float = layout[name] ?: error("layout.json has no $name")

    val WINDOW_THRESHOLD: Int = named("windowThreshold").toInt()
    val WINDOW_OVERSCAN: Int = named("windowOverscan").toInt()
    val RESIZE_STEP: Float = named("resizeStep")
    val PANEL_MIN: Float = named("panelMin")
    val RESIZE_HANDLE_HIT: Float = named("resizeHandleHit")
    val FIELD_INTRINSIC_WIDTH: Float = named("fieldIntrinsicWidth")
    val MEDIA_INTRINSIC_WIDTH: Float = named("mediaIntrinsicWidth")
    val MEDIA_ASPECT_RATIO: Float = named("mediaAspectRatio")
    val TREE_GUIDE_COLUMN: Float = named("treeGuideColumn")

    /** `locale.json` `rtlMirroredIcons`: the directional glyphs mirrored under rtl (round 1 §4). */
    val rtlMirroredIcons: List<String> = ExponentialUICatalog.rtlMirroredIcons

    /** `style.json` `animations` names (round 2 §2). */
    val animationNames: List<String> = ExponentialUICatalog.animationNames
}
