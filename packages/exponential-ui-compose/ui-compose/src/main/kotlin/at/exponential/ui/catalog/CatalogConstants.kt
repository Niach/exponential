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
    /** The tree-guide elbow's corner radius (round 3). */
    val TREE_GUIDE_RADIUS: Float = named("treeGuideRadius")
    /** How far every guide vertical overshoots the row's top (paint only, round 3: bridges a Section divider). */
    val TREE_GUIDE_BRIDGE: Float = named("treeGuideBridge")
    /** EXP-1259: how long a `loading` Button looks idle (ms). */
    val PENDING_DELAY_MS: Long = named("pendingDelayMs").toLong()
    /** EXP-1259: the Skeleton's ONE pulse (ms, low → high → low) and its opacities. */
    val SKELETON_PULSE_MS: Int = named("skeletonPulseMs").toInt()
    val SKELETON_OPACITY_LOW: Float = named("skeletonOpacityLow")
    val SKELETON_OPACITY_HIGH: Float = named("skeletonOpacityHigh")
    val SKELETON_REDUCED_OPACITY: Float = named("skeletonReducedOpacity")
    /** #EXP-1629: a `row` Select's popup width (dp) when `popupWidth` is unset. */
    val SELECT_ROW_POPUP_WIDTH: Float = named("selectRowPopupWidth")
    /** #EXP-1629: how far under its value a `row` Select's popup hangs, and how close to the window's edges it may go (dp). */
    val SELECT_ROW_POPUP_OFFSET: Float = named("selectRowPopupOffset")
    val SELECT_ROW_POPUP_MARGIN: Float = named("selectRowPopupMargin")
    /** EXP-1259: the bottom fade over a list while more rows sit below (dp; #EXP-1629: a Select's popup too). */
    val SEARCH_LIST_FADE: Float = named("searchListFade")

    /** `locale.json` `rtlMirroredIcons`: the directional glyphs mirrored under rtl (round 1 §4). */
    val rtlMirroredIcons: List<String> = ExponentialUICatalog.rtlMirroredIcons

    /** #EXP-1764: the icons painted in their OWN colour (`#RRGGBB`, `brand-claude` = Anthropic's orange), never the ink. */
    val iconFills: Map<String, Long> = ExponentialUICatalog.iconFillNames.zip(ExponentialUICatalog.iconFillColors.map { 0xFF000000L or it.removePrefix("#").toLong(16) }).toMap()

    /** `style.json` `animations` names (round 2 §2). */
    val animationNames: List<String> = ExponentialUICatalog.animationNames
}
