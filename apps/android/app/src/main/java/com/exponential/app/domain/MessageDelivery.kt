package com.exponential.app.domain

// #FEED-237 (Danny: "like whatsapp"): a person's OWN message is one bubble
// at the bottom of the transcript from the press, its delivery mark (the
// clock, one check, two checks, the red mark) and its menu. Hand-ported
// from `packages/domain-contract/src/message-delivery.ts`, locked to
// `fixtures/message-delivery.json` (its `$comment` holds the rules) by
// MessageDeliveryTest, which replays `stateCases`, `outboxCases` and
// `actionCases`.

object MessageDelivery {
    object Copy {
        object Status {
            const val SENDING = "Sending"
            const val SENT = "Sent"
            const val READ = "Read"
            const val FAILED = "Not sent"
        }

        object Menu {
            const val RETRY = "Retry"
            const val DELETE = "Delete"
            const val CANCEL = "Cancel sending"
        }

        /** `sessions.unqueue` refused (the row was consumed meanwhile). */
        const val CANCEL_FAILED = "Couldn't remove the message"
    }

    object TestIds {
        const val STATUS = "message-status"
        const val MENU = "message-menu"
    }

    const val MARK_SIZE = 14
    const val MATCH_WINDOW_MS = 60_000L

    enum class Tone(val wire: String) { Muted("muted"), Info("info"), Danger("danger") }

    /** genUI's `messageStatus`. */
    enum class State(val wire: String, val icon: String, val tone: Tone, val label: String) {
        Sending("sending", "ui-clock", Tone.Muted, Copy.Status.SENDING),
        Sent("sent", "ui-check", Tone.Muted, Copy.Status.SENT),
        Read("read", "ui-read", Tone.Info, Copy.Status.READ),
        Failed("failed", "ui-error", Tone.Danger, Copy.Status.FAILED),
        ;

        companion object {
            fun of(wire: String): State = entries.first { it.wire == wire }
        }
    }

    /** An outbox entry's own state (`sending` | `failed`). */
    enum class Local(val wire: String) {
        Sending("sending"), Failed("failed");

        companion object {
            fun of(wire: String?): Local? = entries.firstOrNull { it.wire == wire }
        }
    }

    enum class Action(val wire: String, val label: String) {
        Retry("retry", Copy.Menu.RETRY),
        Delete("delete", Copy.Menu.DELETE),
        Cancel("cancel", Copy.Menu.CANCEL),
    }

    /**
     * [local] = the outbox entry's state (null = a synced row alone);
     * [stored] = the server holds the row; [echoed] = the bubble IS the live
     * transcript's echo of the turn. First match wins.
     */
    fun deliveryState(
        local: Local?,
        stored: Boolean,
        deliveredAt: String?,
        consumedAt: String?,
        echoed: Boolean,
    ): State = when {
        echoed || !consumedAt.isNullOrEmpty() -> State.Read
        !deliveredAt.isNullOrEmpty() -> State.Sent
        local == Local.Failed && !stored -> State.Failed
        else -> State.Sending
    }

    data class Entry(
        /** A fresh UUID per press; also the `sessions.send` `clientMessageId`. */
        val clientId: String,
        val text: String,
        val createdAt: String,
        val state: Local,
        /** The row the server stored it as (null until the send answers). */
        val messageId: String?,
    )

    data class Row(
        val id: String,
        val authorKind: String,
        val authorId: String?,
        val body: String,
        val createdAt: String,
        val deliveredAt: String? = null,
        val consumedAt: String? = null,
    )

    /** One of the viewer's own bubbles at the transcript's bottom: a synced
     *  row the device has not consumed ([messageId]) or a kept outbox entry
     *  ([clientId]). [createdAt] = the `HH:mm`'s wire. */
    data class Bubble(
        val key: String,
        val text: String,
        val createdAt: String,
        val state: State,
        val stored: Boolean,
        val clientId: String?,
        val messageId: String?,
    ) {
        val actions: List<Action> get() = bubbleActions(state, stored)
    }

