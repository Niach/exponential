package com.exponential.app.ui.support

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SupportLinkedIssue
import com.exponential.app.data.api.SupportMessage
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import kotlinx.coroutines.delay

/** Amber accent for internal notes (member-only annotations). */
private val InternalAmber = DesignTokens.Semantic.Yellow

/**
 * One support ticket's conversation (EXP-180): reporter messages leading,
 * member replies trailing, internal notes amber-tinted; bottom composer with a
 * Reply / Internal-note toggle; Close/Reopen in the top bar; escalate-to-issue
 * via a board picker (or the linked-issue chip once escalated).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SupportThreadScreen(
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit,
    viewModel: SupportThreadViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val boards by viewModel.boards.collectAsStateWithLifecycle()
    var internalMode by rememberSaveable { mutableStateOf(false) }
    var draft by rememberSaveable { mutableStateOf("") }
    var escalateSheetOpen by remember { mutableStateOf(false) }
    val listState = rememberLazyListState()

    // Keep the newest message in view as the transcript grows.
    LaunchedEffect(state.messages.size) {
        if (state.messages.isNotEmpty()) listState.animateScrollToItem(state.messages.size - 1)
    }
    // Transient action errors auto-dismiss.
    LaunchedEffect(state.transient) {
        if (state.transient != null) {
            delay(4_000)
            viewModel.consumeTransient()
        }
    }

    val thread = state.thread
    val resolved = thread?.status == "resolved"
    val reporter = thread?.reporterName?.takeIf { it.isNotBlank() } ?: thread?.reporterEmail

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(
                            thread?.title ?: "Support",
                            style = MaterialTheme.typography.titleMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (reporter != null) {
                            Text(
                                reporter,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    if (thread != null) {
                        TextButton(
                            onClick = {
                                if (resolved) viewModel.reopenTicket() else viewModel.closeTicket()
                            },
                        ) {
                            Text(if (resolved) "Reopen ticket" else "Close ticket")
                        }
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .imePadding(),
        ) {
            when {
                state.linkedIssue != null -> LinkedIssueChip(
                    issue = state.linkedIssue!!,
                    onClick = { onOpenIssue(state.linkedIssue!!.id) },
                )
                thread != null -> Row(Modifier.padding(horizontal = 16.dp)) {
                    Text(
                        "Escalate to issue",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .glassButton()
                            .clickable { escalateSheetOpen = true }
                            .padding(horizontal = 14.dp, vertical = 8.dp),
                    )
                }
            }
            when {
                state.loading && thread == null -> LoadingState(Modifier.weight(1f))
                thread == null -> EmptyState(
                    message = state.error ?: "This ticket could not be loaded.",
                    modifier = Modifier.weight(1f),
                )
                else -> LazyColumn(
                    state = listState,
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 16.dp, vertical = 10.dp),
                ) {
                    items(state.messages, key = { it.id }) { message ->
                        MessageBubble(message)
                    }
                }
            }
            state.transient?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.labelMedium,
                    color = DesignTokens.Semantic.Red,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                )
            }
            if (thread != null) {
                Composer(
                    internalMode = internalMode,
                    onModeChange = { internalMode = it },
                    draft = draft,
                    onDraftChange = { draft = it },
                    sending = state.sending,
                    onSend = {
                        viewModel.sendMessage(draft, internalMode) { draft = "" }
                    },
                )
            }
        }
    }

    if (escalateSheetOpen) {
        ModalBottomSheet(onDismissRequest = { escalateSheetOpen = false }) {
            Column(
                modifier = Modifier
                    .padding(horizontal = 16.dp)
                    .navigationBarsPadding(),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text("Escalate to issue", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Files an issue on a board of this team, linked to this ticket.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                Spacer(Modifier.height(4.dp))
                if (boards.isEmpty()) {
                    Text(
                        "No boards in this team.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
                boards.forEach { board ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .glassRow()
                            .clickable {
                                escalateSheetOpen = false
                                viewModel.escalate(board.id)
                            }
                            .padding(
                                horizontal = GlassTokens.RowPaddingH,
                                vertical = GlassTokens.RowPaddingV,
                            ),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            board.name,
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            board.prefix,
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

/** The escalated issue, as a chip navigating to the ordinary issue screen. */
@Composable
private fun LinkedIssueChip(issue: SupportLinkedIssue, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .padding(horizontal = 16.dp)
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            issue.identifier,
            fontFamily = FontFamily.Monospace,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            issue.title,
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

// Inbound (reporter) bubbles lead in neutral glass; outbound member replies
// trail in the brighter sender tint (the agent-session convention); internal
// notes trail amber-tinted with an explicit label.
@Composable
private fun MessageBubble(message: SupportMessage) {
    val inbound = message.direction == "inbound"
    val internal = message.visibility == "internal"
    val shape = RoundedCornerShape(12.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        horizontalArrangement = if (inbound) Arrangement.Start else Arrangement.End,
    ) {
        if (!inbound) Spacer(Modifier.width(32.dp))
        Column(
            modifier = Modifier
                .background(
                    when {
                        internal -> InternalAmber.copy(alpha = 0.10f)
                        inbound -> GlassTokens.SectionFill
                        else -> GlassTokens.RowFillActive
                    },
                    shape,
                )
                .border(
                    GlassTokens.Hairline,
                    if (internal) InternalAmber.copy(alpha = 0.35f) else GlassTokens.StrokeRow,
                    shape,
                )
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            if (internal) {
                Text(
                    "Internal",
                    style = MaterialTheme.typography.labelSmall,
                    color = InternalAmber,
                )
            }
            SelectionContainer {
                Text(
                    message.body,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
            Text(
                relativeTime(message.createdAt),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            )
        }
        if (inbound) Spacer(Modifier.width(32.dp))
    }
}

@Composable
private fun Composer(
    internalMode: Boolean,
    onModeChange: (Boolean) -> Unit,
    draft: String,
    onDraftChange: (String) -> Unit,
    sending: Boolean,
    onSend: () -> Unit,
) {
    Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ModePill("Reply", active = !internalMode) { onModeChange(false) }
            ModePill("Internal note", active = internalMode) { onModeChange(true) }
        }
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            TextField(
                value = draft,
                onValueChange = onDraftChange,
                modifier = Modifier.weight(1f),
                placeholder = {
                    Text(
                        if (internalMode) "Add an internal note…" else "Reply to the reporter…",
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                },
                maxLines = 4,
                shape = RoundedCornerShape(12.dp),
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = if (internalMode) InternalAmber.copy(alpha = 0.10f) else GlassTokens.RowFill,
                    unfocusedContainerColor = if (internalMode) InternalAmber.copy(alpha = 0.10f) else GlassTokens.RowFill,
                    disabledContainerColor = GlassTokens.RowFill,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                    disabledIndicatorColor = Color.Transparent,
                ),
            )
            IconButton(
                onClick = onSend,
                enabled = draft.isNotBlank() && !sending,
            ) {
                Icon(
                    Icons.AutoMirrored.Filled.Send,
                    contentDescription = if (internalMode) "Add note" else "Send reply",
                    tint = MaterialTheme.colorScheme.onSurface.copy(
                        alpha = if (draft.isNotBlank() && !sending) 1f else TextEmphasis.Quaternary,
                    ),
                )
            }
        }
    }
}

@Composable
private fun ModePill(label: String, active: Boolean, onClick: () -> Unit) {
    Text(
        label,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurface.copy(
            alpha = if (active) 1f else TextEmphasis.Secondary,
        ),
        modifier = Modifier
            .glassButton(active = active)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 6.dp),
    )
}
