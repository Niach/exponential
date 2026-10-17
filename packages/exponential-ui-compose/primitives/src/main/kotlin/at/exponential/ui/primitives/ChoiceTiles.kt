package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The look of [ChoiceTiles] (EXP-1280, the eUI core `ChoiceTiles` recipe and
 * `plan-card.json` `tiles` ×4): radius 10, gap 8, picture aspect 1.6 (w/h),
 * padding 8, a 2 accent ring on a picked tile, label over a muted description.
 */
@Immutable
data class ChoiceTilesStyle(
    val columns: Int = 2,
    val gap: Dp = 8.dp,
    val radius: Dp = 10.dp,
    val padding: Dp = 8.dp,
    /** The picture's width / height. */
    val pictureAspect: Float = 1.6f,
    val selectedRing: Dp = 2.dp,
    val tileFill: Color = Color.Gray.copy(alpha = 0.08f),
    val tileFillSelected: Color = Color.Gray.copy(alpha = 0.16f),
    val stroke: Color = Color.Gray.copy(alpha = 0.3f),
    val strokeWidth: Dp = 1.dp,
    val accent: Color = Color(0xFF007AFF),
    val pictureFill: Color = Color.Gray.copy(alpha = 0.15f),
    val label: Color = Color(0xFF1C1C1E),
    val description: Color = Color(0xFF8E8E93),
    val labelSize: TextUnit = 13.sp,
    val descriptionSize: TextUnit = 12.sp,
    /** The gap between the picture and the label. */
    val pictureGap: Dp = 6.dp,
) {
    companion object {
        /** The control on [tokens]: muted tiles, border hairlines, the primary ring. */
        fun default(tokens: PrimitiveTokens): ChoiceTilesStyle = ChoiceTilesStyle(
            tileFill = tokens.muted.copy(alpha = tokens.muted.alpha * 0.5f),
            tileFillSelected = tokens.accent,
            stroke = tokens.border,
            strokeWidth = tokens.hairline,
            accent = tokens.primary,
            pictureFill = tokens.muted,
            label = tokens.foreground,
            description = tokens.mutedForeground,
        )
    }
}

/** One option of [ChoiceTiles]. [hasPicture] = the tile shows a picture
 *  slot (filled by the control's `picture`, else the placeholder). */
@Immutable
data class ChoiceTile(
    val label: String,
    val description: String? = null,
    val hasPicture: Boolean = true,
    val disabled: Boolean = false,
    val testTag: String? = null,
)

/**
 * The picture choice (EXP-1280; eUI core `ChoiceTiles`): a grid of
 * `style.columns` equal tiles, each = the picture (`pictureAspect`, cropped,
 * the tile's radius less its padding) over the label and the muted
 * description; a picked tile wears the `selectedRing` accent ring. Single
 * ([multiple] false: radios) or several picks (checkboxes); [onToggle] gets
 * the option's index. A tile whose option has no picture keeps the frame
 * with [placeholder] in it, so a row keeps one height.
 */
@Composable
fun ChoiceTiles(
    tiles: List<ChoiceTile>,
    selected: Set<Int>,
    onToggle: (Int) -> Unit,
    modifier: Modifier = Modifier,
    multiple: Boolean = false,
    enabled: Boolean = true,
    style: ChoiceTilesStyle = ChoiceTilesStyle.default(LocalPrimitiveTokens.current),
    picture: @Composable (index: Int) -> Unit = {},
    placeholder: @Composable () -> Unit = {},
) {
    val columns = style.columns.coerceIn(1, 4)
    val anyPicture = tiles.any { it.hasPicture }
    Column(
        modifier = modifier.fillMaxWidth().selectableGroup(),
        verticalArrangement = Arrangement.spacedBy(style.gap),
    ) {
        tiles.indices.chunked(columns).forEach { row ->
            Row(
                modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Min),
                horizontalArrangement = Arrangement.spacedBy(style.gap),
            ) {
                row.forEach { index ->
                    val tile = tiles[index]
                    ChoiceTileView(
                        tile = tile,
                        picked = index in selected,
                        multiple = multiple,
                        enabled = enabled && !tile.disabled,
                        showPicture = anyPicture,
                        style = style,
                        onClick = { onToggle(index) },
                        picture = { if (tile.hasPicture) picture(index) else placeholder() },
                        modifier = Modifier.weight(1f).fillMaxHeight(),
                    )
                }
                repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun ChoiceTileView(
    tile: ChoiceTile,
    picked: Boolean,
    multiple: Boolean,
    enabled: Boolean,
    showPicture: Boolean,
    style: ChoiceTilesStyle,
    onClick: () -> Unit,
    picture: @Composable () -> Unit,
    modifier: Modifier,
) {
    val shape = RoundedCornerShape(style.radius)
    val inner = RoundedCornerShape((style.radius - style.padding / 2).coerceAtLeast(2.dp))
    val press = if (multiple) {
        Modifier.toggleable(value = picked, enabled = enabled, role = Role.Checkbox, onValueChange = { onClick() })
    } else {
        Modifier.selectable(selected = picked, enabled = enabled, role = Role.RadioButton, onClick = onClick)
    }
    Column(
        modifier = modifier
            .then(if (tile.testTag != null) Modifier.testTag(tile.testTag) else Modifier)
            .alpha(if (enabled || picked) 1f else 0.6f)
            .clip(shape)
            .background(if (picked) style.tileFillSelected else style.tileFill, shape)
            .border(
                if (picked) style.selectedRing else style.strokeWidth,
                if (picked) style.accent else style.stroke,
                shape,
            )
            .then(press)
            .padding(style.padding),
        verticalArrangement = Arrangement.Top,
    ) {
        if (showPicture) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(style.pictureAspect)
                    .clip(inner)
                    .background(style.pictureFill, inner),
                contentAlignment = Alignment.Center,
            ) { picture() }
            Spacer(Modifier.height(style.pictureGap))
        }
        BasicText(
            tile.label,
            style = TextStyle(fontSize = style.labelSize, fontWeight = FontWeight.Medium, color = style.label),
        )
        tile.description?.takeIf { it.isNotEmpty() }?.let { description ->
            BasicText(
                description,
                modifier = Modifier.padding(top = 2.dp),
                style = TextStyle(fontSize = style.descriptionSize, color = style.description),
            )
        }
    }
}
