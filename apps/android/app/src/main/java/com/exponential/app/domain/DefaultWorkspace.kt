package com.exponential.app.domain

import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceEntity

/**
 * Default workspace for the global selection when none is set (EXP-166/EXP-168):
 * the workspace of the last-opened project (the Issues root's context), else the
 * first workspace that HAS a project (mirroring AppViewModel.currentProject's
 * fallback, so Agents/Reviews scope to the same workspace whose project the
 * Issues root shows), else the first synced workspace. Null for an EMPTY
 * workspace list — "Resync now" wipes tables before refetching, and selecting
 * off that transient would pin an arbitrary row (same caveat as iOS
 * AppNavigator's non-empty-emissions rule).
 */
fun defaultWorkspaceId(
    workspaces: List<WorkspaceEntity>,
    projects: List<ProjectEntity>,
    lastProjectId: String?,
): String? {
    if (workspaces.isEmpty()) return null
    val lastProjectWorkspace = lastProjectId
        ?.let { last -> projects.firstOrNull { it.id == last }?.workspaceId }
        ?.let { wsId -> workspaces.firstOrNull { it.id == wsId }?.id }
    return lastProjectWorkspace
        ?: workspaces.firstOrNull { ws -> projects.any { it.workspaceId == ws.id } }?.id
        ?: workspaces.first().id
}
