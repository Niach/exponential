package com.exponential.app.domain

import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-166/EXP-168: the app-level selection bootstrap must prefer the workspace
// of the last-opened project (the Issues root's context) and never select off
// an empty (mid-resync) workspace list.
class DefaultWorkspaceTest {

    private fun workspace(id: String) = WorkspaceEntity(
        id = id,
        name = "Workspace $id",
        slug = id,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    private fun project(id: String, workspaceId: String) = ProjectEntity(
        id = id,
        workspaceId = workspaceId,
        name = "Project $id",
        slug = id,
        prefix = id.take(3).uppercase(),
        color = "#8b5cf6",
        sortOrder = 1.0,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    @Test
    fun lastProjectWorkspaceWins() {
        val workspaces = listOf(workspace("ws-personal"), workspace("ws-team"))
        val projects = listOf(project("p1", "ws-personal"), project("p2", "ws-team"))
        assertEquals("ws-team", defaultWorkspaceId(workspaces, projects, "p2"))
    }

    @Test
    fun danglingLastProjectFallsToFirstWorkspaceWithAProject() {
        // Mirrors AppViewModel.currentProject: with no usable last-project the
        // Issues root shows the first workspace's first project — ws-b here, so
        // the selection must scope there too (a projectless personal workspace
        // must not win just by sorting first).
        val workspaces = listOf(workspace("ws-personal"), workspace("ws-b"))
        val projects = listOf(project("p1", "ws-b"))
        assertEquals("ws-b", defaultWorkspaceId(workspaces, projects, "p-deleted"))
    }

    @Test
    fun noProjectsAnywhereFallsToFirstWorkspace() {
        val workspaces = listOf(workspace("ws-a"), workspace("ws-b"))
        assertEquals("ws-a", defaultWorkspaceId(workspaces, emptyList(), null))
    }

    @Test
    fun lastProjectInUnsyncedWorkspaceFallsToFirst() {
        // The project row points at a workspace that hasn't synced (yet) — the
        // selection must stay valid against the live workspace list.
        val workspaces = listOf(workspace("ws-a"))
        val projects = listOf(project("p1", "ws-gone"))
        assertEquals("ws-a", defaultWorkspaceId(workspaces, projects, "p1"))
    }

    @Test
    fun emptyWorkspacesYieldNull() {
        // "Resync now" wipes tables before refetching — never pin a selection
        // off that transient.
        assertNull(defaultWorkspaceId(emptyList(), emptyList(), "p1"))
    }
}
