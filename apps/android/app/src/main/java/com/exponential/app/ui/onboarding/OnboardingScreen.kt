package com.exponential.app.ui.onboarding

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// First-run wizard (web onboarding parity): create your first project, then your
// first issue, gated once by onboardingCompletedAt. Single screen with internal
// step state; floats on the shared AppBackground (transparent Scaffold).
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun OnboardingScreen(
    onDone: () -> Unit,
    viewModel: OnboardingViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val done by viewModel.done.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.ensureWorkspace() }
    LaunchedEffect(done) { if (done) onDone() }
    BackHandler(enabled = state.step > 0) { viewModel.back() }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 32.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                "Welcome to Exponential",
                style = MaterialTheme.typography.headlineSmall,
                color = MaterialTheme.colorScheme.onSurface,
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                "Let's set up your workspace.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(20.dp))
            ProgressDots(step = state.step, count = 2)
            Spacer(Modifier.height(24.dp))

            Box(modifier = Modifier.widthIn(max = 460.dp).fillMaxWidth()) {
                when (state.step) {
                    0 -> ProjectStep(busy = state.busy) { name, prefix, color ->
                        viewModel.createProject(name, prefix, color)
                    }
                    else -> IssueStep(
                        busy = state.busy,
                        onCreate = { viewModel.createIssue(it) },
                        onSkip = { viewModel.skip() },
                    )
                }
            }

            state.error?.let {
                Spacer(Modifier.height(12.dp))
                Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }

            Spacer(Modifier.height(16.dp))
            TextButton(onClick = { viewModel.skip() }, enabled = !state.busy) {
                Text(
                    "Skip setup entirely",
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ProjectStep(busy: Boolean, onContinue: (name: String, prefix: String, color: String) -> Unit) {
    var name by remember { mutableStateOf("") }
    var prefix by remember { mutableStateOf("") }
    var prefixEdited by remember { mutableStateOf(false) }
    var color by remember { mutableStateOf(LabelPalette.colors.first()) }

    Column(modifier = Modifier.fillMaxWidth().glassCard().padding(20.dp)) {
        Text("Create your first project", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurface)
        Spacer(Modifier.height(12.dp))
        OutlinedTextField(
            value = name,
            onValueChange = {
                name = it
                if (!prefixEdited) prefix = derivePrefix(it)
            },
            label = { Text("Project name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = prefix,
            onValueChange = {
                prefixEdited = true
                prefix = it.uppercase().take(10)
            },
            label = { Text("Issue prefix") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(14.dp))
        Text("Color", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary))
        Spacer(Modifier.height(8.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            LabelPalette.colors.forEach { c ->
                Box(
                    modifier = Modifier
                        .size(28.dp)
                        .clip(CircleShape)
                        .background(parseColor(c))
                        .then(
                            if (c == color) Modifier.border(2.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                            else Modifier,
                        )
                        .clickable { color = c },
                )
            }
        }
        Spacer(Modifier.height(18.dp))
        Button(
            enabled = !busy && name.isNotBlank() && prefix.isNotBlank(),
            onClick = { onContinue(name.trim(), prefix.trim().uppercase(), color) },
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(if (busy) "Creating…" else "Continue")
        }
    }
}

@Composable
private fun IssueStep(busy: Boolean, onCreate: (String) -> Unit, onSkip: () -> Unit) {
    var title by remember { mutableStateOf("") }
    Column(modifier = Modifier.fillMaxWidth().glassCard().padding(20.dp)) {
        Text("Create your first issue", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurface)
        Spacer(Modifier.height(12.dp))
        OutlinedTextField(
            value = title,
            onValueChange = { title = it },
            label = { Text("Issue title") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(18.dp))
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            TextButton(onClick = onSkip, enabled = !busy, modifier = Modifier.weight(1f)) { Text("Skip") }
            Button(
                enabled = !busy && title.isNotBlank(),
                onClick = { onCreate(title.trim()) },
                modifier = Modifier.weight(1f),
            ) {
                Text(if (busy) "Creating…" else "Create issue")
            }
        }
    }
}

@Composable
private fun ProgressDots(step: Int, count: Int) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        repeat(count) { i ->
            val active = i <= step
            Box(
                modifier = Modifier
                    .size(if (i == step) 10.dp else 8.dp)
                    .clip(CircleShape)
                    .background(
                        if (active) MaterialTheme.colorScheme.primary
                        else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                    ),
            )
            if (i < count - 1) {
                Box(
                    modifier = Modifier
                        .width(20.dp)
                        .height(2.dp)
                        .background(MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)),
                )
            }
        }
    }
}

private fun derivePrefix(name: String): String =
    name.trim().filter { it.isLetterOrDigit() }.take(3).uppercase()
