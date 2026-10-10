package com.exponential.app.ui.auth

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.theme.TextEmphasis

/** The legal pair under every sign-in list ×4 (web `auth-form-shell.tsx`). */
object AuthLegalLinks {
    const val PRIVACY_URL = "https://exponential.at/privacy/"
    const val TERMS_URL = "https://exponential.at/terms/"
}

/**
 * The sign-in page frame ×4 (web `AuthFormShell`, EXP-1176): the brand mark
 * over a centred title, the flow's content on the bare page (no card), the
 * muted "Privacy · Terms" pair closing it.
 */
@Composable
fun AuthShell(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.systemBars)
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(
            modifier = Modifier.widthIn(max = 420.dp).fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            ExponentialMark(size = 48.dp)
            Spacer(Modifier.height(16.dp))
            Text(
                title,
                style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.SemiBold),
                color = Color.White,
                textAlign = TextAlign.Center,
            )
            if (subtitle != null) {
                Spacer(Modifier.height(4.dp))
                Text(
                    subtitle,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Color.White.copy(alpha = TextEmphasis.Secondary),
                    textAlign = TextAlign.Center,
                )
            }
            Spacer(Modifier.height(24.dp))
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.Start,
                content = content,
            )
            Spacer(Modifier.height(24.dp))
            PrivacyTermsFooter()
        }
    }
}

/** Muted "Privacy · Terms", each half opening its page in a Custom Tab. */
@Composable
fun PrivacyTermsFooter(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val muted = Color.White.copy(alpha = TextEmphasis.Tertiary)
    val style = MaterialTheme.typography.labelSmall
    fun open(url: String) {
        CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
    }
    Row(
        modifier = modifier.testTag("auth-legal-links"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "Privacy",
            style = style,
            color = muted,
            modifier = Modifier.clickable { open(AuthLegalLinks.PRIVACY_URL) },
        )
        Text(" · ", style = style, color = muted)
        Text(
            "Terms",
            style = style,
            color = muted,
            modifier = Modifier.clickable { open(AuthLegalLinks.TERMS_URL) },
        )
    }
}

/**
 * The method the cloud chooser picked ("Continue with email" / "Login with
 * passkey"): the login screen consumes it once its config loads and opens
 * that method at once, so the tap never has to be repeated.
 */
object PendingLoginMethod {
    const val EMAIL = "email"
    const val PASSKEY = "passkey"

    @Volatile
    var value: String? = null

    fun consume(): String? = value.also { value = null }
}
