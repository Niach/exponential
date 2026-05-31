package com.exponential.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class SyncDiagnosticsViewModel @Inject constructor(
    auth: AuthRepository,
    stats: SyncStats,
) : ViewModel() {
    // The active account's shapes, sorted by name (parity with iOS, which only
    // shows the active account).
    val shapes: StateFlow<List<SyncStats.ShapeStatus>> =
        combine(auth.activeAccountId, stats.state) { accountId, all ->
            all[accountId]?.values?.sortedBy { it.shape } ?: emptyList()
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SyncDiagnosticsScreen(
    onBack: () -> Unit,
    viewModel: SyncDiagnosticsViewModel = hiltViewModel(),
) {
    val shapes by viewModel.shapes.collectAsStateWithLifecycle()

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            TopAppBar(
                title = { Text("Sync Diagnostics") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        if (shapes.isEmpty()) {
            Box(Modifier.padding(padding).fillMaxSize()) {
                EmptyState(message = "No active sync.")
            }
            return@Scaffold
        }
        LazyColumn(
            modifier = Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            item {
                SectionHeader("Electric Shapes")
                Spacer(Modifier.size(8.dp))
            }
            items(shapes, key = { it.shape }) { status ->
                ShapeRow(status)
            }
            item {
                Spacer(Modifier.size(12.dp))
                Text(
                    "Live polling status for each Electric shape on the active account.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

@Composable
private fun ShapeRow(status: SyncStats.ShapeStatus) {
    // The dot reflects CURRENT health, not the lifetime error tally: a long-gone
    // transient blip (errorCount > 0 but consecutiveErrors == 0) reads as healthy.
    val isUnauthorized = status.phase == "unauthorized"
    val dot = when {
        isUnauthorized -> Color(0xFFEF4444)              // red: persistent auth failure
        status.consecutiveErrors > 0 -> Color(0xFFF97316) // orange: currently failing
        status.phase == "live" -> Color(0xFF22C55E)       // green: healthy
        status.phase == "initial" -> Color(0xFF3B82F6)    // blue: initial sync
        else -> Color(0xFFA1A1AA)                          // grey: idle
    }
    val phaseLabel = if (isUnauthorized) "unauthorized" else status.phase
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(10.dp).background(dot, CircleShape))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                status.shape,
                style = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                phaseLabel,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(
                "${status.rowsApplied} rows",
                style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            // Show a count only while the shape is CURRENTLY failing. A genuinely
            // failing shape still surfaces a non-zero count + colored dot here.
            if (isUnauthorized) {
                Text(
                    "unauthorized",
                    style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                    color = MaterialTheme.colorScheme.error,
                )
            } else if (status.consecutiveErrors > 0) {
                Text(
                    "${status.consecutiveErrors} errors",
                    style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}
