package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import at.exponential.ui.host.FunctionCallInfo
import at.exponential.ui.json.JsonValue
import com.exponential.app.ui.apps.AppConsentCard
import com.exponential.app.ui.apps.AppConsentGate

const val AppConsentSpecimenTestId = "app-consent-specimen"

/**
 * VAPP-120: the styleguide's consent-card specimen (`sg_app-consent`): THE
 * card an app surface raises for a typed action nobody granted — the
 * operation's own words (`AppActions.actionConsent`), the arguments it would
 * send, Deny and Allow — drawn by the REAL [AppConsentCard] over a stand-in
 * surface. Capture only: the painted surface's rows carry no text semantics
 * the test could press, so the live walk cannot raise it; the call is the
 * Working hours demo's "Post to the issue" on the docs, admin entry.
 */
@Composable
fun AppConsentSpecimenScreen() {
    val gate = remember { AppConsentGate() }
    LaunchedEffect(gate) {
        gate.ask(
            FunctionCallInfo(
                surfaceId = "working-hours",
                componentId = "postComment",
                name = "exponential.comments.create",
                args = linkedMapOf(
                    "issueId" to JsonValue.Str("APP-15"),
                    "body" to JsonValue.Str("Logged 8 h: docs, admin"),
                ),
            ),
        )
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .testTag(AppConsentSpecimenTestId),
    ) {
        Column(modifier = Modifier.padding(24.dp)) {
            Text("Working hours", style = MaterialTheme.typography.titleLarge)
            Text("October 2026", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        AppConsentCard(gate)
    }
}
