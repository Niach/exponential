package com.exponential.app.ui.components

import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton

/** The two pill rungs — a 32dp control and a 24dp inline badge. */
enum class PillSize { Md, Sm }

/**
 * What a pill IS, which is the only thing that may change its chrome:
 * [Action] taps and does something, [Select] carries a [GlassPill] `selected`
 * state, [Readonly] is a label that happens to be capsule-shaped.
 */
enum class PillMode { Action, Select, Readonly }

/**
 * The ONE capsule on Android (EXP-698).
 *
 * Before this the app drew fourteen of them: `GlassPillButton`, `PropertyChip`,
 * `FilterPill`, `LabelChip`, `OriginChip`, `RepoNameChip`, `SuggestionKindChip`,
 * `NoticeChip`, `AgentTabChip`, the members role badge, the sync chips, plus
 * twenty-five bare `Modifier.glassButton()` rows — each with its own height,
 * padding, glyph size and label alpha, and one with a hand-typed `0.9f`
 * emphasis that exists nowhere else in the design system. They are all this
 * now; [GlassPillDefaults] pins the numbers and `GlassPillDefaultsTest` keeps
 * them from being re-typed at a call site.
 *
 * Chrome is a pure function of ([size], [mode], [selected], [primary]): the
 * capsule is always [GlassTokens.CardFill] behind a [GlassTokens.StrokeCard] hairline
 * with a secondary-emphasis label, and ONLY a selected [PillMode.Select] pill
 * brightens to the shared active fill + stroke and a primary label. A
 * [PillMode.Readonly] pill takes the same rest chrome, is not clickable and
 * shows no ripple. [primary] is the one emphatic paint — a solid capsule for
 * the single call to action on a surface.
 *
 * [leading] is a free slot (a progress ring, a status glyph, an avatar) drawn
 * in the pill's own content color; [icon] is the common case of a plain glyph.
 * [dot] draws the 6dp colour disc a label pill needs (the label palette's
 * colour, the agent's brand tint). [trailing] is the dismiss `×` of a filter
 * pill.
 */
