package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Difference
import androidx.compose.material.icons.filled.Tv
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.api.PrFilesResult
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

private val LiveGreen = Color(0xFF34D399)

// The unified "Changes" section on the issue detail (masterplan §4.8, mobile
// tiers 2–4 — mobile never sees a local worktree and does no git ops):
//   2. a PR exists            → "View changes" opens the dedicated diff page,
//                               with a browser link;
//   3. branch pushed, no PR   → repositories.branchDiff summary ("Branch exp/…
//                               — no PR yet") + the same "View changes" page;
//   4. nothing pushed yet     → "Being coded on <device>" opening the native
//      steer viewer (when a session is running), else a quiet empty state.
@Composable
fun ChangesSection(
    prUrl: String?,
    branch: String?,
    prState: String?,
    runningSessionId: String?,
    runningSessionDeviceLabel: String?,
    steerEnabled: Boolean,
    isMember: Boolean,
    prClosing: Boolean,
    prCloseError: String?,
    loadBranchDiff: suspend () -> PrFilesResult?,
    onOpenChanges: () -> Unit,
    onWatch: (String) -> Unit,
    onClosePr: () -> Unit,
) {
    val hasPr = !prUrl.isNullOrBlank()
    val hasBranch = !branch.isNullOrBlank()
    // Nothing to show: no PR, no branch, and no live session.
    if (!hasPr && !hasBranch && runningSessionId == null) return

    Column(modifier = Modifier.fillMaxWidth().glassSection().padding(12.dp)) {
        Text(
            "Changes",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.height(10.dp))
        if (hasPr) {
            PrTier(
                prUrl = prUrl!!,
                branch = branch,
                // The reject path (EXP-100) only exists for members with an
                // OPEN PR — merged/closed PRs and public viewers never see it.
                canClose = isMember && prState == DomainContract.prStateOpen,
                closing = prClosing,
                closeError = prCloseError,
                onClosePr = onClosePr,
                onOpenChanges = onOpenChanges,
            )
        } else {
            BranchOrCodingTier(
                branch = branch,
                runningSessionId = runningSessionId,
                deviceLabel = runningSessionDeviceLabel,
                steerEnabled = steerEnabled,
                isMember = isMember,
                loadBranchDiff = loadBranchDiff,
                onOpenChanges = onOpenChanges,
                onWatch = onWatch,
            )
        }
    }
}

// Tier 2: linked PR — branch label, a browser link, and the diff-page link.
// Members with an OPEN PR also get a deliberately subtle "Close PR" at the
// row's far edge (EXP-100: reject the PR when the issue got dropped even
// though the work exists) — confirmation-gated, never the visual primary.
@Composable
private fun PrTier(
    prUrl: String,
    branch: String?,
    canClose: Boolean,
    closing: Boolean,
    closeError: String?,
    onClosePr: () -> Unit,
    onOpenChanges: () -> Unit,
) {
    val context = LocalContext.current
    var closeConfirmOpen by remember { mutableStateOf(false) }
    if (!branch.isNullOrBlank()) {
        Text(
            branch,
            style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
            color = CommentMeta,
        )
        Spacer(Modifier.height(4.dp))
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.clickable {
                runCatching {
                    val intent = android.content.Intent(
                        android.content.Intent.ACTION_VIEW,
                        android.net.Uri.parse(prUrl),
                    )
                    intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                    context.startActivity(intent)
                }
            },
        ) {
            Icon(
                Icons.AutoMirrored.Filled.OpenInNew,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = CommentAccent,
            )
            Spacer(Modifier.width(6.dp))
            Text(
                "View pull request",
                style = MaterialTheme.typography.labelMedium,
                color = CommentAccent,
            )
        }
        Spacer(Modifier.weight(1f))
        if (canClose) {
            if (closing) {
                CircularProgressIndicator(modifier = Modifier.size(14.dp), strokeWidth = 2.dp)
            } else {
                Text(
                    "Close PR",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .clickable { closeConfirmOpen = true }
                        .padding(horizontal = 4.dp, vertical = 2.dp),
                )
            }
        }
    }
    if (closeError != null) {
        Spacer(Modifier.height(4.dp))
        Text(
            closeError,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.error,
        )
    }
    Spacer(Modifier.height(8.dp))
    ViewChangesButton(onClick = onOpenChanges)

    if (closeConfirmOpen) {
        AlertDialog(
            onDismissRequest = { closeConfirmOpen = false },
            title = { Text("Close pull request?") },
            text = {
                Text(
                    "Closes the pull request on GitHub WITHOUT merging — use this " +
                        "when the issue was dropped even though the work exists. " +
                        "The branch is kept and the PR can be reopened on GitHub.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        closeConfirmOpen = false
                        onClosePr()
                    },
                ) {
                    Text("Close PR", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { closeConfirmOpen = false }) { Text("Cancel") }
            },
        )
    }
}

