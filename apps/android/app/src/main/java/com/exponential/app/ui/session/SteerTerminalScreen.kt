package com.exponential.app.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardReturn
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Dangerous
import androidx.compose.material.icons.filled.Keyboard
import androidx.compose.material.icons.filled.KeyboardHide
import androidx.compose.material.icons.filled.RemoveRedEye
import androidx.compose.material.icons.filled.Replay
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.terminal.VT_COLOR_DEFAULT
import com.exponential.app.ui.terminal.VT_TRUECOLOR
import com.exponential.app.ui.terminal.VtCell
import com.exponential.app.ui.terminal.VtSnapshot
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import kotlinx.coroutines.flow.distinctUntilChanged

// Remote steer viewer (masterplan §5c): a live monospace mirror of the desktop
// PTY over the relay, with presence, a steer claim toggle, and a keystroke
// input row while holding the claim. Watch is primary; the desktop terminal
// remains authoritative (its geometry arrives as relay resize frames).

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SteerTerminalScreen(
    onBack: () -> Unit,
    viewModel: SteerTerminalViewModel = hiltViewModel(),
) {
    val session by viewModel.session.collectAsStateWithLifecycle()
    val phase by viewModel.phase.collectAsStateWithLifecycle()
    val snapshot by viewModel.snapshot.collectAsStateWithLifecycle()
    val viewers by viewModel.viewers.collectAsStateWithLifecycle()
    val steererId by viewModel.steererId.collectAsStateWithLifecycle()
    val perm by viewModel.perm.collectAsStateWithLifecycle()
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val killState by viewModel.killState.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.connectIfIdle() }

    val steering = steererId != null && steererId == currentUserId
    val otherSteerer = viewers.firstOrNull { it.userId == steererId && it.userId != currentUserId }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text("Live terminal")
                        val label = session?.deviceLabel
                        if (!label.isNullOrBlank()) {
                            Text(
                                label,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .imePadding()
                .padding(horizontal = 12.dp),
        ) {
            // Presence bar: who's watching, who holds the steer claim.
            if (viewers.isNotEmpty()) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState())
                        .padding(vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Icon(
                        Icons.Filled.RemoveRedEye,
                        contentDescription = "Watching",
                        modifier = Modifier.size(13.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    viewers.forEach { viewer ->
                        val isSteerer = viewer.userId == steererId
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            if (isSteerer) {
                                Icon(
                                    Icons.Filled.Keyboard,
                                    contentDescription = "Steering",
                                    modifier = Modifier.size(13.dp),
                                    tint = MaterialTheme.colorScheme.onSurface,
                                )
                                Spacer(Modifier.width(3.dp))
                            }
                            Text(
                                viewer.name + if (isSteerer) " (steering)" else "",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(
                                    alpha = if (isSteerer) TextEmphasis.Primary else TextEmphasis.Secondary,
                                ),
                            )
                        }
                    }
                }
            }

            TerminalView(
                snapshot = snapshot,
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
            )

            when (val p = phase) {
                SteerPhase.Connecting -> StatusRow {
                    CircularProgressIndicator(
                        modifier = Modifier.size(14.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        "Connecting…",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
                is SteerPhase.Ended -> StatusRow {
                    Text(
                        p.detail ?: "The session has ended.",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
                is SteerPhase.Closed -> StatusRow {
                    Text(
                        p.detail ?: "Connection lost.",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.weight(1f),
                    )
                    PillButton(
                        icon = Icons.Filled.Replay,
                        label = "Reconnect",
                        onClick = { viewModel.connect() },
                    )
                }
                SteerPhase.Live -> {
                    // steer.killSession also authorizes the session owner — a
                    // member who remote-started their own session can kill it
                    // without steer perm (mirrors the web viewer).
                    val canKill = perm == "steer" ||
                        (session?.userId != null && session?.userId == currentUserId)
                    if (perm == "steer" || canKill) {
                        StatusRow {
                            if (perm == "steer") {
                                if (steering) {
                                    PillButton(
                                        icon = Icons.Filled.KeyboardHide,
                                        label = "Release control",
                                        active = true,
                                        onClick = { viewModel.release() },
                                    )
                                } else {
                                    PillButton(
                                        icon = Icons.Filled.Keyboard,
                                        label = if (otherSteerer != null) {
                                            "${otherSteerer.name} is steering"
                                        } else {
                                            "Take control"
                                        },
                                        enabled = otherSteerer == null,
                                        onClick = { viewModel.claim() },
                                    )
                                }
                            }
                            if (canKill) {
                                PillButton(
                                    icon = Icons.Filled.Dangerous,
                                    label = "Kill session",
                                    tint = MaterialTheme.colorScheme.error,
                                    onClick = { viewModel.requestKill() },
                                )
                            }
                        }
                    }
                    if (steering) {
                        SteerInputRow(onSend = viewModel::sendInput)
                    }
                }
                SteerPhase.Idle -> Unit
            }
            Spacer(Modifier.height(8.dp))
        }
    }

    // Kill confirm dialog (web parity: force-end the session via
    // steer.killSession; the synced row flips to ended and the relay `bye`
    // tears this viewer down). A failure keeps the dialog open with the error.
    if (killState != KillState.Idle) {
        val killing = killState == KillState.Killing
        AlertDialog(
            onDismissRequest = { viewModel.dismissKill() },
            title = { Text("Kill this coding session?") },
            text = {
                Column {
                    val device = session?.deviceLabel
                        ?.takeIf { it.isNotBlank() }
                        ?.let { " on $it" } ?: ""
                    Text(
                        "This force-terminates the terminal$device and ends the " +
                            "session. Uncommitted work in the worktree is kept, " +
                            "but Claude stops immediately.",
                    )
                    val failed = killState as? KillState.Failed
                    if (failed != null) {
                        Spacer(Modifier.height(8.dp))
                        Text(
                            failed.message,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { viewModel.confirmKill() }, enabled = !killing) {
                    Text("Kill session", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { viewModel.dismissKill() }, enabled = !killing) {
                    Text("Cancel")
                }
            },
        )
    }
}

@Composable
private fun StatusRow(content: @Composable RowScope.() -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}

@Composable
private fun PillButton(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    onClick: () -> Unit,
    active: Boolean = false,
    enabled: Boolean = true,
    tint: Color? = null,
) {
    val color = when {
        !enabled -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
        tint != null -> tint
        else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Primary)
    }
    Row(
        modifier = Modifier
            .glassButton(active = active)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = color,
        )
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = color,
        )
    }
}

// ── Keystroke input (bonus: soft-keyboard steering while holding the claim) ──

@Composable
private fun SteerInputRow(onSend: (String) -> Unit) {
    var field by remember { mutableStateOf("") }

    Column(modifier = Modifier.fillMaxWidth()) {
        // Quick keys the soft keyboard can't type.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            QuickKey("Esc") { onSend("\u001b") }
            QuickKey("Tab") { onSend("\t") }
            QuickKey("^C") { onSend("\u0003") }
            QuickKey("↑") { onSend("\u001b[A") }
            QuickKey("↓") { onSend("\u001b[B") }
            QuickKey("←") { onSend("\u001b[D") }
            QuickKey("→") { onSend("\u001b[C") }
            QuickKey("⏎") { onSend("\r") }
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            TextField(
                value = field,
                onValueChange = { field = it },
                modifier = Modifier.weight(1f),
                placeholder = {
                    Text(
                        "Type into the terminal…",
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                },
                singleLine = true,
                shape = RoundedCornerShape(12.dp),
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = GlassTokens.RowFill,
                    unfocusedContainerColor = GlassTokens.RowFill,
                    disabledContainerColor = GlassTokens.RowFill,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                    disabledIndicatorColor = Color.Transparent,
                ),
            )
            IconButton(onClick = {
                onSend(field + "\r")
                field = ""
            }) {
                Icon(Icons.AutoMirrored.Filled.Send, contentDescription = "Send")
            }
            IconButton(onClick = {
                if (field.isNotEmpty()) {
                    onSend(field)
                    field = ""
                }
            }) {
                Icon(
                    Icons.AutoMirrored.Filled.KeyboardReturn,
                    contentDescription = "Send without Enter",
                )
            }
        }
    }
}

@Composable
private fun QuickKey(label: String, onClick: () -> Unit) {
    Text(
        label,
        style = MaterialTheme.typography.labelMedium,
        fontFamily = FontFamily.Monospace,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier
            .glassButton()
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 6.dp),
    )
}

// ── The terminal surface: scrollback + grid as monospace annotated lines ─────

private val TerminalBg = Color(0xFF09090B)
private val DefaultFg = Color(0xFFE4E4E7)

@Composable
private fun TerminalView(snapshot: VtSnapshot, modifier: Modifier = Modifier) {
    BoxWithConstraints(
        modifier = modifier
            .background(TerminalBg, RoundedCornerShape(12.dp))
            .border(GlassTokens.Hairline, GlassTokens.StrokeSection, RoundedCornerShape(12.dp))
            .padding(8.dp),
    ) {
        // Fit the publisher's full width on screen: monospace advance ≈ 0.6 em.
        val density = LocalDensity.current
        val fontSizeSp = with(density) {
            (constraints.maxWidth.toFloat() / snapshot.cols.coerceAtLeast(1) / 0.603f).toSp()
        }.value.coerceIn(3f, 14f).sp
        val lineHeightSp = (fontSizeSp.value * 1.25f).sp

        val totalLines = snapshot.scrollback.size + snapshot.rows
        val listState = rememberLazyListState()
        var follow by remember { mutableStateOf(true) }

        // Only user drags flip follow-mode; programmatic scrolls keep it.
        LaunchedEffect(listState) {
            snapshotFlow { listState.isScrollInProgress to listState.canScrollForward }
                .distinctUntilChanged()
                .collect { (dragging, canForward) ->
                    if (dragging) follow = !canForward
                }
        }
        LaunchedEffect(snapshot, follow) {
            if (follow && totalLines > 0) listState.scrollToItem(totalLines - 1)
        }

        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            itemsIndexed(snapshot.scrollback) { _, line ->
                TerminalLine(line, null, fontSizeSp, lineHeightSp)
            }
            itemsIndexed(snapshot.cells) { rowIndex, line ->
                val cursorCol =
                    if (snapshot.cursorVisible && rowIndex == snapshot.cursorRow) {
                        snapshot.cursorCol
                    } else {
                        null
                    }
                TerminalLine(line, cursorCol, fontSizeSp, lineHeightSp)
            }
        }
    }
}

