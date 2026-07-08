package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.domain.WorkspacePermissions
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// Shape name for workspace membership (SyncManager.launchShape "workspace_members").
internal const val MEMBERS_SHAPE = "workspace_members"

/**
 * Banner shown above issue content while a viewer's membership is still
 * syncing, so an un-synced member doesn't read as a silent permission denial.
 */
enum class SyncBanner { None, Syncing, Stalled }

/**
 * Show the banner only when the viewer is signed in but NOT yet a resolved
 * member (or admin) AND the workspace_members shape hasn't gone live — i.e. the
 * membership row simply hasn't landed yet. If that shape is currently erroring,
 * escalate the copy to point at Sync Diagnostics.
 */
internal fun syncBannerFor(
    permissions: WorkspacePermissions,
    membersShape: SyncStats.ShapeStatus?,
): SyncBanner {
    if (!permissions.isAuthed || permissions.isMember || permissions.isAdmin) return SyncBanner.None
    if (membersShape?.phase == "live") return SyncBanner.None
    return if ((membersShape?.consecutiveErrors ?: 0) > 0) SyncBanner.Stalled else SyncBanner.Syncing
}

/** Slim glass row rendered above issue content while [banner] is not [SyncBanner.None]. */
@Composable
fun SyncBannerRow(banner: SyncBanner, modifier: Modifier = Modifier) {
    if (banner == SyncBanner.None) return
    Row(
        modifier = modifier
            .fillMaxWidth()
            .glassRow()
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        CircularProgressIndicator(strokeWidth = 2.dp, modifier = Modifier.size(16.dp))
        Text(
            when (banner) {
                SyncBanner.Stalled -> "Sync stalled — open Settings → Sync diagnostics"
                else -> "Syncing workspace…"
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}