    /** [bubbles] in ORDER (the unconsumed own rows oldest first, then the
     *  kept entries); [drop] = the entries their rows replaced. */
    data class Tail(val bubbles: List<Bubble>, val drop: List<String>)

    /** The viewer's own tail of a session ([rows] = their synced rows). */
    fun tail(viewerId: String?, outbox: List<Entry>, rows: List<Row>): Tail {
        val merge = outboxMerge(viewerId, outbox, rows)
        val byClient = outbox.associateBy { it.clientId }
        val localByRow = outbox.mapNotNull { e -> e.messageId?.let { it to e.state } }.toMap()
        val pending = rows
            .filter { it.authorKind == DomainContract.messageAuthorKindUser && viewerId != null && it.authorId == viewerId }
            .filter { it.consumedAt.isNullOrEmpty() }
            .sortedWith(compareBy<Row>({ ms(it.createdAt) }, { it.id }))
            .map { row ->
                Bubble(
                    key = "own-row-${row.id}",
                    text = row.body,
                    createdAt = row.createdAt,
                    state = deliveryState(localByRow[row.id], stored = true, row.deliveredAt, null, echoed = false),
                    stored = true,
                    clientId = null,
                    messageId = row.id,
                )
            }
        val kept = merge.keep.mapNotNull(byClient::get).map { entry ->
            val stored = !entry.messageId.isNullOrEmpty()
            Bubble(
                key = "own-entry-${entry.clientId}",
                text = entry.text,
                createdAt = entry.createdAt,
                state = deliveryState(entry.state, stored, null, null, echoed = false),
                stored = stored,
                clientId = entry.clientId,
                messageId = entry.messageId,
            )
        }
        return Tail(pending + kept, merge.drop)
    }

    data class Merge(val drop: List<String>, val keep: List<String>)

    private fun ms(wire: String): Long = WireTimestamps.parseEpochMs(wire) ?: 0L

    private val byAge = compareBy<Entry>({ ms(it.createdAt) }, { it.clientId })

    /** Which outbox entries a synced row replaced ([Merge.drop]) and which
     *  still draw ([Merge.keep], oldest first). */
    fun outboxMerge(viewerId: String?, outbox: List<Entry>, rows: List<Row>): Merge {
        val entries = outbox.sortedWith(byAge)
        val rowIds = rows.mapTo(HashSet()) { it.id }
        val matched = HashSet<String>()
        val dropped = HashSet<String>()
        // By id first: an acked entry's row is its row, whatever its text.
        for (entry in entries) {
            val id = entry.messageId
            if (!id.isNullOrEmpty() && id in rowIds) {
                dropped += entry.clientId
                matched += id
            }
        }
        val own = rows
            .filter { it.authorKind == DomainContract.messageAuthorKindUser && viewerId != null && it.authorId == viewerId }
            .sortedWith(compareBy<Row>({ ms(it.createdAt) }, { it.id }))
        for (entry in entries) {
            if (entry.clientId in dropped || !entry.messageId.isNullOrEmpty() || entry.state == Local.Failed) continue
            val from = ms(entry.createdAt) - MATCH_WINDOW_MS
            val row = own.firstOrNull { it.id !in matched && it.body == entry.text && ms(it.createdAt) >= from } ?: continue
            matched += row.id
            dropped += entry.clientId
        }
        return Merge(
            drop = entries.filter { it.clientId in dropped }.map { it.clientId },
            keep = entries.filter { it.clientId !in dropped }.map { it.clientId },
        )
    }

    /** The bubble's menu, in order (empty = no menu). */
    fun bubbleActions(state: State, stored: Boolean): List<Action> = when (state) {
        State.Failed -> listOf(Action.Retry, Action.Delete)
        State.Read -> emptyList()
        else -> if (stored) listOf(Action.Cancel) else emptyList()
    }
}
