package com.exponential.app.ui.components

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.OnboardingCopy
import com.exponential.app.ui.theme.glassCard
import kotlinx.coroutines.delay

/**
 * The invite-link creator (EXP-725): one button that mints a shareable link,
 * then the link itself with Copy and Share.
 *
 * Hosted by the first-run wizard's invite step and by team settings → Members
 * (owner-only there — minting is owner-only server-side). Both keep the
 * ViewModel keyed on the team:
 *
 *     val vm = hiltViewModel<InviteLinkViewModel>(key = "invite-link:$teamId")
 *     LaunchedEffect(teamId) { vm.bind(teamId) }
 *
 * At the seat cap the whole card is GONE — see [InviteLinkState.hidden]. It
 * must not degrade into a disabled button or an explanatory line: store
 * billing policy (EXP-216) forbids pointing at an outside purchase, and the
 * neutral plan-limit sentence is still an explanation of a paywall.
 */
@Composable
fun InviteLinkCard(
    state: InviteLinkState,
    onGenerate: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (state.hidden) return

    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) {
        if (copied) {
            delay(2_000)
            copied = false
        }
    }
    // A fresh link resets the flash, so "Copied" can't outlive the URL it
    // described.
    LaunchedEffect(state.inviteUrl) { copied = false }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .glassCard()
            .padding(20.dp)
            .testTag("invite-link-creator"),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val url = state.inviteUrl
        if (url == null) {
            GlassSubmitButton(
                label = OnboardingCopy.INVITE_GENERATE,
                onClick = onGenerate,
                enabled = !state.generating,
                modifier = Modifier.testTag("invite-generate"),
            )
        } else {
            Text(
                url,
                style = MaterialTheme.typography.bodySmall.copy(
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.Normal,
                ),
                color = MaterialTheme.colorScheme.onSurface,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                GlassPill(
                    label = if (copied) OnboardingCopy.INVITE_COPIED else OnboardingCopy.INVITE_COPY,
                    icon = if (copied) ExpIcons.uiCheck else ExpIcons.uiCopy,
                    onClick = {
                        clipboard.setText(AnnotatedString(url))
                        copied = true
                    },
                )
                GlassPill(
                    label = OnboardingCopy.SHARE,
                    icon = ExpIcons.uiShare,
                    onClick = {
                        val send = Intent(Intent.ACTION_SEND).apply {
                            type = "text/plain"
                            putExtra(Intent.EXTRA_TEXT, url)
                        }
                        runCatching {
                            context.startActivity(
                                Intent.createChooser(send, OnboardingCopy.SHARE),
                            )
                        }
                    },
                )
            }
        }
        // Only NON-plan failures ever reach here (the ViewModel swallows the
        // cap by hiding the card).
        state.error?.let { message ->
            Text(
                message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error.copy(alpha = 0.8f),
            )
        }
    }
}

/**
 * The card plus its keyed ViewModel — what both hosts actually mount.
 */
@Composable
fun InviteLinkCard(teamId: String, modifier: Modifier = Modifier) {
    val viewModel = hiltViewModel<InviteLinkViewModel>(key = "invite-link:$teamId")
    LaunchedEffect(teamId) { viewModel.bind(teamId) }
    val state by viewModel.state.collectAsStateWithLifecycle()
    InviteLinkCard(
        state = state,
        onGenerate = viewModel::generate,
        modifier = modifier,
    )
}