// Tiers 3 & 4: no PR. Try the pushed-branch diff; if the branch was never
// pushed, fall through to the live-session ("being coded") / empty state.
@Composable
private fun BranchOrCodingTier(
    branch: String?,
    runningSessionId: String?,
    deviceLabel: String?,
    steerEnabled: Boolean,
    isMember: Boolean,
    loadBranchDiff: suspend () -> PrFilesResult?,
    onOpenChanges: () -> Unit,
    onWatch: (String) -> Unit,
) {
    val hasBranch = !branch.isNullOrBlank()
    var loading by remember(branch) { mutableStateOf(hasBranch) }
    var diff by remember(branch) { mutableStateOf<PrFilesResult?>(null) }
    LaunchedEffect(branch) {
        if (!hasBranch) {
            loading = false
            return@LaunchedEffect
        }
        loading = true
        diff = runCatching { loadBranchDiff() }.getOrNull()
        loading = false
    }

    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

    when {
        loading -> Row(
            modifier = Modifier.padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
            Text("Loading changes…", style = MaterialTheme.typography.bodySmall, color = secondary)
        }
        // Tier 3: the branch is pushed — summary counts + the diff page.
        diff != null -> {
            Text(
                if (!branch.isNullOrBlank()) "Branch $branch — no PR yet" else "No PR yet",
                style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                color = CommentMeta,
            )
            Spacer(Modifier.height(6.dp))
            val files = diff!!.files
            if (files.isEmpty()) {
                Text("No changed files.", style = MaterialTheme.typography.bodySmall, color = tertiary)
            } else {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "${files.size} ${if (files.size == 1) "file" else "files"} changed",
                        style = MaterialTheme.typography.bodySmall,
                        color = secondary,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        "+${files.sumOf { it.additions }}",
                        color = DiffAddColor,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 11.sp,
                    )
                    Spacer(Modifier.width(4.dp))
                    Text(
                        "−${files.sumOf { it.deletions }}",
                        color = DiffDelColor,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 11.sp,
                    )
                }
                Spacer(Modifier.height(8.dp))
                ViewChangesButton(onClick = onOpenChanges)
            }
        }
        // Tier 4: nothing pushed. Name the live machine + open the steer viewer.
        runningSessionId != null -> {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "Being coded on ${deviceLabel?.takeIf { it.isNotBlank() } ?: "a desktop"}",
                    style = MaterialTheme.typography.bodySmall,
                    color = secondary,
                    modifier = Modifier.weight(1f),
                )
                if (steerEnabled && isMember) {
                    Row(
                        modifier = Modifier
                            .glassButton()
                            .clickable { onWatch(runningSessionId) }
                            .padding(horizontal = 12.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(
                            Icons.Filled.Tv,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = LiveGreen,
                        )
                        Text(
                            "Watch",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            }
        }
        else -> Text(
            "No changes yet.",
            style = MaterialTheme.typography.bodySmall,
            color = tertiary,
        )
    }
}

// "View changes" pill — navigates to the dedicated diff page (EXP-34; the old
// inline FilePatch expansion is gone).
@Composable
private fun ViewChangesButton(onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .glassButton()
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            Icons.Filled.Difference,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Text(
            "View changes",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}
