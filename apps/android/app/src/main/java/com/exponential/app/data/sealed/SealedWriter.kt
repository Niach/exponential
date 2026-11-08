package com.exponential.app.data.sealed

import android.util.Log
import com.exponential.app.data.api.AnswerInput
import com.exponential.app.data.api.CreateSessionKeyInput
import com.exponential.app.data.api.InstallsApi
import com.exponential.app.data.api.SealInput
import com.exponential.app.data.api.SessionKeysApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SessionsSendResult
import com.exponential.app.data.api.TreeKeyWrap
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.enet.EnetFfi
import com.exponential.app.data.enet.EnetIdentityStore
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.enet.base64Url
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

/** A send ready to go: [input] (sealed when it could be), [onSent] keeps a new root's key. */
class PreparedSend(val input: SessionsSendInput, val onSent: (SessionsSendResult) -> Unit = {})

/** The tree a send writes into: its root id (null = a new root) and current key. */
internal data class SendTarget(val rootId: String?, val teamId: String?, val publicKey: String?, val epoch: Int?, val deviceId: String?)

/**
 * EXP-1459 writer (sealed-content.json + contract addendum 2 on #EXP-1458):
 * every person message and answer leaves this install SEALED for its tree.
 *
 * - A tree with a key: the text is sealed with its public key + epoch.
 * - An existing root without one: a fresh tree key, `sessionKeys.create`
 *   with wraps for the readers this install resolves, then seal.
 * - A new root (a chat, a first message to an agent / app): the key rides
 *   the send as `seal` (one server transaction) and is kept under the root
 *   the answer names.
 * - Mentions travel as `mentionUserIds`, `#IDENT`s as `refIssueIds`; attachment embeds go INSIDE the
 *   sealed body (the server never appends to it).
 *
 * Nothing to seal with (no registered install yet, no eNet library) = the
 * plaintext send of before: a body without the envelope stays legal.
 */
