package com.exponential.app.ui.settings

import android.content.Intent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import com.exponential.app.AppConstants
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassSection

private const val SOURCE_URL = "https://github.com/Niach/exponential"
private const val LICENSE_URL = "https://github.com/Niach/exponential/blob/master/LICENSE"

/**
 * Settings → About (EXP-262): the app's version surface plus the third-party
 * licence acknowledgements every distributed build must carry. The licences
 * themselves are one push further ([ThirdPartyLicensesScreen]) so the notice
 * blob never weighs down this screen.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AboutScreen(
    onOpenThirdPartyLicenses: () -> Unit,
    onBack: () -> Unit,
) {
    val context = LocalContext.current

    AppBackground {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                CenterAlignedTopAppBar(
                    title = { Text("About") },
                    navigationIcon = {
                        TopBarBackButton(onClick = onBack)
                    },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                        containerColor = Color.Transparent,
                    ),
                )
            },
        ) { padding ->
            Column(
                modifier = Modifier
                    .padding(padding)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                    AboutRow(
                        icon = ExpIcons.settingsAbout,
                        title = "Exponential",
                        subtitle = "Version ${AppConstants.VERSION_NAME}",
                        trailingIcon = null,
                        onClick = null,
                    )
                }
                Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                    AboutRow(
                        icon = ExpIcons.settingsLicenses,
                        title = "Third-party licenses",
                        trailingIcon = ExpIcons.uiChevronRight,
                        onClick = onOpenThirdPartyLicenses,
                    )
                    AboutDivider()
                    AboutRow(
                        icon = ExpIcons.uiGithub,
                        title = "Source code",
                        trailingIcon = ExpIcons.uiExternalLink,
                        onClick = {
                            context.startActivity(Intent(Intent.ACTION_VIEW, SOURCE_URL.toUri()))
                        },
                    )
                    AboutDivider()
                    AboutRow(
                        icon = ExpIcons.uiInfo,
                        title = "License (Apache-2.0)",
                        trailingIcon = ExpIcons.uiExternalLink,
                        onClick = {
                            context.startActivity(Intent(Intent.ACTION_VIEW, LICENSE_URL.toUri()))
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun AboutRow(
    icon: ImageVector,
    title: String,
    subtitle: String? = null,
    trailingIcon: ImageVector?,
    onClick: (() -> Unit)?,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .let { if (onClick != null) it.clickable(onClick = onClick) else it }
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(22.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (subtitle != null) {
                Text(
                    subtitle,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        if (trailingIcon != null) {
            Spacer(Modifier.width(8.dp))
            Icon(
                trailingIcon,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            )
        }
    }
}

@Composable
private fun AboutDivider() {
    HorizontalDivider(thickness = 0.5.dp, color = Color.White.copy(alpha = 0.06f))
}