@Composable
fun GlassPill(
    /** Empty renders an icon-only capsule. */
    label: String,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    size: PillSize = PillSize.Md,
    mode: PillMode = if (onClick == null) PillMode.Readonly else PillMode.Action,
    selected: Boolean = false,
    /**
     * PAINT, orthogonal to [size] and [mode] (EXP-698 r4 — web `primary`,
     * desktop `.primary()`, iOS `primary:`): the solid primary fill with
     * primary-foreground content and no hairline, for the ONE call to action
     * on a surface. A disabled pill ignores it and keeps the dimmed glass.
     */
    primary: Boolean = false,
    icon: ImageVector? = null,
    leading: (@Composable () -> Unit)? = null,
    dot: Color? = null,
    trailing: (@Composable () -> Unit)? = null,
    opaque: Boolean = false,
    enabled: Boolean = true,
    /** A call in flight: the glyph slot becomes a spinner, so the pill keeps
     *  its width and the caller only has to dim it via [enabled]. */
    loading: Boolean = false,
    /** Monospace for an identifier or a `owner/repo` name (web `font-mono`,
     *  iOS `.monospaced()`). Null keeps the rung's own family. */
    fontFamily: FontFamily? = null,
    /**
     * ONE line, ellipsized — the only value a fixed-height capsule can honour,
     * so it is the default and raising it is a caller's explicit choice on a
     * pill it has also given room to grow. Text that can genuinely need two
     * lines is a NOTICE, not a pill (see the sync/steer captions).
     */
    maxLines: Int = 1,
    /** Overrides the emphasis-derived content color — the ONE destructive
     *  pill (Delete team) is red on the same glass. Nothing else may use it. */
    contentColor: Color? = null,
    /** Names the pill for accessibility when the LABEL does not — the
     *  icon-only capsule, whose glyph is its whole meaning. */
    contentDescription: String? = null,
) {
    // A DISABLED select pill is never lit: the active fill plus a quaternary
    // label rendered as a bright capsule with near-invisible text in it.
    val active = enabled && mode == PillMode.Select && selected
    // The emphatic paint is for a LIVE call to action only: a disabled primary
    // capsule would read as tappable at full brightness.
    val emphatic = enabled && primary
    val fg = contentColor ?: if (emphatic) {
        DesignTokens.Palette.PrimaryForeground
    } else {
        MaterialTheme.colorScheme.onSurface.copy(
            alpha = when {
                !enabled -> TextEmphasis.Quaternary
                active -> TextEmphasis.Primary
                else -> TextEmphasis.Secondary
            },
        )
    }
    val tap = if (enabled && mode != PillMode.Readonly) onClick else null
    // An icon-only capsule insets by whatever centres its glyph, so it comes
    // out a CIRCLE. The label padding would have made it a stubby 28x24 oval.
    val iconOnly = label.isEmpty()
    // Aliased because inside `semantics {}` the bare name resolves to THIS
    // parameter, not to the receiver's property of the same name.
    val describedAs = contentDescription
    // Only a solid fill needs a press state of its own — the glass rungs get
    // theirs from the ripple, and collecting the interaction stream for them
    // would recompose fifty capsules for nothing.
    val interactionSource = remember { MutableInteractionSource() }
    val pressed = if (emphatic) interactionSource.collectIsPressedAsState().value else false
    Row(
        verticalAlignment = Alignment.CenterVertically,
        // Centred, so a pill told to fillMaxWidth (the one destructive
        // "Delete team" capsule) centres its content instead of hugging the
        // left edge; it is a no-op on a pill that wraps its content.
        horizontalArrangement = Arrangement.spacedBy(
            GlassPillDefaults.spacing(size),
            Alignment.CenterHorizontally,
        ),
        modifier = modifier
            .height(GlassPillDefaults.height(size))
            .glassButton(active = active, opaque = opaque, primary = emphatic, pressed = pressed)
            .then(
                if (tap != null) {
                    Modifier.clickable(
                        interactionSource = interactionSource,
                        // The primary capsule's press cue IS the fill dipping;
                        // a ripple over a solid fill only muddies it.
                        indication = if (emphatic) null else LocalIndication.current,
                        onClick = tap,
                    )
                } else {
                    Modifier
                },
            )
            .then(
                if (describedAs != null) {
                    Modifier.semantics { this.contentDescription = describedAs }
                } else {
                    Modifier
                },
            )
            .padding(
                horizontal = if (iconOnly) {
                    GlassPillDefaults.iconOnlyHorizontalPadding(size)
                } else {
                    GlassPillDefaults.horizontalPadding(size)
                },
            ),
    ) {
        val glyph = GlassPillDefaults.glyphSize(size)
        when {
            loading -> CircularProgressIndicator(
                modifier = Modifier.size(glyph),
                strokeWidth = 2.dp,
                color = fg,
            )
            leading != null -> CompositionLocalProvider(LocalContentColor provides fg) { leading() }
            icon != null -> Icon(icon, contentDescription = null, modifier = Modifier.size(glyph), tint = fg)
        }
        if (dot != null) {
            Box(
                Modifier
                    .size(GlassPillDefaults.DotSize)
                    .clip(CircleShape)
                    .background(dot, CircleShape),
            )
        }
        // An EMPTY label is the icon-only pill (the issue header's "edit
        // properties" `+`) — it keeps the capsule, drops the text and the
        // spacing that would otherwise sit beside nothing.
        if (!iconOnly) {
            Text(
                label,
                style = GlassPillDefaults.textStyle(size).let {
                    if (fontFamily != null) it.copy(fontFamily = fontFamily) else it
                },
                color = fg,
                maxLines = maxLines,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (trailing != null) CompositionLocalProvider(LocalContentColor provides fg) { trailing() }
    }
}

/**
 * The pill's own numbers (EXP-698), pinned by `GlassPillDefaultsTest` — the
 * capsule is drawn at fifty call sites and none of them may re-type a height,
 * a padding or a glyph size.
 */
object GlassPillDefaults {
    /** [PillSize.Md] — the control rung every other 32dp control sits on. */
    val MdHeight: Dp = GlassTokens.ControlSize
    val MdHorizontalPadding: Dp = 12.dp
    val MdSpacing: Dp = 6.dp
    val MdGlyphSize: Dp = 16.dp

    /** [PillSize.Sm] — an inline badge inside a list row or a title line. */
    val SmHeight: Dp = 24.dp
    val SmHorizontalPadding: Dp = 8.dp
    val SmSpacing: Dp = 4.dp
    val SmGlyphSize: Dp = 12.dp

    /** The colour disc of a label / agent pill. */
    val DotSize: Dp = 6.dp

    /**
     * A person in a pill (the assignee chip). An avatar is a face, not a
     * glyph: at the 12dp glyph rung its initials fell to ~5sp and stopped
     * being readable, so it gets its own slightly larger rung.
     */
    val AvatarSize: Dp = 18.dp

    fun height(size: PillSize): Dp = if (size == PillSize.Md) MdHeight else SmHeight

    fun horizontalPadding(size: PillSize): Dp =
        if (size == PillSize.Md) MdHorizontalPadding else SmHorizontalPadding

    fun spacing(size: PillSize): Dp = if (size == PillSize.Md) MdSpacing else SmSpacing

    fun glyphSize(size: PillSize): Dp = if (size == PillSize.Md) MdGlyphSize else SmGlyphSize

    /** Whatever centres the glyph — an icon-only pill is a circle, not an oval. */
    fun iconOnlyHorizontalPadding(size: PillSize): Dp = (height(size) - glyphSize(size)) / 2

    /** Medium weight on both rungs — a pill never announces itself by weight. */
    @Composable
    fun textStyle(size: PillSize): TextStyle = when (size) {
        PillSize.Md -> MaterialTheme.typography.labelLarge
        PillSize.Sm -> MaterialTheme.typography.labelMedium
    }.copy(fontWeight = FontWeight.Medium)
}
