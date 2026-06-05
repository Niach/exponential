package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowCircleUp
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.QuestionMark
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.SmartToy
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.PlanColors
import com.exponential.app.ui.theme.glassSection
import kotlinx.coroutines.launch

// Agent lifecycle events shown in the quiet activity feed (and used to detect a
// terminal error for the Retry affordance). Mirrors AGENT_EVENT_TYPES in
// apps/web/src/components/agent-plan-panel.tsx.
internal val agentEventTypes = setOf(
    "agent_started", "plan_ready", "agent_question",
    "agent_answer", "pr_opened", "pr_merged", "agent_error",
)

private val PanelText = Color.White.copy(alpha = 0.9f)
private val PanelMeta = Color.White.copy(alpha = 0.5f)
private val PanelFieldBg = Color.White.copy(alpha = 0.06f)
private val PanelAccent = Color(red = 0.42f, green = 0.64f, blue = 1.0f)
private val ErrorRed = Color(0xFFF87171)

// First-class panel for the agent plan/question lifecycle, replacing the
// plan/question comment rows (mirror of apps/web/src/components/agent-plan-panel.tsx).
// State is driven by the synced `issue` columns; the plan/question TEXT is
// fetched via agentPlan.getState (server-only, not in Electric).
@Composable
fun AgentPlanPanel(
    issueId: String,
    canApprovePlan: Boolean,
    viewModel: AgentPlanPanelViewModel = hiltViewModel(),
) {
    LaunchedEffect(issueId) { viewModel.bind(issueId) }
    val state by viewModel.state.collectAsStateWithLifecycle()
    val issue = state.issue
    val scope = rememberCoroutineScope()

    val agentEvents = remember(state.events) { state.events.filter { it.type in agentEventTypes } }
    val latestIsError = agentEvents.lastOrNull()?.type == "agent_error"
    val planState = issue?.agentPlanState

    // Render nothing when there is no agent involvement.
    if (issue == null || (planState == null && !latestIsError)) return

    var busy by remember { mutableStateOf<String?>(null) }
    var answer by remember { mutableStateOf("") }

    val finished = issue.status == "done" || issue.status == "cancelled"
    val implementing = !finished && planState == "approved" && issue.prState == null && !latestIsError

    fun act(label: String, block: suspend () -> Unit) {
        scope.launch {
            busy = label
            try {
                block()
            } finally {
                // Always clear, even if the action throws or the coroutine is
                // cancelled — otherwise the buttons stay disabled forever.
                busy = null
            }
        }
    }

    Column(modifier = Modifier.fillMaxWidth().glassSection().padding(12.dp)) {
        // Header
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                Icons.Filled.SmartToy,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = PlanColors.AwaitingApproval,
            )
            Spacer(Modifier.width(6.dp))
            Text("Agent plan", style = MaterialTheme.typography.labelLarge, color = PanelText)
            if (issue.agentPlanRevision > 0) {
                Spacer(Modifier.width(6.dp))
                Text("rev ${issue.agentPlanRevision}", style = MaterialTheme.typography.labelSmall, color = PanelMeta)
            }
            Spacer(Modifier.weight(1f))
            if (planState == "approved" && issue.agentPlanApprovedAt != null) {
                Icon(Icons.Filled.Check, contentDescription = null, modifier = Modifier.size(12.dp), tint = PlanColors.Approved)
                Spacer(Modifier.width(4.dp))
                Text("Approved", style = MaterialTheme.typography.labelSmall, color = PlanColors.Approved)
            }
        }

        Spacer(Modifier.height(8.dp))

        when (planState) {
            "drafting", "planning" -> LoadingRow("Agent is working on a plan…")
            "awaiting_answer" -> QuestionContent(
                question = state.questionText,
                canApprovePlan = canApprovePlan,
                answer = answer,
                onAnswerChange = { answer = it },
                sending = busy == "answer",
                enabled = busy == null,
                onSend = {
                    val text = answer.trim()
                    if (text.isNotEmpty()) act("answer") { viewModel.answerQuestion(text); answer = "" }
                },
            )
            "awaiting_approval", "approved" -> PlanContent(
                planText = state.planText,
                showApproval = planState == "awaiting_approval" && canApprovePlan,
                busy = busy,
                onApprove = { act("approve") { viewModel.approvePlan() } },
                onRequestChanges = { act("request") { viewModel.requestChanges() } },
                implementing = implementing,
            )
            else -> {}
        }

        if (latestIsError) {
            Spacer(Modifier.height(8.dp))
            ErrorBanner(
                canRetry = canApprovePlan,
                retrying = busy == "retry",
                enabled = busy == null,
                onRetry = { act("retry") { viewModel.retry() } },
            )
        }
    }
}

@Composable
private fun LoadingRow(text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CircularProgressIndicator(modifier = Modifier.size(14.dp), strokeWidth = 2.dp, color = PlanColors.AwaitingApproval)
        Text(text, style = MaterialTheme.typography.bodySmall, color = PanelMeta)
    }
}

