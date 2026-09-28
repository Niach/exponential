package com.exponential.app.ui.spike

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.traversalIndex
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.InitialsAvatar
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.glassSwitchColors
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassCard
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Echo delay of the fake host (LANES.md, the typing test). */
private const val ECHO_DELAY_MS = 150L

@Composable
fun VappNode(node: VappNodeModel, state: VappSurfaceState) {
    if (node.isContainer) {
        VappContainer(node, state)
        return
    }
    val clip = Modifier.clipToAncestors(node, state)
    when (node.kind) {
        "text" -> VappText(node, state, clip)
        "button" -> VappButton(node, state, clip)
        "textfield" -> if (node.bool("echo")) VappEchoField(node, clip) else VappTextField(node, clip, multiLine = false)
        "textarea" -> VappTextField(node, clip, multiLine = true)
        "toggle" -> VappToggle(node, clip)
        "select" -> VappSelect(node, clip)
        "listrow" -> VappListRow(node, clip)
        "badge" -> VappBadge(node, clip)
        "pill" -> VappPill(node, clip)
        "avatar" -> InitialsAvatar(
            nameOrEmail = node.str("name"),
            size = node.num("size", 32.0).toFloat().dp,
            userId = node.str("name"),
            modifier = clip.semantics { contentDescription = node.str("name") ?: "Avatar" },
        )
        "image" -> VappImage(node, clip)
        "divider" -> Box(clip) { GroupDivider() }
        "progress" -> {
            val value = node.num("value", 0.0).toFloat().coerceIn(0f, 1f)
            LinearProgressIndicator(
                progress = { value },
                modifier = clip
                    .height(8.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .semantics { contentDescription = "Progress ${(value * 100).toInt()}%" },
                color = DesignTokens.Palette.Primary,
                trackColor = GlassTokens.RowFillActive,
                gapSize = 0.dp,
                drawStopIndicator = {},
            )
        }
        "markdown" -> MarkdownView(markdown = node.str("text") ?: "", modifier = clip)
        else -> Box(clip)
    }
}

/**
 * A container is a sibling background box (children are NOT nested in it: the
 * surface Layout places every node itself). Its paint comes from the frame's
 * visual, read in the draw phase.
 */
@Composable
private fun VappContainer(node: VappNodeModel, state: VappSurfaceState) {
    val base = if (node.kind == "card") Modifier.glassCard() else Modifier
    Box(
        base
            .clipToAncestors(node, state)
            .drawBehind {
                @Suppress("UNUSED_VARIABLE")
                val version = state.visualVersion // observe (draw-phase only)
                val v = state.visuals[node.index] ?: return@drawBehind
                val r = (v.borderRadius ?: 0f).dp.toPx()
                val bg = vappColor(v.backgroundColor)
                if (bg != null) {
                    drawRoundRect(color = bg, cornerRadius = CornerRadius(r, r))
                }
                val bw = v.borderWidth ?: 0f
                val bc = vappColor(v.borderColor)
                if (bw > 0f && bc != null) {
                    val sw = bw.dp.toPx()
                    drawRoundRect(
                        color = bc,
                        topLeft = Offset(sw / 2, sw / 2),
                        size = Size(size.width - sw, size.height - sw),
                        cornerRadius = CornerRadius(r, r),
                        style = Stroke(sw),
                    )
                }
            },
    )
}

/**
 * Children are flat siblings, so a parent's `overflow: hidden` + radius cannot
 * clip them structurally: each descendant clips itself (draw phase) to every
 * overflow-hidden ancestor's rounded rect, translated into its own space.
 */
private fun Modifier.clipToAncestors(node: VappNodeModel, state: VappSurfaceState): Modifier {
    // Every node's outermost modifier: TalkBack reads the surface in fixture
    // PRE-order (= node index) instead of Compose's geometric sort, which put
    // the vertically centred "Scan now" before the header subtitle. The
    // surface is the traversal group (VappSurface). Each node is ALSO a group:
    // traversalIndex sorts the group's flattened descendants, so a leaf whose
    // focusable node is an inner Text (badge, pill, field, markdown) would
    // otherwise sort at the default index 0, ahead of everything.
    val ordered = this.semantics {
        traversalIndex = node.index.toFloat()
        isTraversalGroup = true
    }
    val clippers = generateSequence(node.parent) { state.nodes[it].parent }.toList()
    if (clippers.isEmpty()) return ordered
    return ordered.drawWithContent {
        @Suppress("UNUSED_VARIABLE")
        val version = state.visualVersion
        val me = state.framesPx[node.index]
        val path = clippers.firstNotNullOfOrNull { a ->
            val v = state.visuals[a]
            val f = state.framesPx[a]
            if (v?.overflowHidden == true && f != null && me != null) {
                val r = (v.borderRadius ?: 0f).dp.toPx()
                Path().apply {
                    addRoundRect(
                        RoundRect(
                            left = f.x - me.x,
                            top = f.y - me.y,
                            right = f.x - me.x + f.w,
                            bottom = f.y - me.y + f.h,
                            cornerRadius = CornerRadius(r, r),
                        ),
                    )
                }
            } else {
                null
            }
        }
        if (path == null) drawContent() else clipPath(path) { this@drawWithContent.drawContent() }
    }
}

@Composable
private fun VappText(node: VappNodeModel, state: VappSurfaceState, modifier: Modifier) {
    val spec = state.textSpecs.getOrNull(node.index) ?: VappTextSpec(14f, 400, 20f, null, null)
    val variant = node.str("variant") ?: "body"
    val muted = variant == "muted" || variant == "caption"
    val color = vappColor(spec.color)
        ?: MaterialTheme.colorScheme.onSurface.copy(
            alpha = if (muted) TextEmphasis.Secondary else TextEmphasis.Primary,
        )
    Text(
        text = node.str("text") ?: "",
        modifier = modifier,
        color = color,
        fontSize = spec.fontSize.sp,
        fontWeight = FontWeight(spec.fontWeight.coerceIn(1, 1000)),
        lineHeight = spec.lineHeight.sp,
        textAlign = when (spec.textAlign) {
            "center" -> TextAlign.Center
            "right", "end" -> TextAlign.End
            "left", "start" -> TextAlign.Start
            else -> TextAlign.Unspecified
        },
    )
}

@Composable
private fun VappButton(node: VappNodeModel, state: VappSurfaceState, modifier: Modifier) {
    val label = node.str("label") ?: ""
    val variant = node.str("variant") ?: "outline"
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    // Press → set_pressed([id]) → re-layout: the core resolves `:pressed` into
    // the frame's opacity, applied at placement (no local visual).
    LaunchedEffect(pressed) {
        if (pressed) state.pressed[node.id] = true else state.pressed.remove(node.id)
    }
    val shape = RoundedCornerShape(10.dp)
    val chrome = when (variant) {
        // The GlassSubmitButton paint (solid primary, radius 10) at the 36dp
        // control rung: GlassSubmitButton itself hard-codes fillMaxWidth + 14dp
        // vertical padding (48dp), so it cannot sit at controlLg.
        "primary" -> Modifier.clip(shape).background(DesignTokens.Palette.Primary, shape)
        "outline" -> Modifier.glassButton()
        else -> Modifier.clip(shape)
    }
    val fg = when (variant) {
        "primary" -> DesignTokens.Palette.PrimaryForeground
        "ghost" -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        else -> MaterialTheme.colorScheme.onSurface
    }
    Box(
        modifier
            .height(DesignTokens.Size.ControlLg)
            .then(chrome)
            .clickable(interactionSource = interaction, indication = androidx.compose.foundation.LocalIndication.current, role = Role.Button) {}
            .padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = fg, fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1)
    }
}

