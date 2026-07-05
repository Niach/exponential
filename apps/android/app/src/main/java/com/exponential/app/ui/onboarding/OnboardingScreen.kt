package com.exponential.app.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Devices
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// First-run screen. The mobile app is a companion (masterplan L26) — workspaces
// and projects are created on the web or desktop app, so onboarding is a single
// informational screen instead of a create-project/issue wizard. Continue marks
// onboarding complete so the nav gate stops showing it; the ViewModel also
// self-heals a stale account via reconcile() before the user taps Continue.
@Composable
fun OnboardingScreen(
    onDone: () -> Unit,
    viewModel: OnboardingViewModel = hiltViewModel(),
) {
    val instanceUrl by viewModel.instanceUrl.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val done by viewModel.done.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.reconcile() }
    LaunchedEffect(done) { if (done) onDone() }

    val host = instanceUrl?.let { runCatching { android.net.Uri.parse(it).host }.getOrNull() ?: it }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 32.dp, vertical = 48.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                "Welcome to Exponential",
                style = MaterialTheme.typography.headlineSmall,
                color = MaterialTheme.colorScheme.onSurface,
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(24.dp))

            Column(
                modifier = Modifier
                    .widthIn(max = 460.dp)
                    .fillMaxWidth()
                    .glassCard()
                    .padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        Icons.Filled.Devices,
                        contentDescription = null,
                        modifier = Modifier.size(24.dp),
                        tint = MaterialTheme.colorScheme.onSurface,
                    )
                    Spacer(Modifier.width(12.dp))
                    Text(
                        "Create your first project on the web or desktop app",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
                Text(
                    "This app is your companion for tracking and updating issues on the go. " +
                        "Set up workspaces and projects — and start coding — from the web or " +
                        "desktop app, then everything syncs here.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                if (host != null) {
                    Text(
                        host,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(Color.White.copy(alpha = 0.06f))
                            .padding(horizontal = 14.dp, vertical = 10.dp),
                    )
                }
            }

            Spacer(Modifier.height(24.dp))
            Button(
                enabled = !busy,
                onClick = { viewModel.finish() },
                modifier = Modifier.widthIn(max = 460.dp).fillMaxWidth(),
            ) {
                Text(if (busy) "Finishing…" else "Continue")
            }
        }
    }
}
