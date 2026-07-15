package com.exponential.app.ui.update

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.net.Uri
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
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.Image
import com.exponential.app.AppConstants
import com.exponential.app.R
import com.exponential.app.data.api.UpdateGate
import com.google.android.play.core.appupdate.AppUpdateManagerFactory
import com.google.android.play.core.appupdate.AppUpdateOptions
import com.google.android.play.core.install.model.AppUpdateType
import com.google.android.play.core.install.model.UpdateAvailability

private const val IMMEDIATE_UPDATE_REQUEST_CODE = 5104

/**
 * Full-screen blocking gate shown when the server 426s this build (below its
 * minimum version, EXP-104). It floats on the app's [AppBackground] gradient
 * (its caller already supplies it), matching the login/instance screens. The
 * only action is "Update": production builds launch Play's immediate in-app
 * update flow, and any failure — plus every staging build — falls through to a
 * plain Play Store link.
 */
@Composable
fun UpdateRequiredScreen(info: UpdateGate.UpgradeInfo) {
    val context = LocalContext.current

    Column(
        modifier = Modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.systemBars)
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.Start,
    ) {
        Image(
            painter = painterResource(R.mipmap.ic_launcher),
            contentDescription = null,
            modifier = Modifier.size(64.dp),
        )
        Spacer(Modifier.height(24.dp))
        Text(
            "Update required",
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.height(8.dp))
        Text(
            buildUpdateBody(info.min),
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
    }
}

private fun buildUpdateBody(min: String?): String {
    val base =
        "This version of Exponential is no longer supported. Please update to keep using the app."
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
    // Strip the staging suffix so the link targets the published production app.
    val appId = context.packageName.removeSuffix(".staging")
    val marketIntent = Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$appId"))
    try {
        context.startActivity(marketIntent)
    } catch (_: ActivityNotFoundException) {
        context.startActivity(
            Intent(
                Intent.ACTION_VIEW,
                Uri.parse("https://play.google.com/store/apps/details?id=$appId"),
            ),
        )
    }
}

private fun Context.findActivity(): Activity? {
    var ctx: Context = this
    while (ctx is ContextWrapper) {
        if (ctx is Activity) return ctx
        ctx = ctx.baseContext
    }
    return null
}