@Composable
private fun TerminalLine(
    line: List<VtCell>,
    cursorCol: Int?,
    fontSize: androidx.compose.ui.unit.TextUnit,
    lineHeight: androidx.compose.ui.unit.TextUnit,
) {
    val annotated = remember(line, cursorCol) { annotateLine(line, cursorCol) }
    Text(
        annotated,
        fontFamily = FontFamily.Monospace,
        fontSize = fontSize,
        lineHeight = lineHeight,
        softWrap = false,
        maxLines = 1,
    )
}

private fun annotateLine(line: List<VtCell>, cursorCol: Int?): AnnotatedString =
    buildAnnotatedString {
        line.forEachIndexed { i, cell ->
            var fg = resolveColor(cell.fg, isForeground = true, bold = cell.bold)
            var bg = resolveColor(cell.bg, isForeground = false, bold = false)
            if (cell.inverse != (i == cursorCol)) {
                val tmp = fg
                fg = if (bg == Color.Unspecified) TerminalBg else bg
                bg = if (tmp == Color.Unspecified) DefaultFg else tmp
            }
            withStyle(
                SpanStyle(
                    color = if (fg == Color.Unspecified) DefaultFg else fg,
                    background = if (bg == Color.Unspecified) Color.Transparent else bg,
                    fontWeight = if (cell.bold) FontWeight.Bold else null,
                    textDecoration = if (cell.underline) TextDecoration.Underline else null,
                ),
            ) {
                append(cell.text)
            }
        }
    }

