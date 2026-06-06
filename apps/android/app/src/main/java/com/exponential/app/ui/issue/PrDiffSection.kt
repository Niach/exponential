package com.exponential.app.ui.issue

import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.api.PullFile

private val AddLine = Color(0xFF6EE7B7) // emerald-300
private val DelLine = Color(0xFFFDA4AF) // rose-300
private val HunkLine = Color(0xFFA5B4FC) // indigo-300

private fun lineColor(line: String, context: Color): Color = when {
    line.startsWith("@@") -> HunkLine
    line.startsWith("+") -> AddLine
    line.startsWith("-") -> DelLine
    else -> context
}

// Collapsible inline PR diff. Mirrors the web DiffView: a "View changes" toggle
// that lazily fetches the issue's PR file patches and renders them with +/-
// line coloring. Shown only when the issue has a linked PR.
@Composable
fun PrDiffSection(prUrl: String?, loadFiles: suspend () -> List<PullFile>) {
    if (prUrl.isNullOrBlank()) return
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