@Singleton
class SealedWriter @Inject constructor(
    private val dbs: DatabaseHolder,
    private val auth: AuthRepository,
    private val nodes: dagger.Lazy<EnetNodeManager>,
    private val identities: EnetIdentityStore,
    private val installs: InstallsApi,
    private val sessionKeys: SessionKeysApi,
    private val treeKeys: TreeKeyStore,
) {
    private val crypto: SealedCrypto get() = SealedContent.crypto

    suspend fun prepareSend(accountId: String, input: SessionsSendInput): PreparedSend = withContext(Dispatchers.IO) {
        val text = input.text.orEmpty()
        val attachments = input.attachmentIds.orEmpty()
        if (text.isBlank() && attachments.isEmpty()) return@withContext PreparedSend(input)
        val db = dbs.database(accountId)
        val mentions = mentionIds(db, text)
        val refs = issueRefIds(db, text)
        val withMentions = input.copy(
            mentionUserIds = mentions.takeIf { it.isNotEmpty() },
            refIssueIds = refs.takeIf { it.isNotEmpty() },
        )
        try {
            val target = target(db, accountId, input) ?: return@withContext PreparedSend(withMentions)
            val body = SealedWrite.withAttachmentEmbeds(text, attachments)
            val rootId = target.rootId
            if (rootId != null && target.publicKey != null && target.epoch != null) {
                return@withContext PreparedSend(withMentions.copy(text = crypto.sealBody(target.publicKey, target.epoch, body)))
            }
            val installId = nodes.get().installId(accountId) ?: return@withContext PreparedSend(withMentions)
            val key = crypto.generateTreeKey()
            val wraps = readerWraps(accountId, installId, target, key.secret) ?: return@withContext PreparedSend(withMentions)
            val sealed = crypto.sealBody(key.publicKey, FIRST_EPOCH, body)
            if (rootId != null) {
                // An existing keyless root: its key first, then the sealed text.
                sessionKeys.create(accountId, CreateSessionKeyInput(rootId, installId, FIRST_EPOCH, key.publicKey, wraps))
                keep(accountId, rootId, key.secret)
                return@withContext PreparedSend(withMentions.copy(text = sealed))
            }
            PreparedSend(
                withMentions.copy(text = sealed, seal = SealInput(installId, key.publicKey, FIRST_EPOCH, wraps)),
            ) { result -> keep(accountId, result.sessionId, key.secret) }
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            // No eNet library, or the server refused the key: plaintext stays legal.
            Log.w(TAG, "sealing a send failed: ${err.javaClass.simpleName}")
            PreparedSend(withMentions)
        }
    }

    /**
     * A sealed card's answer = the FULL card re-sealed with it (`sealedCard`,
     * clear `decision` + `fresh`), no plaintext answer. A plain card answers
     * as before. A sealed card this install cannot open refuses: the answer
     * would leak in clear.
     */
    suspend fun prepareAnswer(accountId: String, input: AnswerInput): AnswerInput = withContext(Dispatchers.IO) {
        val db = dbs.database(accountId)
        // #EXP-1561: a dismiss carries no answer; the server stamps the clear
        // fields beside a sealed card's envelope.
        if (input.dismissed == true) return@withContext input
        val row = db.messageDao().getById(input.messageId) ?: return@withContext input
        val raw = row.card?.let { runCatching { Json.parseToJsonElement(it) as? JsonObject }.getOrNull() }
        if (!SealedContent.isSealedCard(raw)) return@withContext input
        val opened = SealedContent.openCard(row.rootId, raw!!)
        val root = db.sessionDao().getById(row.rootId)
        // #EXP-1641: a root not synced here (a notification action) asks the server.
        val key = root?.sealPublicKey?.let { it to root.sealEpoch } ?: publicKey(accountId, row.rootId)
        val publicKey = key?.first
        val epoch = key?.second
        if (SealedContent.isSealedCard(opened) || publicKey == null || epoch == null) throw TrpcException(LOCKED_ANSWER)
        val answered = SealedWrite.answeredCard(opened, input) ?: throw TrpcException(LOCKED_ANSWER)
        val sealedCard = Json.parseToJsonElement(crypto.sealCard(publicKey, epoch, answered.toString())) as JsonObject
        AnswerInput(
            messageId = input.messageId,
            decision = input.decision,
            fresh = input.fresh,
            sealedCard = sealedCard,
        )
    }

    /**
     * #EXP-1641: a message into the ENCRYPTED tree [rootId], named explicitly
     * (a notification's reply: the session row may not be synced here). Sealed
     * with that tree's public key (the root row's, else the server's); no key
     * = refused, never the plaintext fallback of [prepareSend].
     */
    suspend fun prepareSealedSend(accountId: String, input: SessionsSendInput, rootId: String): PreparedSend =
        withContext(Dispatchers.IO) {
            val text = input.text.orEmpty()
            if (text.isBlank()) throw TrpcException(LOCKED_SEND)
            val db = dbs.database(accountId)
            val mentions = mentionIds(db, text)
            val refs = issueRefIds(db, text)
            val root = db.sessionDao().getById(rootId)
            val key = root?.sealPublicKey?.let { it to root.sealEpoch } ?: publicKey(accountId, rootId)
            val publicKey = key?.first ?: throw TrpcException(LOCKED_SEND)
            val epoch = key.second ?: throw TrpcException(LOCKED_SEND)
            val body = SealedWrite.withAttachmentEmbeds(text, input.attachmentIds.orEmpty())
            PreparedSend(
                input.copy(
                    text = crypto.sealBody(publicKey, epoch, body),
                    mentionUserIds = mentions.takeIf { it.isNotEmpty() },
                    refIssueIds = refs.takeIf { it.isNotEmpty() },
                ),
            )
        }

    private suspend fun mentionIds(db: ExponentialDatabase, text: String): List<String> {
        val emails = SealedWrite.mentionEmails(text)
        if (emails.isEmpty()) return emptyList()
        return runCatching { db.userDao().byEmails(emails).map { it.id }.distinct() }.getOrDefault(emptyList())
    }

    /** The text's `#IDENT` refs this install knows (the server never parses a sealed text). */
    private suspend fun issueRefIds(db: ExponentialDatabase, text: String): List<String> {
        val identifiers = SealedWrite.issueRefs(text)
        if (identifiers.isEmpty()) return emptyList()
        return runCatching { db.issueDao().byIdentifiers(identifiers).map { it.id }.distinct() }.getOrDefault(emptyList())
    }

    /** The tree [input] writes into; null when it cannot be told (sent in clear). */
    private suspend fun target(db: ExponentialDatabase, accountId: String, input: SessionsSendInput): SendTarget? {
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
        val existing: SessionEntity? = when {
            input.sessionId != null -> db.sessionDao().getById(input.sessionId) ?: return null
            input.parentId != null -> db.sessionDao().getById(input.parentId) ?: return null
            input.agentId != null -> userId?.let { db.sessionDao().agentConversation(input.agentId, it) }
            input.appId != null -> userId?.let { db.sessionDao().appConversation(input.appId, it) }
            else -> null
        }
        if (existing != null) {
            val rootId = existing.rootId.ifEmpty { existing.id }
            val root = if (rootId == existing.id) existing else db.sessionDao().getById(rootId) ?: return null
            val key = root.sealPublicKey?.let { it to root.sealEpoch } ?: publicKey(accountId, rootId)
            return SendTarget(rootId, root.teamId, key?.first, key?.second, root.deviceId)
        }
        val teamId = input.teamId
            ?: input.agentId?.let { db.agentDefinitionDao().observeById(it).first()?.teamId }
            ?: input.appId?.let { db.appDao().observeById(it).first()?.teamId }
        return SendTarget(rootId = null, teamId = teamId, publicKey = null, epoch = null, deviceId = input.deviceId)
    }

    /** A root whose row has not synced its key yet: ask the server (absent = keyless). */
    private suspend fun publicKey(accountId: String, rootId: String): Pair<String, Int?>? =
        runCatching { sessionKeys.publicKeys(accountId, listOf(rootId)).firstOrNull { it.rootId == rootId } }
            .getOrNull()?.let { it.publicKey to it.epoch }

    /**
     * Wraps for the readers this install resolves (addendum 1): its own
     * install always, the user's other installs, and the target device's
     * install when `installs.list` names it. Holders fill any gap later via
     * `pending`. Null when this install's own wrap cannot be made.
     */
    private suspend fun readerWraps(accountId: String, installId: String, target: SendTarget, secret: ByteArray): List<TreeKeyWrap>? {
        val identity = identities.vault.identity(accountId, create = false) ?: return null
        val own = TreeKeyWrap(installId, crypto.wrapTreeKey(base64Url(EnetFfi.x25519PublicOf(identity)), secret))
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
        val teamInstalls = target.teamId?.let { teamId -> runCatching { installs.list(accountId, teamId) }.getOrNull() }.orEmpty()
        val others = teamInstalls
            .filter { it.installId != installId }
            .filter { it.userId == userId || (target.deviceId != null && it.deviceId == target.deviceId) }
            .mapNotNull { reader ->
                runCatching { TreeKeyWrap(reader.installId, crypto.wrapTreeKey(reader.x25519Public, secret)) }.getOrNull()
            }
        return listOf(own) + others
    }

    private fun keep(accountId: String, rootId: String, secret: ByteArray) {
        val keys = mapOf(rootId to mapOf(FIRST_EPOCH to secret))
        treeKeys.vault.put(accountId, keys)
        SealedKeys.merge(keys)
        nodes.get().wakeHolders()
    }

    private companion object {
        const val TAG = "SealedWriter"
        const val FIRST_EPOCH = 1
        const val LOCKED_ANSWER = "This card can't be answered on this device yet."
        const val LOCKED_SEND = "This chat can't be written to from this device yet."
    }
}
