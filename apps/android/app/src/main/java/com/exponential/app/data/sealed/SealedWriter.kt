package com.exponential.app.data.sealed

import android.util.Log
import com.exponential.app.data.api.AnswerInput
import com.exponential.app.data.api.CreateSessionKeyInput
import com.exponential.app.data.api.InstallsApi
import com.exponential.app.data.api.SealInput
import com.exponential.app.data.api.SessionKeysApi
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SessionsSendResult
import com.exponential.app.data.api.TreeKeyWrap
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.chatTeamIssues
import com.exponential.app.data.crossTeamOrder
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.agentChat
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.enet.EnetFfi
import com.exponential.app.data.enet.EnetIdentityStore
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.enet.base64Url
import com.exponential.app.domain.AgentSettings
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

/** A send ready to go: [input] (sealed when it could be), [onSent] keeps a new root's key. */
class PreparedSend(val input: SessionsSendInput, val onSent: (SessionsSendResult) -> Unit = {})

/** #EXP-1958: a replies / suggestions card as a tap reads it ([SealedWriter.cardTap]). */
class CardTap(val sealed: Boolean, val opened: JsonObject?, val sessionId: String?)

/** Who a send's tree key is wrapped for: its team and target device. */
internal data class Readers(val teamId: String?, val deviceId: String?)

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
 * #EDE-97 (`clearTree`): NO SILENT PLAINTEXT. Only a `clear_tree` root (an
 * outside MCP client's) takes a text in clear and is never keyed; nothing to
 * seal with after the waits = the send fails with [NO_KEY_REFUSAL].
 *
 * #FEED-250 (`staleTree`): Room is a CACHE of the tree ([StaleTree]): a
 * conversation with an agent / app it lacks asks `sessions.sendTarget`.
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
    private val sessions: dagger.Lazy<SessionsApi>,
) {
    private val crypto: SealedCrypto get() = SealedContent.crypto

    /**
     * [fresh] = the server refused this send's key write once: resolve the tree from the server alone.
     *
     * #EDE-97 (sealed-content.json `clearTree`, [sendWire]): a text goes out in
     * clear ONLY into a `clear_tree` root; otherwise sealed, after waiting for
     * the key holder ([SEAL_HOLDER_WAIT_MS]) and a keyless tree's key
     * ([SEAL_KEY_WAIT_MS]), or the send FAILS with [NO_KEY_REFUSAL] (the
     * failed-send toast; the draft stays). An attachment-only send has no text
     * and is not held to it (sealed when it can be, as before).
     */
    suspend fun prepareSend(accountId: String, input: SessionsSendInput, fresh: Boolean = false): PreparedSend = withContext(Dispatchers.IO) {
        val text = input.text.orEmpty()
        val attachments = input.attachmentIds.orEmpty()
        if (text.isBlank() && attachments.isEmpty() && input.actionCard == null) return@withContext PreparedSend(input)
        val db = dbs.database(accountId)
        val mentions = mentionIds(db, text)
        val refs = issueRefIds(db, accountId, input, text)
        val withMentions = input.copy(
            mentionUserIds = mentions.takeIf { it.isNotEmpty() },
            refIssueIds = refs.takeIf { it.isNotEmpty() },
        )
        val body = SealedWrite.withAttachmentEmbeds(text, attachments)
        if (text.isBlank() && input.actionCard == null) {
            return@withContext try {
                attachmentsOnly(db, accountId, input, withMentions, body, fresh)
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                Log.w(TAG, "sealing an attachment-only send failed: ${err.javaClass.simpleName}")
                PreparedSend(withMentions)
            }
        }
        try {
            sealed(db, accountId, input, withMentions, body, fresh)
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            // Never the plaintext send of before: nothing leaves.
            if (err is TrpcException && err.message == NO_KEY_REFUSAL) throw err
            Log.w(TAG, "sealing a send failed: ${err.javaClass.simpleName}")
            throw TrpcException(NO_KEY_REFUSAL)
        }
    }

    /** A text send ([sendWire]): plaintext into a clear tree, sealed otherwise, else [NO_KEY_REFUSAL]. */
    private suspend fun sealed(
        db: ExponentialDatabase,
        accountId: String,
        input: SessionsSendInput,
        withMentions: SessionsSendInput,
        body: String,
        fresh: Boolean,
    ): PreparedSend {
        val (first, readers) = target(db, accountId, input, fresh)
        // A tree Room cannot tell yet (its root not synced): wait for it, then ask the server.
        val wire: TreeWire = if (first != TreeWire.Unknown) first else {
            awaitWithin(SEAL_KEY_WAIT_MS, POLL_MS) {
                target(db, accountId, input, fresh).first.takeIf { it != TreeWire.Unknown }
            } ?: target(db, accountId, input, fresh = true).first
        }
        val tree = when (wire) {
            is TreeWire.Clear -> SendTree.Clear
            TreeWire.Inline -> SendTree.New
            is TreeWire.Sealed -> SendTree.Keyed
            is TreeWire.Create -> SendTree.Keyless
            TreeWire.Unknown -> throw TrpcException(NO_KEY_REFUSAL)
        }
        if (tree == SendTree.Clear) return PreparedSend(withMentions)
        val installId = nodes.get().awaitInstallId(accountId, SEAL_HOLDER_WAIT_MS)
        val holder = if (installId != null && identities.vault.identity(accountId, create = false) != null) SendHolder.Ready else SendHolder.None
        if (refusesUpFront(tree, holder, hasInstall = installId != null) || installId == null) throw TrpcException(NO_KEY_REFUSAL)
        return when (wire) {
            is TreeWire.Sealed -> sealedTo(withMentions, allowed(accountId, wire.rootId, wire.key) ?: throw TrpcException(NO_KEY_REFUSAL), body)
            is TreeWire.Create -> {
                // #EXP-1960 (`keylessCases`): `reader` = this install keys it; a
                // failed create takes [keylessStep]: a blip is retried, a tree
                // this install may not key asks its devices first, never a
                // silent wait.
                when (val keyed = keyKeyless(db, accountId, wire.rootId, readers)) {
                    is Keyless.Clear -> PreparedSend(withMentions)
                    is Keyless.Created -> sealedTo(withMentions, keyed.key, body)
                    is Keyless.Arrived -> when (sendWire(tree, holder, reader = false, keyArrives = keyed.key != null)) {
                        SendWire.Sealed -> sealedTo(withMentions, allowed(accountId, wire.rootId, keyed.key!!) ?: throw TrpcException(NO_KEY_REFUSAL), body)
                        else -> throw TrpcException(NO_KEY_REFUSAL)
                    }
                }
            }
            else -> {
                val key = crypto.generateTreeKey()
                val targets = readerTargets(accountId, installId, readers) ?: throw TrpcException(NO_KEY_REFUSAL)
                val wraps = wrapsFor(accountId, targets, key, rootId = null) ?: throw TrpcException(NO_KEY_REFUSAL)
                val sealed = crypto.sealBody(key.publicKey, FIRST_EPOCH, body)
                PreparedSend(
                    sealedName(withMentions, key.publicKey, FIRST_EPOCH).copy(
                        text = sealed,
                        seal = SealInput(installId, key.publicKey, FIRST_EPOCH, wraps),
                        actionCard = sealedActionCard(withMentions.actionCard, key.publicKey, FIRST_EPOCH),
                    ),
                ) { result -> keepSeal(accountId, result.sessionId, key, targets, wraps) }
            }
        }
    }

    /** An attachment-only send (`clearTree`: unchanged): its embeds sealed when this install can, plain otherwise. */
    private suspend fun attachmentsOnly(
        db: ExponentialDatabase,
        accountId: String,
        input: SessionsSendInput,
        withMentions: SessionsSendInput,
        body: String,
        fresh: Boolean,
    ): PreparedSend {
        val (wire, readers) = target(db, accountId, input, fresh)
        val treeKey = when (wire) {
            TreeWire.Unknown, is TreeWire.Clear -> return PreparedSend(withMentions)
            is TreeWire.Sealed -> allowed(accountId, wire.rootId, wire.key) ?: return PreparedSend(withMentions)
            is TreeWire.Create -> StaleTree.keyFirst(
                wire.rootId,
                create = { createKey(accountId, wire.rootId, readers) },
                publicKey = { publicKey(accountId, it) },
            ) ?: return PreparedSend(withMentions)
            TreeWire.Inline -> null
        }
        if (treeKey != null) return sealedTo(withMentions, treeKey, body)
        val installId = nodes.get().installId(accountId) ?: return PreparedSend(withMentions)
        val key = crypto.generateTreeKey()
        val targets = readerTargets(accountId, installId, readers) ?: return PreparedSend(withMentions)
        val wraps = wrapsFor(accountId, targets, key, rootId = null) ?: return PreparedSend(withMentions)
        val sealed = crypto.sealBody(key.publicKey, FIRST_EPOCH, body)
        return PreparedSend(
            withMentions.copy(text = sealed, seal = SealInput(installId, key.publicKey, FIRST_EPOCH, wraps)),
        ) { result -> keepSeal(accountId, result.sessionId, key, targets, wraps) }
    }

    private sealed interface Keyless {
        data object Clear : Keyless
        data class Created(val key: PublicTreeKey) : Keyless
        /** Keyed by another (its device, a concurrent writer); null = no key: the send fails. */
        data class Arrived(val key: PublicTreeKey?) : Keyless
    }

    /** #EXP-1960: a keyless root keyed by this install, else by its devices (`sessionKeys.requestKey`). */
    private suspend fun keyKeyless(db: ExponentialDatabase, accountId: String, rootId: String, readers: Readers): Keyless {
        var ownOnly = false
        var attempt = 1
        while (true) {
            val failure = try {
                val created = createKey(accountId, rootId, readers, ownOnly)
                    ?: return Keyless.Arrived(null)
                return Keyless.Created(created)
            } catch (err: CancellationException) {
                throw err
            } catch (err: TrpcException) {
                createFailure(err.code, err.message)
            } catch (err: Exception) {
                CreateFailure.Transient
            }
            when (keylessStep(failure, attempt, ownOnly)) {
                KeylessStep.Plain -> return Keyless.Clear
                KeylessStep.ServerKey -> return publicKey(accountId, rootId)?.let { Keyless.Arrived(it) } ?: askDevice(db, accountId, rootId)
                KeylessStep.RequestKey -> return askDevice(db, accountId, rootId)
                KeylessStep.RetryOwn -> ownOnly = true
                KeylessStep.Retry -> delay(SEAL_CREATE_RETRY_MS * attempt)
                else -> return Keyless.Arrived(null)
            }
            attempt++
        }
    }

    /** The tree's devices asked to key it, then its key waited for; no host = fails at once. */
    private suspend fun askDevice(db: ExponentialDatabase, accountId: String, rootId: String): Keyless {
        var answer = RequestKeyAnswer.Error
        val result = try {
            sessionKeys.requestKey(accountId, rootId)
        } catch (err: CancellationException) {
            throw err
        } catch (err: Exception) {
            if (err is TrpcException && err.code == "NOT_FOUND") answer = RequestKeyAnswer.NotFound
            null
        }
        if (result != null) {
            answer = RequestKeyAnswer.entries.firstOrNull { it.name.replaceFirstChar(Char::lowercase) == result.state } ?: RequestKeyAnswer.Error
        }
        return when (keylessStep(answer)) {
            KeylessStep.Plain -> Keyless.Clear
            KeylessStep.Failed -> Keyless.Arrived(null)
            KeylessStep.Sealed -> Keyless.Arrived(
                result?.let { r -> r.publicKey?.let { key -> r.epoch?.let { PublicTreeKey(key, it) } } },
            )
            else -> waitForKey(db, accountId, rootId)
        }
    }

    private suspend fun waitForKey(db: ExponentialDatabase, accountId: String, rootId: String): Keyless =
        Keyless.Arrived(awaitWithin(SEAL_KEY_WAIT_MS, KEY_POLL_MS) { currentKey(db, accountId, rootId) })

    private fun sealedTo(input: SessionsSendInput, key: PublicTreeKey, body: String) =
        PreparedSend(
            sealedName(input, key.publicKey, key.epoch).copy(
                // #EXP-2067: a press without a body sends no text at all.
                text = if (body.isEmpty() && input.actionCard != null) "" else crypto.sealBody(key.publicKey, key.epoch, body),
                actionCard = sealedActionCard(input.actionCard, key.publicKey, key.epoch),
            ),
        )

    /** #EXP-2067: a keyed tree takes the action card sealed only: `{kind: "action", sealed}`. */
    private fun sealedActionCard(card: JsonObject?, publicKey: String, epoch: Int): JsonObject? {
        card ?: return null
        val sealed = Json.parseToJsonElement(crypto.sealCard(publicKey, epoch, card.toString())) as JsonObject
        return JsonObject(mapOf("kind" to card.getValue("kind"), "sealed" to sealed.getValue("sealed")))
    }

    /** EXP-1458: a new session's `name` leaves as `sealedName` (the plaintext title stays neutral). */
    private fun sealedName(input: SessionsSendInput, publicKey: String, epoch: Int): SessionsSendInput {
        val name = input.name?.takeIf { it.isNotBlank() } ?: return input.copy(name = null)
        return input.copy(name = null, sealedName = crypto.sealBody(publicKey, epoch, name))
    }

    /**
     * #EXP-1958 (`cardStamps`): the stored replies / suggestions card [messageId]
     * names, as a tap reads it: [CardTap.sealed] = the stored card has a
     * `sealed` field, [CardTap.opened] = it opened here (stamps laid over its
     * items by `openCard`), null when it stays sealed. Null = no such row here.
     */
    suspend fun cardTap(accountId: String, messageId: String): CardTap? = withContext(Dispatchers.IO) {
        val row = dbs.database(accountId).messageDao().getById(messageId) ?: return@withContext null
        val raw = row.card?.let { runCatching { Json.parseToJsonElement(it) as? JsonObject }.getOrNull() } ?: return@withContext null
        if (!SealedContent.isSealedCard(raw)) return@withContext CardTap(sealed = false, opened = raw, sessionId = row.sessionId)
        val opened = SealedContent.openCard(row.rootId, raw).takeIf { !SealedContent.isSealedCard(it) }
        CardTap(sealed = true, opened = opened, sessionId = row.sessionId)
    }

    /** A sealed Start's `issueIds` ([CardItemWire.startIssueIds]): `#IDENT`s this install's issues resolve. */
    suspend fun startIssueIds(accountId: String, ids: List<String>): List<String> = withContext(Dispatchers.IO) {
        if (ids.isEmpty()) return@withContext ids
        val db = dbs.database(accountId)
        val idents = ids.map { it.removePrefix("#").uppercase() }
        val known = runCatching { db.issueDao().byIdentifiers(idents) }.getOrDefault(emptyList())
            .associate { it.identifier.uppercase() to it.id }
        CardItemWire.startIssueIds(ids) { wanted -> wanted.mapNotNull { w -> known[w.uppercase()]?.let { w to it } }.toMap() }
    }

    /** A keyless root's key once one exists: Room's synced row first, else the server's word. */
    private suspend fun currentKey(db: ExponentialDatabase, accountId: String, rootId: String): PublicTreeKey? {
        val row = runCatching { db.sessionDao().getById(rootId) }.getOrNull()
        row?.sealPublicKey?.let { key -> row.sealEpoch?.let { return PublicTreeKey(key, it) } }
        return publicKey(accountId, rootId)
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
        val key = root?.sealPublicKey?.let { k -> root.sealEpoch?.let { PublicTreeKey(k, it) } } ?: publicKey(accountId, row.rootId)
        // #EXP-1950 R2 + R3: only to a key the rule seals to.
        val allowedKey = key?.let { allowed(accountId, row.rootId, it) }
        val publicKey = allowedKey?.publicKey
        val epoch = allowedKey?.epoch
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
     * = refused at once (no waits; #EDE-97: never plaintext, as [prepareSend]).
     */
    suspend fun prepareSealedSend(accountId: String, input: SessionsSendInput, rootId: String): PreparedSend =
        withContext(Dispatchers.IO) {
            val text = input.text.orEmpty()
            if (text.isBlank()) throw TrpcException(LOCKED_SEND)
            val db = dbs.database(accountId)
            val mentions = mentionIds(db, text)
            val refs = issueRefIds(db, accountId, input, text)
            val root = db.sessionDao().getById(rootId)
            val key = root?.sealPublicKey?.let { k -> root.sealEpoch?.let { PublicTreeKey(k, it) } } ?: publicKey(accountId, rootId)
            // #EXP-1950 R2 + R3: only to a key the rule seals to.
            val allowedKey = key?.let { allowed(accountId, rootId, it) } ?: throw TrpcException(LOCKED_SEND)
            val publicKey = allowedKey.publicKey
            val epoch = allowedKey.epoch
            val body = SealedWrite.withAttachmentEmbeds(text, input.attachmentIds.orEmpty())
            PreparedSend(
                sealedName(input, publicKey, epoch).copy(
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

    /** The text's `#IDENT` refs this install knows (the server never parses a sealed text).
     *  #EXP-2079 (agent-settings.json `assistant.chatTeams` (1)): a send into
     *  a team that is not synced (the Assistant's chat) resolves them in the
     *  chat teams; an identifier found in several = the first team's in order. */
    private suspend fun issueRefIds(db: ExponentialDatabase, accountId: String, input: SessionsSendInput, text: String): List<String> {
        val identifiers = SealedWrite.issueRefs(text)
        if (identifiers.isEmpty()) return emptyList()
        return runCatching {
            val issues = db.issueDao().byIdentifiers(identifiers)
            val teams = unsyncedSendChatTeams(db, accountId, input)
            val known = if (teams == null) issues else chatTeamIssues(issues, db.boardDao().observeAll().first(), teams)
            known.map { it.id }.distinct()
        }.getOrDefault(emptyList())
    }

    /** #EXP-2079: the chat teams of the tree [input] writes into while its
     *  own team is NOT one of the synced teams; null = a synced team (or
     *  nothing known): every team chat's send resolves as it always did. */
    private suspend fun unsyncedSendChatTeams(db: ExponentialDatabase, accountId: String, input: SessionsSendInput): List<String>? {
        val synced = crossTeamOrder(db.teamDao().observeAll().first()).map { it.id }
        if (synced.isEmpty()) return null
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
        val explicitId = input.sessionId ?: input.parentId
        val existing: SessionEntity? = when {
            explicitId != null -> db.sessionDao().getById(explicitId)
            input.agentId != null -> userId?.let { db.sessionDao().agentConversation(input.agentId, it) }
            input.appId != null -> userId?.let { db.sessionDao().appConversation(input.appId, it) }
            else -> null
        }
        val root = existing?.let { row ->
            val rootId = row.rootId.ifEmpty { row.id }
            if (rootId == row.id) row else db.sessionDao().getById(rootId)
        }
        val own = (root ?: existing)?.teamId?.takeIf { it.isNotEmpty() }
            ?: input.teamId
            ?: input.agentId?.let { db.agentDefinitionDao().observeById(it).first()?.teamId }
            ?: input.appId?.let { db.appDao().observeById(it).first()?.teamId }
        if (own.isNullOrEmpty() || own in synced) return null
        // The turn's teams: this send's Teams pick (JSON null = every team),
        // else the conversation root's.
        val picked = when (val scope = input.scopeTeamIds) {
            null -> root?.scopeTeamIdList
            is kotlinx.serialization.json.JsonArray -> scope.mapNotNull { (it as? kotlinx.serialization.json.JsonPrimitive)?.content }
            else -> null
        }
        return AgentSettings.Assistant.chatTeams(own, synced, picked)
    }

    /** The tree [input] writes into (`staleTree`) and who its key is wrapped for. */
    private suspend fun target(db: ExponentialDatabase, accountId: String, input: SessionsSendInput, fresh: Boolean): Pair<TreeWire, Readers> {
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
        val explicitId = input.sessionId ?: input.parentId
        val conversation = explicitId == null && (input.agentId != null || input.appId != null)
        val existing: SessionEntity? = when {
            explicitId != null -> db.sessionDao().getById(explicitId)
            // #EXP-1990: a team agent's group chat (a first message into one not
            // synced yet opens a fresh tree, as a first message to an agent).
            input.agentId != null -> db.agentChat(input.agentId, userId)
            input.appId != null -> userId?.let { db.sessionDao().appConversation(input.appId, it) }
            else -> null
        }
        val root = existing?.let { row ->
            val rootId = row.rootId.ifEmpty { row.id }
            if (rootId == row.id) row else db.sessionDao().getById(rootId)
        }
        val local = root?.let { row ->
            LocalRoot(row.id, row.sealPublicKey?.let { key -> row.sealEpoch?.let { PublicTreeKey(key, it) } }, clear = row.clearTree)
        }
        val teamId = root?.teamId
            ?: input.teamId
            ?: input.agentId?.let { db.agentDefinitionDao().observeById(it).first()?.teamId }
            ?: input.appId?.let { db.appDao().observeById(it).first()?.teamId }
        val readers = Readers(teamId, root?.deviceId ?: input.deviceId)
        // A child or session Room has, whose root it lacks: cannot be told.
        if (existing != null && root == null && !fresh) return TreeWire.Unknown to readers
        val wire = StaleTree.resolve(
            conversation = conversation,
            explicitId = root?.id ?: existing?.rootId?.ifEmpty { null } ?: explicitId,
            local = local,
            fresh = fresh,
            sendTarget = {
                sessions.get().sendTarget(accountId, input.agentId, input.appId).let { target ->
                    ServerTree(target.rootId, target.publicKey?.let { key -> target.epoch?.let { PublicTreeKey(key, it, target.vouch) } }, clear = target.clearTree)
                }
            },
            publicKey = { publicKey(accountId, it) },
        )
        return wire to readers
    }

    /** `sessionKeys.create` of a fresh key for the keyless [rootId]; null = this install cannot make it. */
    private suspend fun createKey(accountId: String, rootId: String, readers: Readers, ownOnly: Boolean = false): PublicTreeKey? {
        val installId = nodes.get().installId(accountId) ?: return null
        val key = crypto.generateTreeKey()
        // #EXP-1960: a refused wrap: once more for this account's installs only.
        val targets = readerTargets(accountId, installId, if (ownOnly) readers.copy(deviceId = null) else readers) ?: return null
        // #EXP-1950: the root exists: every wrap is signed.
        val wraps = wrapsFor(accountId, targets, key, rootId) ?: return null
        sessionKeys.create(accountId, CreateSessionKeyInput(rootId, installId, FIRST_EPOCH, key.publicKey, wraps))
        keep(accountId, rootId, key.secret)
        // #EXP-1950: it made the key itself: verified.
        nodes.get().recordMadeKey(accountId, rootId, FIRST_EPOCH)
        return PublicTreeKey(key.publicKey, FIRST_EPOCH)
    }

    /** A root whose row has not synced its key yet: ask the server (absent = keyless). */
    private suspend fun publicKey(accountId: String, rootId: String): PublicTreeKey? =
        runCatching { sessionKeys.publicKeys(accountId, listOf(rootId)).firstOrNull { it.rootId == rootId } }
            .getOrNull()?.let { PublicTreeKey(it.publicKey, it.epoch, it.vouch) }

    /** #EXP-1950 R2 + R3 (install-trust.json `sealCases`): [key] when the rule seals [rootId]'s send to it; null = refused (nothing leaves). */
    private fun allowed(accountId: String, rootId: String, key: PublicTreeKey): PublicTreeKey? =
        key.takeIf { nodes.get().sealAllowed(accountId, rootId, it.publicKey, it.epoch, it.vouch) }

    /**
     * The readers this install resolves (addendum 1): its own install
     * always (first), the user's other installs, and the target device's
     * install when `installs.list` names it. Holders fill any gap later via
     * `pending`. Null without an identity.
     */
    private suspend fun readerTargets(accountId: String, installId: String, target: Readers): List<WrapTarget>? {
        val identity = identities.vault.identity(accountId, create = false) ?: return null
        val own = WrapTarget(installId, WrapSignerFfi.endpointIdOf(identity), base64Url(EnetFfi.x25519PublicOf(identity)))
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
        val teamInstalls = target.teamId?.let { teamId -> runCatching { installs.list(accountId, teamId) }.getOrNull() }.orEmpty()
        val others = teamInstalls
            .filter { it.installId != installId }
            .filter { it.userId == userId || (target.deviceId != null && it.deviceId == target.deviceId) }
            .map { WrapTarget(it.installId, it.endpointId, it.x25519Public) to it.userId }
        // #EXP-1950 R4: only the readers the rule wraps for (a skipped one asks a holder later).
        return listOf(own) + nodes.get().allowedTargets(accountId, others)
    }

    /**
     * [key]'s wraps for [targets]; null when the own (first) one cannot be
     * made. #EXP-1950: signed when [rootId] is known; a new root's seal
     * (null) goes unsigned and its holder signs it again ([keepSeal]).
     */
    private suspend fun wrapsFor(accountId: String, targets: List<WrapTarget>, key: TreeKey, rootId: String?): List<TreeKeyWrap>? {
        val signing = rootId?.let { root -> identities.vault.identity(accountId, create = false)?.let { root to it } }
        val wraps = targets.mapNotNull { target ->
            val wrapped = runCatching { crypto.wrapTreeKey(target.x25519Public, key.secret) }.getOrNull() ?: return@mapNotNull null
            val signature = signing?.let { (root, id) ->
                runCatching { WrapSignerFfi.sign(id, root, FIRST_EPOCH, key.publicKey, target.endpointId, wrapped) }.getOrNull()
            }
            TreeKeyWrap(target.installId, wrapped, signature)
        }
        return wraps.takeIf { it.firstOrNull()?.installId == targets.firstOrNull()?.installId }
    }

    /** A new root's key once the send created [rootId]; #EXP-1950: its unsigned seal is signed again by the holder. */
    private fun keepSeal(accountId: String, rootId: String, key: TreeKey, targets: List<WrapTarget>, wraps: List<TreeKeyWrap>) {
        keep(accountId, rootId, key.secret)
        // #EXP-1950 (`newRoot.makerRecord`): it made the key itself: verified.
        nodes.get().recordMadeKey(accountId, rootId, FIRST_EPOCH)
        nodes.get().resignNewRoot(accountId, rootId, FIRST_EPOCH, key.publicKey, targets.filter { target -> wraps.any { it.installId == target.installId } })
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
        /** How often a send waiting for its tree (Room's root) looks again. */
        const val POLL_MS = 500L
        /** How often a send waiting for a keyless tree's key asks again (Room, then the server). */
        const val KEY_POLL_MS = 2_000L
        const val LOCKED_ANSWER = "This card can't be answered on this device yet."
        const val LOCKED_SEND = "This chat can't be written to from this device yet."
    }
}
