package com.exponential.app.data.api

import com.exponential.app.domain.SidebarFolders
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

// #EXP-2109 (apps/web/src/lib/trpc/sidebar-layout.ts): the member's own sidebar
// layout. The client runs the pure `SidebarFolders` writes over its synced rows
// and sends them here.

@Serializable
data class SidebarWriteInput(
    val id: String?,
    val kind: String,
    val targetId: String?,
    val parentId: String?,
    val name: String?,
    val sortOrder: Double,
)

@Serializable
data class SidebarSaveInput(
    val teamId: String,
    val section: String,
    val upserts: List<SidebarWriteInput>,
    val deletes: List<String>,
)

@Serializable
private data class SidebarRenameInput(val id: String, val name: String)

@Serializable
private data class SidebarRemoveInput(val targetId: String)

@Singleton
class SidebarLayoutApi @Inject constructor(private val trpc: TrpcClient) {
    /** One move's rows. */
    suspend fun save(accountId: String, teamId: String, section: String, writes: SidebarFolders.Writes) =
        trpc.mutationUnit(
            accountId,
            path = "sidebarLayout.save",
            input = SidebarSaveInput(
                teamId = teamId,
                section = section,
                upserts = writes.upserts.map { SidebarWriteInput(it.id, it.kind, it.targetId, it.parentId, it.name, it.sortOrder) },
                deletes = writes.deletes,
            ),
            inputSerializer = SidebarSaveInput.serializer(),
        )

    /** One of the caller's folders (empty = the default name). */
    suspend fun rename(accountId: String, folderId: String, name: String) =
        trpc.mutationUnit(accountId, "sidebarLayout.rename", SidebarRenameInput(folderId, name), SidebarRenameInput.serializer())

    /** The caller's entry of a placed session. */
    suspend fun remove(accountId: String, targetId: String) =
        trpc.mutationUnit(accountId, "sidebarLayout.remove", SidebarRemoveInput(targetId), SidebarRemoveInput.serializer())
}
