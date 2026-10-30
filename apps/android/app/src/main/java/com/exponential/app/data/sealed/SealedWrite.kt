package com.exponential.app.data.sealed

import com.exponential.app.data.api.AnswerInput
import com.exponential.app.domain.isInlineImage
import com.exponential.app.ui.markdown.IssueRefs
import com.exponential.app.ui.markdown.Mentions
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * EXP-1459: the pure parts of writing sealed content (contract addendum 2 on
 * #EXP-1458): what the server can no longer do to a sealed body or card, the
 * writer does before sealing.
 */
object SealedWrite {
    /** The `@email` tokens of [text], lowercased, once each (`mentionUserIds` resolves them). */
    fun mentionEmails(text: String): List<String> = Mentions.findAll(text).map { it.email.lowercase() }.distinct()

    /** The `#IDENT` issue refs of [text], uppercased, once each (`refIssueIds` resolves them). */
    fun issueRefs(text: String): List<String> = IssueRefs.findAll(text).map { it.identifier.uppercase() }.distinct()

    /** A session upload this process made: what its embed line needs. */
    data class Upload(val filename: String, val contentType: String)

    private val uploads = java.util.concurrent.ConcurrentHashMap<String, Upload>()

    /** Recorded by the session upload routes (IssueImagesApi) for [withAttachmentEmbeds]. */
    fun rememberUpload(id: String, filename: String, contentType: String) {
        uploads[id] = Upload(filename, contentType)
    }

    fun upload(id: String): Upload? = uploads[id]

    /**
     * [text] with an embed line per attachment it does not reference yet
     * (the server's claimAttachments rule: `![name](url)` for an inline image,
     * `[name](url)` otherwise): the server never appends to a sealed body. An
     * upload this process did not record embeds as an image named "image".
     */
    fun withAttachmentEmbeds(text: String, attachmentIds: List<String>, uploadOf: (String) -> Upload? = ::upload): String {
        val lines = attachmentIds.distinct()
            .filter { id -> !text.contains(attachmentUrl(id)) }
            .map { id ->
                val upload = uploadOf(id)
                val name = (upload?.filename ?: "image").replace(Regex("[\\[\\]]"), "")
                val image = upload == null || isInlineImage(upload.contentType)
                "${if (image) "!" else ""}[$name](${attachmentUrl(id)})"
            }
        if (lines.isEmpty()) return text
        return (listOf(text.trimEnd()) + lines).filter { it.isNotEmpty() }.joinToString("\n\n")
    }

    fun attachmentUrl(id: String) = "/api/attachments/$id"

    /**
     * The FULL card with its answer in it (the server's `answeredCard`,
     * lib/sessions/card-answer.ts): a question gets `answer` + `answers`, a
     * plan `decision` + `fresh` + `feedback` (+ `answers`); either keeps the
     * form's `surfaceData`. Null when [card] asks nothing.
     */
    fun answeredCard(card: JsonObject, input: AnswerInput): JsonObject? {
        val kind = (card["kind"] as? JsonPrimitive)?.content
        val out = card.toMutableMap()
        input.surfaceData?.let { out["surfaceData"] = it }
        when (kind) {
            "question" -> {
                val labels = (input.answers as? JsonArray).orEmpty()
                    .mapNotNull { (it as? JsonPrimitive)?.content?.trim() }.filter { it.isNotEmpty() }
                val answers = labels.ifEmpty { listOfNotNull(input.answer?.trim()) }
                if (answers.isEmpty()) return null
                out["answer"] = JsonPrimitive(if (labels.isNotEmpty()) labels.joinToString(", ") else answers[0])
                out["answers"] = JsonArray(answers.map(::JsonPrimitive))
            }
            "plan" -> {
                val decision = input.decision ?: return null
                out["decision"] = JsonPrimitive(decision)
                out["fresh"] = JsonPrimitive(decision == "approved" && input.fresh == true)
                out["feedback"] = input.feedback?.trim()?.takeIf { it.isNotEmpty() }?.let(::JsonPrimitive) ?: JsonNull
                (input.answers as? JsonObject)?.let { picks ->
                    val kept = picks.mapNotNull { (id, labels) ->
                        val picked = (labels as? JsonArray).orEmpty()
                            .mapNotNull { (it as? JsonPrimitive)?.content?.trim() }.filter { it.isNotEmpty() }
                        if (picked.isEmpty()) null else id to (JsonArray(picked.map(::JsonPrimitive)) as JsonElement)
                    }.toMap()
                    out["answers"] = JsonObject(kept)
                }
            }
            else -> return null
        }
        return JsonObject(out)
    }
}
