package com.exponential.app.ui.support

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/**
 * The Support tab (EXP-180): the team helpdesk inbox as its own bottom-bar
 * destination — a tab that exists only while the active team's synced
 * `helpdesk_enabled` flag is on (AppNavHost gates it). Owns the screen chrome
 * in PersonalScreen's visual language; the list itself (filter pills, rows,
 * poll lifecycle) lives in [SupportInboxContent].
 */
@Composable
fun SupportScreen(
    onOpenThread: (String) -> Unit,
) {
    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "Support",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            SupportInboxContent(onOpenThread = onOpenThread)
        }
    }
}
