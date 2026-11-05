package com.exponential.app.data.api

import com.exponential.app.domain.ViaAppClaim
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
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
    /** VAPP-176: null (omitted) = a DRAFT named "Untitled app" (New app). */
    val name: String? = null,
    val icon: String? = null,
    val color: String? = null,
    val description: String? = null,
)

/** `apps.update`: every field but [id] optional (null = omitted, the
 *  client's `explicitNulls = false`). VAPP-120/156: [grantedTools] is the
 *  WHOLE grant list and rides with the [headVersion] on screen (a newer head
 *  = CONFLICT); only a team owner grants. */
@Serializable
data class AppUpdateInput(
    val id: String,
    val name: String? = null,
    val icon: String? = null,
    val color: String? = null,
    val description: String? = null,
    val headVersion: Int? = null,
    val grantedTools: List<String>? = null,
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
data class AppDiscardResult(val discarded: Boolean = false, val txid: Long? = null)

/** eApps V1 (#VAPP-210) `apps.publish`: the published version and the head. */
@Serializable
data class AppPublishResult(val published: Int, val head: Int = 0, val txid: Long? = null)

@Serializable
data class AppRestoreResult(val version: Int, val restored: Int, val txid: Long? = null)

@Serializable
data class AppGetResult(val version: Int = 0, val `package`: JsonElement? = null)

@Serializable
private data class TxResult(val txid: Long? = null)

// VAPP-122: `apps.records.upsert` / `apps.records.delete` (an app's own
// rows; `id` = the APP, `recordId` = the record).
@Serializable
private data class AppRecordUpsertInput(
    val id: String,
    val kind: String,
    val data: JsonObject,
    val recordId: String? = null,
    /** VAPP-188: an owner's on-behalf author (null = the writer; omitted). */
    val author: String? = null,
    /** VAPP-215: where the write came from (`editor` = the Data tab; null = omitted, the server says `app`). */
    val via: String? = null,
    /** VAPP-235: a create's CLIENT id (the outbox's; a taken one = CONFLICT). */
    val newId: String? = null,
    /** VAPP-235: the idempotency key (a replayed one answers the first result). */
    val writeId: String? = null,
    /** VAPP-231 (via-app.json `claim`): the host's claim (null = omitted). */
    val viaApp: ViaAppClaim? = null,
)

@Serializable
data class AppRecordUpsertResult(
    val id: String,
    val created: Boolean = false,
    val txid: Long? = null,
    /** VAPP-215: the write's `app_record_events` row (the Undo toast's). */
    val eventId: String? = null,
)

@Serializable
private data class AppRecordDeleteInput(val id: String, val recordId: String, val via: String? = null, val writeId: String? = null)

@Serializable
data class AppRecordDeleteResult(val txid: Long? = null, val eventId: String? = null)

// VAPP-215 (app-records.json `history`): a record's events, History's
// Restore and the toast's Undo (read on demand, no synced shape).
@Serializable
private data class AppRecordHistoryInput(val id: String, val recordId: String)

@Serializable
data class AppRecordEventRow(
    val id: String,
    val op: String,
    val actor: String? = null,
    val author: String? = null,
    val via: String? = null,
    val patch: JsonElement? = null,
    val before: JsonElement? = null,
    val createdAt: String = "",
)

@Serializable
data class AppRecordHistoryResult(val events: List<AppRecordEventRow> = emptyList())

@Serializable
private data class AppRecordRestoreInput(val id: String, val recordId: String, val eventId: String, val via: String? = null)

@Serializable
private data class AppRecordUndoInput(val id: String, val eventIds: List<String>, val via: String? = null)

@Serializable
data class AppRecordEventsResult(val txid: Long? = null, val eventIds: List<String> = emptyList())

// VAPP-194: `apps.records.batch`: a package action's record writes
// (`{op: "upsert", kind, data, id?}` | `{op: "delete", id}`, the plan's
// `writes` passed through unchanged) as ONE transaction.
@Serializable
private data class AppRecordBatchInput(val id: String, val writes: JsonArray, val writeId: String? = null, val via: String? = null, val viaApp: ViaAppClaim? = null)

@Serializable
data class AppRecordBatchResult(
    val txid: Long? = null,
    val ids: List<String> = emptyList(),
    /** VAPP-215: the batch's events (the Undo toast's; none = nothing changed). */
    val eventIds: List<String> = emptyList(),
)

// VAPP-121: `apps.validateImport` / `apps.import` (fixtures/app-export.json):
// the SERVER is the one authority on a pasted app file or bare package.
@Serializable
private data class AppUpstreamReadInput(val id: String, val source: String)

@Serializable
private data class AppImportInput(val teamId: String, val text: String)

/** #VAPP-254: an owner's pick on the import step (`apps.import`). */
@Serializable
data class AppUpstreamMapping(val upstream: String, val serverId: String)

@Serializable
private data class AppImportCreateInput(val teamId: String, val text: String, val upstreamMappings: List<AppUpstreamMapping>)

@Serializable
data class AppImportIssue(val path: String = "", val message: String = "")

/** #VAPP-254: one upstream the file declares and its exact-name default. */
@Serializable
data class AppImportUpstreamDto(val name: String = "", val serverId: String? = null, val serverName: String? = null)

@Serializable
data class AppValidateImportResult(
    val issues: List<AppImportIssue> = emptyList(),
    val name: String? = null,
    val records: Int = 0,
    val upstreams: List<AppImportUpstreamDto> = emptyList(),
    /** What an owner may pick; empty for a member. */
    val servers: List<AppServerDto> = emptyList(),
)

/** VAPP-234 `apps.importTemplate` (fixtures/app-templates.json). */
@Serializable
private data class AppImportTemplateInput(val teamId: String, val template: String, val includeRecords: Boolean)

@Serializable
data class AppImportResult(val app: AppRow, val records: Int = 0, val txid: Long? = null)

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

    /** Rename (name, icon, colour, description) or VAPP-120 the grants. */
    suspend fun update(accountId: String, input: AppUpdateInput) {
        trpc.mutation(
            accountId,
            path = "apps.update",
            input = input,
            inputSerializer = AppUpdateInput.serializer(),
            outputSerializer = AppWriteResult.serializer(),
        )
    }

    /** VAPP-176: leaving an EMPTY draft's editor deletes it (the server
     *  checks: still no version, no editor session, my own; else a no-op). */
    suspend fun discardDraft(accountId: String, appId: String): AppDiscardResult =
        trpc.mutation(
            accountId,
            path = "apps.discardDraft",
            input = AppIdInput(appId),
            inputSerializer = AppIdInput.serializer(),
            outputSerializer = AppDiscardResult.serializer(),
        )

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

    /** eApps V1 `apps.publish {id, number}`: members and the app's tools run
     *  version [number] from now on (an older one = a rollback). */
    suspend fun publish(accountId: String, appId: String, number: Int): AppPublishResult =
        trpc.mutation(
            accountId,
            path = "apps.publish",
            input = AppRestoreInput(appId, number),
            inputSerializer = AppRestoreInput.serializer(),
            outputSerializer = AppPublishResult.serializer(),
        )

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
    suspend fun recordsUpsert(
        accountId: String,
        appId: String,
        kind: String,
        data: JsonObject,
        recordId: String?,
        author: String? = null,
        via: String? = null,
        /** VAPP-235: a create's client id (the outbox replay). */
        newId: String? = null,
        /** VAPP-235: the outbox entry's idempotency key. */
        writeId: String? = null,
        /** VAPP-231: the host's claim. */
        viaApp: ViaAppClaim? = null,
    ): AppRecordUpsertResult =
        trpc.mutation(
            accountId,
            path = "apps.records.upsert",
            input = AppRecordUpsertInput(appId, kind, data, recordId, author, via, newId, writeId, viaApp),
            inputSerializer = AppRecordUpsertInput.serializer(),
            outputSerializer = AppRecordUpsertResult.serializer(),
        )

    /** VAPP-194: a package action's [writes] in app [appId], all or none
     *  (the server checks each against its kind's schema, the caps and the
     *  quota again). VAPP-228: [via] = `import` for the Data tab's CSV
     *  import (each write may carry an owner's on-behalf `author`). */
    suspend fun recordsBatch(accountId: String, appId: String, writes: JsonArray, writeId: String? = null, via: String? = null, viaApp: ViaAppClaim? = null): AppRecordBatchResult =
        trpc.mutation(
            accountId,
            path = "apps.records.batch",
            input = AppRecordBatchInput(appId, writes, writeId, via, viaApp),
            inputSerializer = AppRecordBatchInput.serializer(),
            outputSerializer = AppRecordBatchResult.serializer(),
        )

    /** VAPP-122: trash record [recordId] of app [appId]. */
    suspend fun recordsDelete(accountId: String, appId: String, recordId: String, via: String? = null, writeId: String? = null): AppRecordDeleteResult =
        trpc.mutation(
            accountId,
            path = "apps.records.delete",
            input = AppRecordDeleteInput(appId, recordId, via, writeId),
            inputSerializer = AppRecordDeleteInput.serializer(),
            outputSerializer = AppRecordDeleteResult.serializer(),
        )

    /** VAPP-215: record [recordId]'s events, newest first. */
    suspend fun recordsHistory(accountId: String, appId: String, recordId: String): AppRecordHistoryResult =
        trpc.query(
            accountId,
            path = "apps.records.history",
            input = AppRecordHistoryInput(appId, recordId),
            inputSerializer = AppRecordHistoryInput.serializer(),
            outputSerializer = AppRecordHistoryResult.serializer(),
        )

    /** VAPP-215: History's Restore: the record as event [eventId] left it, as a new write. */
    suspend fun recordsRestore(accountId: String, appId: String, recordId: String, eventId: String, via: String? = null): AppRecordEventsResult =
        trpc.mutation(
            accountId,
            path = "apps.records.restore",
            input = AppRecordRestoreInput(appId, recordId, eventId, via),
            inputSerializer = AppRecordRestoreInput.serializer(),
            outputSerializer = AppRecordEventsResult.serializer(),
        )

    /** VAPP-215: the toast's Undo: each event's `before` as a new write, newest first. */
    suspend fun recordsUndo(accountId: String, appId: String, eventIds: List<String>, via: String? = null): AppRecordEventsResult =
        trpc.mutation(
            accountId,
            path = "apps.records.undo",
            input = AppRecordUndoInput(appId, eventIds, via),
            inputSerializer = AppRecordUndoInput.serializer(),
            outputSerializer = AppRecordEventsResult.serializer(),
        )

    /** VAPP-230: the broker's answer for an `upstream:<server>/<tool>?…`
     *  binding [source] of app [appId], read as the viewer: `{ok: true,
     *  value, meta}` | `{ok: false, error, server, message}`. */
    suspend fun upstreamRead(accountId: String, appId: String, source: String): JsonObject =
        trpc.query(
            accountId,
            path = "apps.upstreamRead",
            input = AppUpstreamReadInput(appId, source),
            inputSerializer = AppUpstreamReadInput.serializer(),
            outputSerializer = JsonObject.serializer(),
        )

    /** VAPP-121: the server's issues with [text] (an app file or a bare
     *  package); none = importable. */
    suspend fun validateImport(accountId: String, teamId: String, text: String): AppValidateImportResult =
        trpc.mutation(
            accountId,
            path = "apps.validateImport",
            input = AppImportInput(teamId, text),
            inputSerializer = AppImportInput.serializer(),
            outputSerializer = AppValidateImportResult.serializer(),
        )

    /** VAPP-234: a new app out of the signed gallery template [template]
     *  (its sample records when [includeRecords]). */
    suspend fun importTemplate(accountId: String, teamId: String, template: String, includeRecords: Boolean): AppImportResult =
        trpc.mutation(
            accountId,
            path = "apps.importTemplate",
            input = AppImportTemplateInput(teamId, template, includeRecords),
            inputSerializer = AppImportTemplateInput.serializer(),
            outputSerializer = AppImportResult.serializer(),
        )

    /** VAPP-121: create the app [text] describes (package as v1, its records). */
    suspend fun importApp(accountId: String, teamId: String, text: String, upstreamMappings: List<AppUpstreamMapping> = emptyList()): AppImportResult =
        trpc.mutation(
            accountId,
            path = "apps.import",
            input = AppImportCreateInput(teamId, text, upstreamMappings),
            inputSerializer = AppImportCreateInput.serializer(),
            outputSerializer = AppImportResult.serializer(),
        )
}
