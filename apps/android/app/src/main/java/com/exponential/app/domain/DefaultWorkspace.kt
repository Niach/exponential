package com.exponential.app.domain

import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceEntity

/**
 * Default workspace for the global selection when none is set (EXP-166/EXP-168):
 * the workspace of the last-opened project (the Issues root's context), else the
 * first synced workspace. Null for an EMPTY workspace list — "Resync now" wipes
 * tables before refetching, and selecting off that transient would pin an
 * arbitrary row (same caveat as iOS AppNavigator's non-empty-emissions rule).
 */
fun defaultWorkspaceId(
    workspaces: List<WorkspaceEntity>,
    projects: List<ProjectEntity>,
    lastProjectId: String?,
): String? {
    if (workspaces.isEmpty()) return null
    val lastProjectWorkspace = lastProjectId
        ?.let { last -> projects.firstOrNull { it.id == last }?.workspaceId }
    return workspaces.firstOrNull { it.id == lastProjectWorkspace }?.id
        ?: workspaces.first().id
}
