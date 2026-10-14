package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * EXP-1283: THE Guide picture viewer ×4 — the Android mirror of
 * `packages/domain-contract/src/guide-viewer.ts`, byte-locked by
 * `session-results.json` `viewer.cases` (`GuideViewerTest`).
 */
object GuideViewer {

    data class Entry(
        val topic: String? = null,
        val label: String? = null,
        val attachmentId: String? = null,
        val text: String? = null,
    )

    data class State(
        /** 1-based place of the picture among every Guide picture. */
        val index: Int,
        val count: Int,
        val topic: String,
        val label: String,
        /** The picture's topic text (its first non-blank text entry), trimmed. */
        val text: String?,
        val prevId: String?,
        val nextId: String?,
    )

    private fun String?.nonBlank(): String? = this?.takeIf { it.isNotBlank() }

    /**
     * The viewer over [attachmentId]: every Guide picture in the Guide's order
     * (the first Summary topic leads, then topics first-seen, pictures in
     * publish order), no wrap at the ends; null when the attachment is not a
     * Guide picture.
     */
    fun state(guide: List<Entry>?, attachmentId: String): State? {
        val topics = mutableListOf<String>()
        val texts = mutableMapOf<String, String>()
        val pictures = mutableMapOf<String, MutableList<Pair<String, String>>>()
        for (entry in guide.orEmpty()) {
            val topic = entry.topic.nonBlank() ?: continue
            val id = entry.attachmentId.nonBlank()
            val label = entry.label.nonBlank()
            val text = entry.text.nonBlank()
            if (id != null && label != null) {
                if (topic !in topics) topics.add(topic)
                pictures.getOrPut(topic) { mutableListOf() }.add(label to id)
            } else if (text != null) {
                if (topic !in topics) topics.add(topic)
                if (topic !in texts) texts[topic] = text.trim()
            }
        }
        val lead = topics.firstOrNull(::isSummaryTopic)
        val ordered = if (lead != null) listOf(lead) + topics.filter { it != lead } else topics
        val flat = ordered.flatMap { topic -> pictures[topic].orEmpty().map { (label, id) -> Triple(topic, label, id) } }
        val at = flat.indexOfFirst { it.third == attachmentId }
        if (at < 0) return null
        val (topic, label) = flat[at]
        return State(
            index = at + 1,
            count = flat.size,
            topic = topic,
            label = label,
            text = texts[topic],
            prevId = if (at > 0) flat[at - 1].third else null,
            nextId = if (at < flat.size - 1) flat[at + 1].third else null,
        )
    }

    private val json = Json { ignoreUnknownKeys = true }

    private fun JsonObject.string(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    /** The synced `sessions.guide` jsonb → entries (non-objects dropped). */
    fun entries(raw: String?): List<Entry> {
        if (raw.isNullOrBlank()) return emptyList()
        val array = runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonArray ?: return emptyList()
        return array.mapNotNull { element ->
            (element as? JsonObject)?.let {
                Entry(it.string("topic"), it.string("label"), it.string("attachmentId"), it.string("text"))
            }
        }
    }

    fun state(raw: String?, attachmentId: String): State? = state(entries(raw), attachmentId)
}
