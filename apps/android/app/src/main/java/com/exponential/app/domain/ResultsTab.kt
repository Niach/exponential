package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull

/**
 * #FEED-191 THE RESULTS TAB ×4 (`packages/domain-contract/fixtures/results-tab.json`,
 * TS reference `src/results-tab.ts`, iOS ExpCore `ResultsTab.swift`, desktop
 * `domain::results_tab`): a chat tree's results, the session screen's last tab.
 * `ResultsTabTest` replays every fixture case, comparing the WHOLE expected array.
 */
object ResultsTab {

    object Copy {
        const val NOW = "now"
        const val UNTITLED = "Result"
    }

    /** The session screen's Results tab (`tab`). */
    object Tab {
        object Copy {
            const val LABEL = "Results"
            const val EMPTY = "No results yet"
            const val CHANGES = "Changes"
            const val STACK = "Stack"
        }

        object TestIds {
            const val TAB = "thread-results-tab"
            const val LIST = "thread-results"
            const val SECTION = "result-section"
            const val CHANGES = "thread-results-changes"
            const val STACK = "thread-results-stack"
        }

        /** A section's geometry (dp); Android = the `phone` form factor. */
        object Tile {
            const val THUMB_HEIGHT = 240f
            const val THUMB_MAX_WIDTH = 320f
            const val GAP = 8f
            /** Line 1 → the description. */
            const val TEXT_GAP = 4f
            const val RADIUS = 10f
            const val LIST_GAP = 32f
            const val LEAD_GAP = 24f
        }

        /** `tab.specimen.case`: the styleguide specimen draws this case. */
        const val SPECIMEN_CASE = 1
    }

    // ── input ──────────────────────────────────────────────────────────────

    data class SessionInput(
        val id: String,
        val rootId: String,
        /** The session's display name (session-title). */
        val name: String,
    )

    data class MessageInput(
        val id: String,
        val sessionId: String,
        val authorSessionId: String?,
        /** Epoch ms. */
        val createdAt: Long,
        /** The raw `messages.card`. */
        val card: JsonObject?,
    )

    // ── output ─────────────────────────────────────────────────────────────

    data class Picture(
        val label: String,
        val attachmentId: String,
        val width: Double?,
        val height: Double?,
        val caption: String?,
    )

    data class Result(
        val id: String,
        val sessionId: String,
        val sessionName: String,
        val createdAt: Long,
        /** `<m>m` / `<h>h` / `<d>d`, `now` under a minute. */
        val age: String,
        val title: String,
        val description: String?,
        val pictures: List<Picture>,
        val surface: Boolean,
    )

    // ── the fold ───────────────────────────────────────────────────────────

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun JsonObject.num(key: String): Double? =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull

    private fun JsonObject.flag(key: String): Boolean? =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull

    private fun resolvedPictures(card: JsonObject): List<Picture> =
        (card["pictures"] as? JsonArray).orEmpty().mapNotNull { raw ->
            val picture = raw as? JsonObject ?: return@mapNotNull null
            val attachmentId = picture.str("attachmentId")?.takeIf { it.isNotEmpty() } ?: return@mapNotNull null
            Picture(
                label = picture.str("label") ?: "",
                attachmentId = attachmentId,
                width = picture.num("width"),
                height = picture.num("height"),
                caption = picture.str("caption"),
            )
        }

    /** A surface card that goes on the Results tab: `result`, else a card from
     *  before the rename's `story`, else any resolved picture. Plans and
     *  questions never do. */
    fun isResultCard(card: JsonObject?): Boolean {
        if (card == null || card.str("kind") != "surface") return false
        if (!card.str("questionId").isNullOrEmpty()) return false
        return card.flag("result") ?: card.flag("story") ?: resolvedPictures(card).isNotEmpty()
    }

    /** `<m>m` / `<h>h` / `<d>d`, floored; under a minute = [Copy.NOW]. */
    fun age(createdAt: Long, now: Long): String {
        val minutes = (now - createdAt).coerceAtLeast(0) / 60_000
        if (minutes < 1) return Copy.NOW
        if (minutes < 60) return "${minutes}m"
        val hours = minutes / 60
        if (hours < 24) return "${hours}h"
        return "${hours / 24}d"
    }

    /** THE fold: every result the root's tree posted, newest first. */
    fun resultsTab(
        now: Long,
        rootId: String,
        sessions: List<SessionInput>,
        messages: List<MessageInput>,
    ): List<Result> {
        val sessionsById = sessions.associateBy { it.id }
        return messages.mapNotNull { message ->
            if (message.authorSessionId == null || message.authorSessionId != message.sessionId) return@mapNotNull null
            if (!isResultCard(message.card)) return@mapNotNull null
            val card = message.card ?: return@mapNotNull null
            val session = sessionsById[message.sessionId] ?: return@mapNotNull null
            if (session.rootId != rootId) return@mapNotNull null
            Result(
                id = message.id,
                sessionId = session.id,
                sessionName = session.name,
                createdAt = message.createdAt,
                age = age(message.createdAt, now),
                title = card.str("title")?.trim()?.takeIf { it.isNotEmpty() } ?: Copy.UNTITLED,
                description = card.str("description")?.trim()?.takeIf { it.isNotEmpty() },
                pictures = resolvedPictures(card),
                surface = card["surface"] is JsonObject,
            )
        }.sortedWith(compareByDescending<Result> { it.createdAt }.thenByDescending { it.id })
    }
}
