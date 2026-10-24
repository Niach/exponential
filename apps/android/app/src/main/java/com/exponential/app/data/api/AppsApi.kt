package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// VAPP-110: the team's APPS (apps/web/src/lib/trpc/apps.ts). The list and
// the head package sync on the `apps` shape, the versions on `app_versions`;
// an older version's package loads on demand (`apps.get {version}`). Create /
// rename / delete = the creator or an owner (delete), Restore = any member
// (a NEW head copying that version).

@Serializable
data class AppCreateInput(
    val teamId: String,
    val name: String,
    val icon: String? = null,
    val color: String? = null,
    val description: String? = null,
)

@Serializable
data class AppUpdateInput(
    val id: String,
    val name: String,
    val icon: String? = null,
    val color: String? = null,
    val description: String,
)

@Serializable
data class AppRow(val id: String, val slug: String = "", val name: String = "")

@Serializable
data class AppWriteResult(val app: AppRow, val txid: Long? = null)

@Serializable
private data class AppIdInput(val id: String)

@Serializable
private data class AppVersionInput(val id: String, val version: Int)

@Serializable
private data class AppRestoreInput(val id: String, val number: Int)

@Serializable
data class AppRestoreResult(val version: Int, val restored: Int, val txid: Long? = null)

@Serializable
data class AppGetResult(val version: Int = 0, val `package`: JsonElement? = null)

@Serializable
private data class TxResult(val txid: Long? = null)

// VAPP-122: `apps.records.upsert` / `apps.records.delete` (an app's own
// rows; `id` = the APP, `recordId` = the record).
@Serializable
private data class AppRecordUpsertInput(val id: String, val kind: String, val data: JsonObject, val recordId: String? = null)

@Serializable
data class AppRecordUpsertResult(val id: String, val created: Boolean = false, val txid: Long? = null)

@Serializable
private data class AppRecordDeleteInput(val id: String, val recordId: String)

@Singleton
class AppsApi @Inject constructor(private val trpc: TrpcClient) {

    /** A new app in [AppCreateInput.teamId] (the slug derives server-side). */
    suspend fun create(accountId: String, input: AppCreateInput): AppRow =
        trpc.mutation(
            accountId,
            path = "apps.create",
            input = input,
            inputSerializer = AppCreateInput.serializer(),
            outputSerializer = AppWriteResult.serializer(),
        ).app

    /** Rename: name, icon, colour and description. */
    suspend fun update(accountId: String, input: AppUpdateInput) {
        trpc.mutation(
            accountId,
            path = "apps.update",
            input = input,
            inputSerializer = AppUpdateInput.serializer(),
            outputSerializer = AppWriteResult.serializer(),
        )
    }

    /** Trash the app (the shape's `deleted_at` mirror hides it). */
    suspend fun delete(accountId: String, appId: String) {
        trpc.mutation(
            accountId,
            path = "apps.delete",
            input = AppIdInput(appId),
            inputSerializer = AppIdInput.serializer(),
            outputSerializer = TxResult.serializer(),
        )
    }

    /** Restore version [number] as the new head. */
    suspend fun restore(accountId: String, appId: String, number: Int): AppRestoreResult =
        trpc.mutation(
            accountId,
            path = "apps.restore",
            input = AppRestoreInput(appId, number),
            inputSerializer = AppRestoreInput.serializer(),
            outputSerializer = AppRestoreResult.serializer(),
        )

    /** Version [version]'s package (JSON text; null = none). */
    suspend fun versionPackage(accountId: String, appId: String, version: Int): String? =
        trpc.query(
            accountId,
            path = "apps.get",
            input = AppVersionInput(appId, version),
            inputSerializer = AppVersionInput.serializer(),
            outputSerializer = AppGetResult.serializer(),
        ).`package`?.takeIf { it !is kotlinx.serialization.json.JsonNull }?.toString()

    /** VAPP-122: create a record of [kind] in app [appId], or (with
     *  [recordId]) shallow-patch that record's data. The server validates
     *  [data] against the kind's schema, the byte cap and the plan quota. */
    suspend fun recordsUpsert(accountId: String, appId: String, kind: String, data: JsonObject, recordId: String?): AppRecordUpsertResult =
        trpc.mutation(
            accountId,
            path = "apps.records.upsert",
            input = AppRecordUpsertInput(appId, kind, data, recordId),
            inputSerializer = AppRecordUpsertInput.serializer(),
            outputSerializer = AppRecordUpsertResult.serializer(),
        )

    /** VAPP-122: trash record [recordId] of app [appId]. */
    suspend fun recordsDelete(accountId: String, appId: String, recordId: String) {
        trpc.mutation(
            accountId,
            path = "apps.records.delete",
            input = AppRecordDeleteInput(appId, recordId),
            inputSerializer = AppRecordDeleteInput.serializer(),
            outputSerializer = TxResult.serializer(),
        )
    }
}
