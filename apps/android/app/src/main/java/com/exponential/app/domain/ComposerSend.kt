package com.exponential.app.domain

/**
 * EXP-1537: what a send leaves in THE composer. Pure and locked by
 * `fixtures/composer-send.json` (`cases`, the `$comment` holds the rules);
 * mirrors the TS reference `composerAfterSend` (src/composer-send.ts),
 * desktop `domain::composer_send` and iOS `ComposerSend.swift`.
 *
 * The field stays editable while a send is in flight: an accepted send clears
 * the text only if the field still equals what was sent byte for byte, and
 * removes exactly the attachments that went out; a failed one leaves the
 * field as it is now.
 */
object ComposerSend {

    enum class Outcome { Sent, Failed }

    /** A composer's content: its text and its attachments' keys ([PendingAttachment.key]). */
    data class Content(val text: String, val attachments: List<String>)

    fun afterSend(outcome: Outcome, sent: Content, current: Content): Content {
        if (outcome == Outcome.Failed) return current.copy(attachments = current.attachments.toList())
        val gone = sent.attachments.toSet()
        return Content(
            text = textAfterSend(sent.text, current.text),
            attachments = current.attachments.filter { it !in gone },
        )
    }

    /** The text half alone (a composer without attachments). */
    fun textAfterSend(sent: String, current: String): String = if (current == sent) "" else current

    /** [afterSend] over a live composer's pending files. */
    data class Settled(val text: String, val attachments: List<PendingAttachment>)

    /**
     * [afterSend] applied to a live composer: [sentText] / [sent] = the field
     * and its files when Send was pressed, [currentText] / [current] = now.
     */
    fun settle(
        outcome: Outcome,
        sentText: String,
        sent: List<PendingAttachment>,
        currentText: String,
        current: List<PendingAttachment>,
    ): Settled {
        val after = afterSend(
            outcome,
            Content(sentText, sent.map { it.key }),
            Content(currentText, current.map { it.key }),
        )
        val keep = after.attachments.toSet()
        return Settled(after.text, current.filter { it.key in keep })
    }
}
