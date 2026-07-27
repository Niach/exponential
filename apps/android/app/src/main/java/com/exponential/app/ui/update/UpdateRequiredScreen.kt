package com.exponential.app.ui.update

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.Image
import com.exponential.app.AppConstants
import com.exponential.app.PlayStore
import com.exponential.app.R
import com.exponential.app.data.api.UpdateGate
import com.google.android.play.core.appupdate.AppUpdateManagerFactory
import com.google.android.play.core.appupdate.AppUpdateOptions
import com.google.android.play.core.install.model.AppUpdateType
import com.google.android.play.core.install.model.UpdateAvailability

private const val IMMEDIATE_UPDATE_REQUEST_CODE = 5104

/**
 * Full-screen blocking gate shown when the ACTIVE account's server 426s this
 * build (below its minimum version, EXP-104). It floats on the app's
 * [AppBackground] gradient (its caller already supplies it), matching the
 * login/instance screens. The primary action is "Update": production builds
 * launch Play's immediate in-app update flow, and any failure — plus every
 * staging build — falls through to a plain Play Store link.
 *
 * The secondary action is the escape hatch (REV2-18): a self-hosted server can
 * demand a version Play doesn't ship, and "Update" can never satisfy it — so
 * signing out of that server has to be reachable from here, not only by
 * clearing app data.
 */
@Composable
fun UpdateRequiredScreen(
    info: UpdateGate.UpgradeInfo,
    serverLabel: String?,
    onSignOutOfServer: () -> Unit,
) {
    val context = LocalContext.current
    var confirmSignOut by remember { mutableStateOf(false) }

    if (confirmSignOut) {
        AlertDialog(
            onDismissRequest = { confirmSignOut = false },
            title = { Text("Sign out of this server?") },
            text = {
                Text(
                    "This removes ${serverLabel ?: "this server"} and its offline copy " +
                        "from this device. Your other servers stay signed in.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    confirmSignOut = false
                    onSignOutOfServer()
                }) { Text("Sign out") }
            },
            dismissButton = {
                TextButton(onClick = { confirmSignOut = false }) { Text("Cancel") }
            },
        )
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.systemBars)
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.Start,
    ) {
        // MUST be the plain vector, never R.mipmap.ic_launcher: the launcher
        // icon only exists as an adaptive-icon XML (mipmap-anydpi-v26), which
        // painterResource rejects with an IllegalArgumentException — crashing
        // the app at the exact moment the 426 gate tries to show this screen
        // (EXP-138). The foreground's logo circle spans 58/108 of the adaptive
        // viewport (EXP-143 geometry), so scale it back up to a 64dp visual;
        // the outer clip cuts the stroke tails that overflow the circle
        // (normally hidden by the adaptive-icon mask). Keep this factor in
        // sync with the circle diameter in ic_launcher_foreground.xml.
        Image(
            painter = painterResource(R.drawable.ic_launcher_foreground),
            contentDescription = null,
            modifier = Modifier
                .size(64.dp)
                .clip(CircleShape)
                .scale(108f / 58f),
        )
        Spacer(Modifier.height(24.dp))
        Text(
            "Update required",
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.height(8.dp))
        Text(
            buildUpdateBody(info.min, serverLabel),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(24.dp))
        Button(
            onClick = { startUpdate(context) },
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Update")
        }
        Spacer(Modifier.height(8.dp))
        TextButton(
            onClick = { confirmSignOut = true },
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Sign out of this server")
        }
    }
}

// Naming the server matters here: the gate is per instance (REV2-18), so the
// user has to be able to tell "the app is outdated" from "this one server
// wants a version Play doesn't ship yet".
private fun buildUpdateBody(min: String?, serverLabel: String?): String {
    val base = if (serverLabel != null) {
        "$serverLabel needs a newer version of Exponential. Please update to keep using it."
    } else {
        "This version of Exponential is no longer supported. Please update to keep using the app."
    }
    return if (min != null) "$base The minimum supported version is $min." else base
}

/**
 * Launch Play's immediate in-app update on production; fall back to the store
 * page on staging or on ANY failure (Play unavailable, no update ready, an
 * exception starting the flow).
 */
private fun startUpdate(context: Context) {
    if (AppConstants.IS_STAGING) {
        openStorePage(context)
        return
    }
    val activity = context.findActivity()
    if (activity == null) {
        openStorePage(context)
        return
    }
    runCatching {
        val manager = AppUpdateManagerFactory.create(context)
        manager.appUpdateInfo
            .addOnSuccessListener { appUpdateInfo ->
                val canImmediate =
                    appUpdateInfo.updateAvailability() == UpdateAvailability.UPDATE_AVAILABLE &&
                        appUpdateInfo.isUpdateTypeAllowed(AppUpdateType.IMMEDIATE)
                if (canImmediate) {
                    runCatching {
                        manager.startUpdateFlowForResult(
                            appUpdateInfo,
                            activity,
                            AppUpdateOptions.newBuilder(AppUpdateType.IMMEDIATE).build(),
                            IMMEDIATE_UPDATE_REQUEST_CODE,
                        )
                    }.onFailure { openStorePage(context) }
                } else {
                    openStorePage(context)
                }
            }
            .addOnFailureListener { openStorePage(context) }
    }.onFailure { openStorePage(context) }
}

private fun openStorePage(context: Context) {
    PlayStore.openListing(context)
}

private fun Context.findActivity(): Activity? {
    var ctx: Context = this
    while (ctx is ContextWrapper) {
        if (ctx is Activity) return ctx
        ctx = ctx.baseContext
    }
    return null
}