@Composable
private fun QuestionContent(
    question: String?,
    canApprovePlan: Boolean,
    answer: String,
    onAnswerChange: (String) -> Unit,
    sending: Boolean,
    enabled: Boolean,
    onSend: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(Icons.Filled.QuestionMark, contentDescription = null, modifier = Modifier.size(14.dp), tint = PlanColors.AwaitingAnswer)
        Spacer(Modifier.width(6.dp))
        Text("The agent has a question", style = MaterialTheme.typography.labelMedium, color = PlanColors.AwaitingAnswer)
    }
    Spacer(Modifier.height(6.dp))
    if (!question.isNullOrEmpty()) {
        MarkdownView(question)
    } else {
        Text("Loading…", style = MaterialTheme.typography.bodySmall, color = PanelMeta)
    }
    if (canApprovePlan) {
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            BasicTextField(
                value = answer,
                onValueChange = onAnswerChange,
                enabled = enabled,
                textStyle = MaterialTheme.typography.bodyMedium.copy(color = PanelText),
                cursorBrush = SolidColor(PanelAccent),
                maxLines = 5,
                modifier = Modifier.weight(1f),
                decorationBox = { inner ->
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(18.dp))
                            .background(PanelFieldBg)
                            .padding(horizontal = 12.dp, vertical = 10.dp),
                    ) {
                        if (answer.isEmpty()) {
                            Text("Answer the agent…", style = MaterialTheme.typography.bodyMedium, color = PanelMeta)
                        }
                        inner()
                    }
                },
            )
            IconButton(onClick = onSend, enabled = enabled && answer.isNotBlank()) {
                if (sending) {
                    CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp, color = PanelAccent)
                } else {
                    Icon(
                        Icons.Filled.ArrowCircleUp,
                        contentDescription = "Send answer",
                        modifier = Modifier.size(30.dp),
                        tint = if (answer.isBlank()) Color.White.copy(alpha = 0.3f) else PanelAccent,
                    )
                }
            }
        }
    }
}

@Composable
private fun PlanContent(
    planText: String?,
    showApproval: Boolean,
    busy: String?,
    onApprove: () -> Unit,
    onRequestChanges: () -> Unit,
    implementing: Boolean,
) {
    if (!planText.isNullOrEmpty()) {
        MarkdownView(planText)
    } else {
        Text("Loading plan…", style = MaterialTheme.typography.bodySmall, color = PanelMeta)
    }
    if (showApproval) {
        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                onClick = onApprove,
                enabled = busy == null,
                colors = ButtonDefaults.buttonColors(
                    containerColor = Color(0xFF22C55E).copy(alpha = 0.22f),
                    contentColor = Color(0xFF22C55E),
                ),
            ) {
                Icon(Icons.Filled.Check, contentDescription = null, modifier = Modifier.size(14.dp))
                Spacer(Modifier.width(6.dp))
                Text(if (busy == "approve") "Approving…" else "Approve")
            }
            OutlinedButton(onClick = onRequestChanges, enabled = busy == null) {
                Text(if (busy == "request") "Requesting…" else "Request changes")
            }
        }
    }
    if (implementing) {
        Spacer(Modifier.height(10.dp))
        LoadingRow("Agent is implementing the approved plan…")
    }
}

@Composable
private fun ErrorBanner(canRetry: Boolean, retrying: Boolean, enabled: Boolean, onRetry: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(ErrorRed.copy(alpha = 0.08f), RoundedCornerShape(8.dp))
            .border(0.5.dp, ErrorRed.copy(alpha = 0.25f), RoundedCornerShape(8.dp))
            .padding(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(ErrorRed))
        Spacer(Modifier.width(8.dp))
        Text("The agent hit an error.", style = MaterialTheme.typography.bodySmall, color = PanelMeta)
        if (canRetry) {
            Spacer(Modifier.weight(1f))
            OutlinedButton(onClick = onRetry, enabled = enabled) {
                Icon(Icons.Filled.Refresh, contentDescription = null, modifier = Modifier.size(14.dp))
                Spacer(Modifier.width(6.dp))
                Text(if (retrying) "Retrying…" else "Retry")
            }
        }
    }
}

// A quiet, collapsible feed of agent lifecycle events. Separate from the human
// comment thread so routine agent activity doesn't read as conversation.
// Mirror of apps/web/src/components/agent-activity-feed.tsx.
@Composable
fun AgentActivityFeed(events: List<IssueEventEntity>, usersById: Map<String, UserEntity>) {
    val agentEvents = remember(events) {
        events.filter { it.type in agentEventTypes }.sortedBy { it.createdAt }
    }
    if (agentEvents.isEmpty()) return
    var expanded by remember { mutableStateOf(false) }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(Color.White.copy(alpha = 0.03f), RoundedCornerShape(8.dp))
            .padding(10.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().clickable { expanded = !expanded },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                if (expanded) Icons.Filled.KeyboardArrowDown else Icons.Filled.KeyboardArrowRight,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = PanelMeta,
            )
            Spacer(Modifier.width(4.dp))
            Text("Agent activity (${agentEvents.size})", style = MaterialTheme.typography.labelMedium, color = PanelMeta)
        }
        if (expanded) {
            Spacer(Modifier.height(6.dp))
            agentEvents.forEach { event ->
                key(event.id) {
                    val who = usersById[event.actorUserId]?.let { it.name ?: it.email } ?: "Agent"
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Box(Modifier.size(6.dp).clip(CircleShape).background(PanelMeta))
                        Text(
                            "$who ${agentEventVerb(event.type)} · ${relativeTime(event.createdAt)}",
                            style = MaterialTheme.typography.labelSmall,
                            color = PanelMeta,
                        )
                    }
                }
            }
        }
    }
}

// Human-readable verb for an issue/agent event type (shared with CommentThread's
// non-agent EventRow).
internal fun agentEventVerb(type: String): String = when (type) {
    "status_changed" -> "changed the status"
    "assignee_changed" -> "changed the assignee"
    "label_added" -> "added a label"
    "label_removed" -> "removed a label"
    "pr_opened" -> "opened a pull request"
    "pr_merged" -> "merged the pull request"
    "plan_ready" -> "posted a plan for review"
    "agent_error" -> "hit an error"
    "agent_started" -> "started working"
    "agent_question" -> "asked a question"
    "agent_answer" -> "answered the agent"
    else -> type.replace('_', ' ')
}
