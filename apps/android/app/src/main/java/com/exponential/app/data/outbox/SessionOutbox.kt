package com.exponential.app.data.outbox

import android.content.Context
import com.exponential.app.Observability
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SessionsSendResult
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.OutboxEntity
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.domain.PendingAttachment
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import java.time.Instant
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * #FEED-237 (message-delivery.json): THE session outbox. A session
 * composer's Send puts an entry here at the press (persisted in the account
 * DB's `outbox` table, so a restart keeps it) and the send (uploads, then
 * `sessions.send` with `clientMessageId` = the entry's clientId) runs on this
 * app-wide scope, outliving the screen. Success stamps the entry's
 * messageId; failure = state `failed` (the bubble's red mark, no toast). On
 * start every `sending` entry the server never answered is sent AGAIN with
 * the same clientMessageId (F21: the server returns the row it already
 * stored); a `failed` one waits for its Retry.
 *
 * FILES: the picks not uploaded yet are part of the entry: their bytes are
 * copied under the app's files dir (`outbox/<clientId>/<n>`) at the press
 * and listed in its `pending_files`; the send uploads them one by one (each
 * landed id joins `attachment_ids`, the file leaves the list and the disk),
 * then sends, so a restart resumes the remaining uploads. A file that is
 * gone fails the entry (Retry tries again, Delete drops it).
 */
@Singleton
class SessionOutbox @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val sessionsApi: SessionsApi,
    private val issueImagesApi: IssueImagesApi,
) {
    /** How a send builds its wire input from the text and the uploaded ids
     *  (the composer's picks ride it); [onSent] runs after the server took it. */
    class Builder(
        val input: (text: String, attachmentIds: List<String>) -> SessionsSendInput,
        val onSent: (SessionsSendResult) -> Unit = {},
    )

    private val scope = CoroutineScope(
        SupervisorJob() + Dispatchers.IO +
            CoroutineExceptionHandler { _, e -> Observability.nonFatal(TAG, "outbox send failed", e) },
    )

    private val builders = ConcurrentHashMap<String, Builder>()
    private val inFlight = ConcurrentHashMap.newKeySet<String>()
    private val resent = ConcurrentHashMap.newKeySet<String>()
    @Volatile private var started = false

    /** At launch (and on each new sign-in): resend the unanswered entries. */
    fun start() {
        if (started) return
        started = true
        scope.launch {
            auth.accounts
                .map { list -> list.filter { it.token != null }.map { it.id }.toSet() }
                .distinctUntilChanged()
                .collect { ids ->
                    for (id in ids) {
                        if (!resent.add(id)) continue
                        scope.launch {
                            val dao = holder.database(id).outboxDao()
                            for (entry in dao.unanswered()) run(id, entry.clientId)
                        }
                    }
                }
        }
    }

    /** The press: a new `sending` entry at the bottom; returns its clientId. */
    fun submit(
        accountId: String,
        sessionId: String,
        text: String,
        images: List<PendingAttachment>,
        builder: Builder,
    ): String {
        val clientId = UUID.randomUUID().toString()
        val landed = images.mapNotNull { it.uploadedId }
        val waiting = images.filter { it.uploadedId == null }
        builders[clientId] = builder
        scope.launch {
            // The bytes go to disk first: the entry never lists a file that
            // was not written (one that failed to write fails the send).
            val files = OutboxFiles.plan(filesDir(clientId), waiting.map { it.filename to it.contentType })
            for ((file, pick) in files.zip(waiting)) {
                try {
                    File(file.path).apply { parentFile?.mkdirs() }.writeBytes(pick.bytes)
                } catch (t: Throwable) {
                    Observability.nonFatal(TAG, "outbox file write failed", t)
                }
            }
            holder.database(accountId).outboxDao().upsert(
                OutboxEntity(
                    clientId = clientId,
                    sessionId = sessionId,
                    text = text,
                    createdAt = Instant.now().toString(),
                    state = MessageDelivery.Local.Sending.wire,
                    messageId = null,
                    attachmentIds = landed.takeIf { it.isNotEmpty() }?.joinToString(","),
                    pendingFiles = OutboxFiles.encode(files),
                ),
            )
            run(accountId, clientId)
        }
        return clientId
    }

    /** A failed bubble's Retry: the SAME clientId, back to `sending`. */
    fun retry(accountId: String, clientId: String) {
        scope.launch {
            holder.database(accountId).outboxDao().setState(clientId, MessageDelivery.Local.Sending.wire)
            run(accountId, clientId)
        }
    }

    /** A failed bubble's Delete: the entry goes, its text with it. */
    fun delete(accountId: String, clientId: String) {
        builders.remove(clientId)
        scope.launch {
            holder.database(accountId).outboxDao().delete(listOf(clientId))
            removeFiles(clientId)
        }
    }

    /** `outboxMerge`'s `drop`: their synced rows took their place. */
    fun drop(accountId: String, clientIds: List<String>) {
        if (clientIds.isEmpty()) return
        clientIds.forEach { builders.remove(it) }
        scope.launch {
            holder.database(accountId).outboxDao().delete(clientIds)
            clientIds.forEach(::removeFiles)
        }
    }

    private suspend fun run(accountId: String, clientId: String) {
        if (!inFlight.add(clientId)) return
        val dao = holder.database(accountId).outboxDao()
        try {
            val entry = dao.get(clientId) ?: return
            var uploads = OutboxFiles.Uploads(entry.attachmentIdList, OutboxFiles.decode(entry.pendingFiles))
            while (uploads.pending.isNotEmpty()) {
                val file = uploads.pending.first()
                // Gone or unreadable = the catch below fails the entry.
                val bytes = File(file.path).readBytes()
                val uploaded = issueImagesApi.uploadSessionImage(
                    accountId, entry.sessionId, bytes, file.filename, file.contentType,
                )
                uploads = uploads.landed(file, uploaded.id)
                dao.setUploads(clientId, uploads.attachmentIdsWire, uploads.pendingWire)
                File(file.path).delete()
            }
            val ids = uploads.attachmentIds
            val builder = builders[clientId]
            val input = builder?.input?.invoke(entry.text, ids)
                ?: SessionsSendInput.message(entry.sessionId, entry.text, ids.takeIf { it.isNotEmpty() })
            val result = sessionsApi.send(accountId, input.copy(clientMessageId = clientId))
            builder?.onSent?.invoke(result)
            val messageId = result.messageId
            when {
                // The message went to another session (an app editor that
                // continued elsewhere): its row never lands in this one.
                result.sessionId != entry.sessionId -> drop(accountId, listOf(clientId))
                messageId != null -> dao.setMessageId(clientId, messageId)
                // An older server answers no id: the text match takes over.
                else -> Unit
            }
            removeFiles(clientId)
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            dao.setState(clientId, MessageDelivery.Local.Failed.wire)
        } finally {
            inFlight.remove(clientId)
        }
    }

    private fun filesDir(clientId: String): File = OutboxFiles.dir(context.filesDir, clientId)

    private fun removeFiles(clientId: String) {
        runCatching { filesDir(clientId).deleteRecursively() }
    }

    private companion object {
        const val TAG = "SessionOutbox"
    }
}
