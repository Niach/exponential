package com.exponential.app.domain

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlin.math.roundToInt

/**
 * EXP-920: one entity an Exponential MCP tool's answer named — the wire shape
 * of `tool_update.preview.refs[]`, every string already clamped by the
 * publisher to `expToolPreview.textMax` (and clamped again here, since a
 * NEWER publisher may not). The Kotlin twin of web `EntityRef` / iOS
 * `EntityRef` / desktop `preview::EntityRef`.
 *
 * Every default is explicit: this is a FEED struct parsed by [parse], never an
 * Electric entity, so the all-defaults partial-update trap does not apply —
 * but the shape stays honest about which fields the wire may omit.
 */
@Serializable
data class EntityRef(
    val kind: String,
    val id: String,
    val identifier: String? = null,
    val title: String? = null,
    val count: Int? = null,
) {
    companion object {
        /**
         * Web `parseEntityRef`: the kind must be a contract `entityRefKind`
         * (unknown kinds DROP — a chip with no rule is worse than no chip),
         * the id trimmed non-empty, `identifier`/`title` trimmed and clamped
         * to [DomainContract.expToolPreviewTextMax] (blank = absent), `count`
         * finite, non-negative and rounded. Anything else is null.
         */
        fun parse(raw: JsonElement?): EntityRef? {
            val obj = raw as? JsonObject ?: return null
            val kind = obj.string("kind") ?: return null
            if (kind !in DomainContract.entityRefKindValues) return null
            val id = obj.string("id")?.trim()?.takeIf { it.isNotEmpty() } ?: return null
            val count = (obj["count"] as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull
                ?.takeIf { it.isFinite() && it >= 0 }
                ?.roundToInt()
            return EntityRef(
                kind = kind,
                id = id.take(DomainContract.expToolPreviewTextMax),
                identifier = obj.string("identifier")?.trim()?.takeIf { it.isNotEmpty() }
                    ?.take(DomainContract.expToolPreviewTextMax),
                title = obj.string("title")?.trim()?.takeIf { it.isNotEmpty() }
                    ?.take(DomainContract.expToolPreviewTextMax),
                count = count,
            )
        }

        private fun JsonObject.string(key: String): String? =
            (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
    }

    /** What this ref adds to a tool row's byte estimate. */
    fun weight(): Int = kind.length + id.length + (identifier?.length ?: 0) + (title?.length ?: 0)
}

/**
 * EXP-920: the ONE entity-preview rule — the Kotlin mirror of
 * `@exp/domain-contract` `entity-preview.ts` (read that file: each function's
 * doc comment IS the rule), byte-locked ×4 by `fixtures/entity-chip.json`
 * (`EntityPreviewTest`). Every client renders a settled Exponential tool's
 * `preview.refs` the SAME way: one CHIP per group (a glyph + a short label),
 * a preview sheet resolved from the client's own synced rows, and an Open
 * that lands on the entity's detail screen.
 */
object EntityPreview {

    /** A chip's label never runs past this many code points; a longer one is
     *  cut to `CHIP_LABEL_MAX - 1` and ends in an ellipsis. */
    const val CHIP_LABEL_MAX = 48

    /** The icon CONCEPT a kind's chip and card header draw (`packages/icons`
     *  `semantic`) — the TS `ENTITY_REF_ICON` table verbatim. A `list` chip
     *  draws its MEMBER kind's icon. */
    val ICON: Map<String, String> = mapOf(
        "issue" to "ui-issue",
        "board" to "nav-boards",
        "action" to "nav-actions",
        "automation" to "nav-automations",
        "comment" to "notification-issue-comment",
        "session" to "coding-running",
        "label" to "settings-labels",
        "status" to "settings-statuses",
        "workflow" to "nav-workflows",
        "device" to "ui-device",
        "member" to "ui-avatar-placeholder",
        "repository" to "ui-repository",
        "team" to "ui-team",
        "invite" to "ui-invite",
        "notification" to "nav-notifications",
        "thread" to "nav-support",
        "attachment" to "ui-attach",
        "list" to "ui-checklist",
    )

    private val NOUNS: Map<String, Pair<String, String>> = mapOf(
        "issue" to ("issue" to "issues"),
        "board" to ("board" to "boards"),
        "action" to ("action" to "actions"),
        "automation" to ("automation" to "automations"),
        "comment" to ("comment" to "comments"),
        "session" to ("run" to "runs"),
        "label" to ("label" to "labels"),
        "status" to ("status" to "statuses"),
        "workflow" to ("workflow" to "workflows"),
        "device" to ("device" to "devices"),
        "member" to ("member" to "members"),
        "repository" to ("repository" to "repositories"),
        "team" to ("team" to "teams"),
        "invite" to ("invite" to "invites"),
        "notification" to ("notification" to "notifications"),
        "thread" to ("thread" to "threads"),
        "attachment" to ("attachment" to "attachments"),
        "list" to ("list" to "lists"),
    )

    /** The product noun for a kind: singular for [count] 1, plural otherwise.
     *  An unknown kind (a NEWER publisher) reads as `item`/`items`. */
    fun kindNoun(kind: String, count: Int = 1): String {
        val pair = NOUNS[kind] ?: ("item" to "items")
        return if (count == 1) pair.first else pair.second
    }

    /** The icon concept a ref draws: a `list` ref its member kind's, anything
     *  else its own; an unknown kind falls back to the `list` glyph. */
    fun refIcon(ref: EntityRef): String {
        val kind = if (ref.kind == "list") ref.id else ref.kind
        return ICON[kind] ?: ICON.getValue("list")
    }

    /** Cut to [CHIP_LABEL_MAX] code points, ellipsis included in the budget;
     *  the kept prefix loses its trailing whitespace so no label ends in ` …`. */
    fun clampChipLabel(text: String): String {
        val trimmed = text.trim()
        val points = trimmed.codePointCount(0, trimmed.length)
        if (points <= CHIP_LABEL_MAX) return trimmed
        val cut = trimmed.offsetByCodePoints(0, CHIP_LABEL_MAX - 1)
        return trimmed.substring(0, cut).trimEnd() + "…"
    }

    /** The chip's text:
     *  - a `list`: `<count> <member noun>` ("3 issues", "1 run", "0 labels");
     *  - an issue: its identifier, else its title, else "Issue";
     *  - anything else: its title, else the capitalized noun ("Board", "Run").
     *  Whitespace-only titles count as absent. */
    fun chipLabel(ref: EntityRef): String {
        if (ref.kind == "list") {
            val count = maxOf(0, ref.count ?: 0)
            return clampChipLabel("$count ${kindNoun(ref.id, count)}")
        }
        val identifier = ref.identifier?.trim()?.takeIf { it.isNotEmpty() }
        val title = ref.title?.trim()?.takeIf { it.isNotEmpty() }
        if (ref.kind == "issue" && identifier != null) return clampChipLabel(identifier)
        if (title != null) return clampChipLabel(title)
        return capitalize(kindNoun(ref.kind, 1))
    }

    /** The secondary text an issue chip shows beside its identifier (the
     *  title), null for every other kind or when the identifier already IS
     *  the label. */
    fun chipDetail(ref: EntityRef): String? {
        if (ref.kind != "issue") return null
        val identifier = ref.identifier?.trim()?.takeIf { it.isNotEmpty() }
        val title = ref.title?.trim()?.takeIf { it.isNotEmpty() }
        return if (identifier != null && title != null) clampChipLabel(title) else null
    }

    /** One chip's worth of refs. */
    data class Group(
        val ref: EntityRef,
        /** The member refs a `list` chip's card lists; empty on every other chip. */
        val members: List<EntityRef>,
    )

    /** One chip per group: a `list` ref opens a group and absorbs every
     *  DIRECTLY following ref of its member kind; the first ref of another
     *  kind (or another list) closes it. Any other ref is a group of its own. */
    fun groupRefs(refs: List<EntityRef>): List<Group> {
        val groups = mutableListOf<Group>()
        var openIndex = -1
        for (ref in refs) {
            if (openIndex >= 0 && ref.kind != "list" && ref.kind == groups[openIndex].ref.id) {
                val open = groups[openIndex]
                groups[openIndex] = open.copy(members = open.members + ref)
                continue
            }
            groups += Group(ref, emptyList())
            openIndex = if (ref.kind == "list") groups.lastIndex else -1
        }
        return groups
    }

    private fun capitalize(text: String): String =
        if (text.isEmpty()) text else text.substring(0, 1).uppercase() + text.substring(1)
}