// Standard xterm palette: 16 ANSI + 6×6×6 cube + grayscale ramp.
private val Ansi16 = intArrayOf(
    0xFF000000.toInt(), 0xFFCD0000.toInt(), 0xFF00CD00.toInt(), 0xFFCDCD00.toInt(),
    0xFF0000EE.toInt(), 0xFFCD00CD.toInt(), 0xFF00CDCD.toInt(), 0xFFE5E5E5.toInt(),
    0xFF7F7F7F.toInt(), 0xFFFF0000.toInt(), 0xFF00FF00.toInt(), 0xFFFFFF00.toInt(),
    0xFF5C5CFF.toInt(), 0xFFFF00FF.toInt(), 0xFF00FFFF.toInt(), 0xFFFFFFFF.toInt(),
)

private fun resolveColor(code: Int, isForeground: Boolean, bold: Boolean): Color {
    if (code == VT_COLOR_DEFAULT) {
        return if (isForeground && bold) DefaultFg else Color.Unspecified
    }
    if (code and VT_TRUECOLOR != 0) {
        return Color(0xFF000000.toInt() or (code and 0xFFFFFF))
    }
    var idx = code and 0xFF
    // Bold brightens the low-intensity ANSI colors (xterm convention).
    if (bold && isForeground && idx < 8) idx += 8
    return when {
        idx < 16 -> Color(Ansi16[idx])
        idx < 232 -> {
            val n = idx - 16
            val r = cubeLevel(n / 36 % 6)
            val g = cubeLevel(n / 6 % 6)
            val b = cubeLevel(n % 6)
            Color(0xFF000000.toInt() or (r shl 16) or (g shl 8) or b)
        }
        else -> {
            val v = 8 + 10 * (idx - 232)
            Color(0xFF000000.toInt() or (v shl 16) or (v shl 8) or v)
        }
    }
}

private fun cubeLevel(v: Int): Int = if (v == 0) 0 else 55 + 40 * v
