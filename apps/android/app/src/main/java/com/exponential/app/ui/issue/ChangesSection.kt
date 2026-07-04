package com.exponential.app.ui.issue

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Tv
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.api.PrFilesResult
import com.exponential.app.data.api.PullFile
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

private val AddLine = Color(0xFF6EE7B7) // emerald-300
private val DelLine = Color(0xFFFDA4AF) // rose-300
private val HunkLine = Color(0xFFA5B4FC) // indigo-300
private val LiveGreen = Color(0xFF34D399)

private fun lineColor(line: String, context: Color): Color = when {
    line.startsWith("@@") -> HunkLine
    line.startsWith("+") -> AddLine
    line.startsWith("-") -> DelLine
    else -> context
}

// The unified "Changes" section on the issue detail (masterplan §4.8, mobile
// tiers 2–4 — mobile never sees a local worktree and does no git ops):
//   2. a PR exists            → the PR diff (as today), with a browser link;
//   3. branch pushed, no PR   → repositories.branchDiff ("Branch exp/… — no PR yet");
//   4. nothing pushed yet     → "Being coded on <device>" opening the native
//      steer viewer (when a session is running), else a quiet empty state.
@Composable
fun ChangesSection(
    prUrl: String?,
    branch: String?,
    runningSessionId: String?,
    runningSessionDeviceLabel: String?,
    steerEnabled: Boolean,
    isMember: Boolean,
    loadPrFiles: suspend () -> List<PullFile>,
    loadBranchDiff: suspend () -> PrFilesResult?,
    onWatch: (String) -> Unit,
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
            PrTier(prUrl = prUrl!!, branch = branch, loadFiles = loadPrFiles)
        } else {
            BranchOrCodingTier(
                branch = branch,
                runningSessionId = runningSessionId,
                deviceLabel = runningSessionDeviceLabel,
                steerEnabled = steerEnabled,
                isMember = isMember,
                loadBranchDiff = loadBranchDiff,
                onWatch = onWatch,
            )
        }
    }
}

// Tier 2: linked PR — branch label, a browser link, and the collapsible diff.
@Composable
private fun PrTier(prUrl: String, branch: String?, loadFiles: suspend () -> List<PullFile>) {
    val context = LocalContext.current
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
    DiffDisclosure(loadFiles = loadFiles)
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
        // Tier 3: the branch is pushed — show the compare diff.
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
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    files.forEach { FilePatch(it) }
                }
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

// Collapsible inline diff: a "View changes" toggle that lazily fetches the file
// patches and renders them with +/- line coloring. Shared by the PR tier.
@Composable
private fun DiffDisclosure(loadFiles: suspend () -> List<PullFile>) {
    var expanded by remember { mutableStateOf(false) }
    Column(modifier = Modifier.fillMaxWidth()) {
        TextButton(onClick = { expanded = !expanded }) {
            Text(if (expanded) "Hide changes" else "View changes")
        }
        if (expanded) {
            var loading by remember { mutableStateOf(true) }
            var error by remember { mutableStateOf<String?>(null) }
            var files by remember { mutableStateOf<List<PullFile>>(emptyList()) }
            LaunchedEffect(Unit) {
                loading = true
                error = null
                try {
                    files = loadFiles()
                } catch (e: Throwable) {
                    error = e.message ?: "Failed to load changes"
                }
                loading = false
            }
            when {
                loading -> Row(
                    modifier = Modifier.padding(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                    Text("Loading changes…", style = MaterialTheme.typography.bodySmall)
                }
                error != null -> Text(
                    "Couldn’t load changes: $error",
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(8.dp),
                )
                files.isEmpty() -> Text(
                    "No changed files.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(8.dp),
                )
                else -> Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    files.forEach { FilePatch(it) }
                }
            }
        }
    }
}

@Composable
private fun FilePatch(file: PullFile) {
    val outline = MaterialTheme.colorScheme.outlineVariant
    val context = MaterialTheme.colorScheme.onSurfaceVariant
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(6.dp))
            .border(1.dp, outline, RoundedCornerShape(6.dp)),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                file.filename,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.size(8.dp))
            Text("+${file.additions}", color = AddLine, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
            Spacer(Modifier.size(4.dp))
            Text("-${file.deletions}", color = DelLine, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
        }
        val patch = file.patch
        if (patch != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(vertical = 4.dp),
            ) {
                patch.split("\n").forEach { line ->
                    Text(
                        text = line.ifEmpty { " " },
                        color = lineColor(line, context),
                        fontFamily = FontFamily.Monospace,
                        fontSize = 11.sp,
                        maxLines = 1,
                        modifier = Modifier.padding(horizontal = 10.dp),
                    )
                }
            }
        } else {
            Text(
                if (file.status == "renamed") "Renamed." else "No textual diff (binary or too large).",
                style = MaterialTheme.typography.bodySmall,
                color = context,
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            )
        }
    }
}