@Composable
private fun VappTextField(node: VappNodeModel, modifier: Modifier, multiLine: Boolean) {
    var value by remember { mutableStateOf("") }
    GlassTextField(
        value = value,
        onValueChange = { value = it },
        modifier = modifier,
        placeholder = node.str("placeholder"),
        singleLine = !multiLine,
        minLines = if (multiLine) 3 else 1,
    )
}

/**
 * The typing-test field. The local field owns the string; each edit bumps a
 * revision and schedules a fake host echo 150 ms later, applied only when its
 * revision is still the latest (stale echoes drop).
 */
@Composable
private fun VappEchoField(node: VappNodeModel, modifier: Modifier) {
    var value by remember { mutableStateOf(TextFieldValue("")) }
    var latest by remember { mutableIntStateOf(0) }
    var echoed by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        GlassTextField(
            value = value,
            onValueChange = { next ->
                value = next
                val rev = ++latest
                val text = next.text
                scope.launch {
                    delay(ECHO_DELAY_MS)
                    if (rev == latest) {
                        echoed = text
                        // Host-owned: the echo is authoritative only when current.
                        if (value.text != text) value = TextFieldValue(text)
                    }
                }
            },
            modifier = Modifier.fillMaxWidth().testTag("echo-field"),
            placeholder = node.str("placeholder"),
            singleLine = true,
        )
        Text(
            "host: $echoed",
            modifier = Modifier.testTag("echo-host"),
            fontSize = 12.sp,
            lineHeight = 16.sp,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun VappToggle(node: VappNodeModel, modifier: Modifier) {
    var checked by remember { mutableStateOf(node.bool("checked")) }
    Row(
        modifier.toggleable(value = checked, role = Role.Switch, onValueChange = { checked = it }),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            node.str("label") ?: "",
            fontSize = 14.sp,
            lineHeight = 20.sp,
        )
        Spacer(Modifier.width(12.dp).weight(1f))
        Switch(checked = checked, onCheckedChange = null, colors = glassSwitchColors())
    }
}

@Composable
private fun VappSelect(node: VappNodeModel, modifier: Modifier) {
    val options = node.strings("options")
    var value by remember { mutableStateOf(node.str("value") ?: options.firstOrNull() ?: "") }
    var expanded by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(12.dp)
    Box(modifier) {
        Row(
            Modifier
                .height(DesignTokens.Size.InputHeight)
                .clip(shape)
                .background(GlassTokens.CardFill, shape)
                .androidBorder(shape)
                .clickable(role = Role.DropdownList) { expanded = true }
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(value, fontSize = 14.sp, lineHeight = 20.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Spacer(Modifier.width(8.dp).weight(1f))
            Icon(
                ExpIcons.uiChevronDown,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        GlassDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            options.forEach { option ->
                GlassMenuItem(text = { Text(option) }, onClick = { value = option; expanded = false })
            }
        }
    }
}

private fun Modifier.androidBorder(shape: RoundedCornerShape): Modifier =
    border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)

@Composable
private fun VappListRow(node: VappNodeModel, modifier: Modifier) {
    Row(
        modifier
            .height(DesignTokens.Size.RowHeight)
            .flatRow()
            .clickable {}
            .padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(node.str("title") ?: "", fontSize = 14.sp, lineHeight = 20.sp, maxLines = 1)
        Spacer(Modifier.width(8.dp).weight(1f))
        Text(
            node.str("meta") ?: "",
            fontSize = 12.sp,
            lineHeight = 16.sp,
            maxLines = 1,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

@Composable
private fun VappBadge(node: VappNodeModel, modifier: Modifier) {
    val count = node.props.opt("count")?.toString() ?: ""
    Box(
        modifier
            .height(20.dp)
            .defaultMinSize(minWidth = 20.dp)
            .background(GlassTokens.RowFillActive, CircleShape)
            .padding(horizontal = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(count, fontSize = 11.sp, lineHeight = 14.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun VappPill(node: VappNodeModel, modifier: Modifier) {
    val live = node.str("tone") == "live"
    GlassPill(
        label = node.str("label") ?: "",
        // Contract: 28 tall. The Md rung is 32, Sm 24; the caller height wins.
        modifier = modifier.height(28.dp),
        onClick = if (live) null else ({}),
        mode = if (live) PillMode.Readonly else PillMode.Select,
        selected = node.bool("selected"),
        opaque = live,
        leading = if (live) ({ LiveDot(busy = false, size = 6.dp) }) else null,
    )
}

@Composable
private fun VappImage(node: VappNodeModel, modifier: Modifier) {
    val tint = vappColor(node.str("placeholder")) ?: DesignTokens.Semantic.Blue
    Box(
        modifier
            .background(tint.copy(alpha = 0.55f))
            .semantics { contentDescription = node.str("alt") ?: "Image" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.editorImage,
            contentDescription = null,
            modifier = Modifier.size(32.dp),
            tint = Color.White.copy(alpha = 0.8f),
        )
    }
}
