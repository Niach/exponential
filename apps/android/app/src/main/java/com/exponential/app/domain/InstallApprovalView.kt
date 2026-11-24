package com.exponential.app.domain

import com.exponential.app.data.sealed.TrustRow
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * #EXP-1950 (fixtures/install-trust.json `copy`, `icon`, and `row` on #3416;
 * decisions a1 + b1): the approval of a new install and a team-mate's reset,
 * and this install waiting itself, as the genUI trees web builds
 * (`apps/web/src/components/devices/install-approval.tsx`) and iOS
 * (`InstallApprovalView.swift`): the same node ids, props and order, painted
 * by the core catalog through [com.exponential.app.ui.ci.CiSurface]: Section,
 * Stack, Row, Badge, Button, EmptyState. No new component, no new text
 * style: the code is the Row's subtitle / the EmptyState's description. Rows:
 * Needs you, and the phone's Devices too. The code is derived on THIS install
 * from the row's endpoint id ([com.exponential.app.data.sealed.InstallCode]),
 * never read from the server.
 */
object InstallApprovalView {
    /** The fixture's `copy`, the same ×4. */
    object Copy {
        const val NEW_INSTALL = "New install"
        const val RESET = "Reset"
        const val APPROVE = "Approve"
        const val REMOVE = "Remove"
        const val WAITING = "Waiting for approval"

        fun badge(kind: TrustRow.Kind): String = when (kind) {
            TrustRow.Kind.NewInstall -> NEW_INSTALL
            TrustRow.Kind.Reset -> RESET
        }
    }

    /** The fixture's `icon` (the waiting EmptyState's glyph). */
    const val ICON = "ui-private"

    /** The fixture's `waiting`: where the waiting state stands (a share of the
     *  window's height), its column, the gap above the Needs you rows. */
    object Waiting {
        const val TOP_OFFSET = 0.2f
        const val MAX_WIDTH = 448
        const val LIST_GAP = 12
    }

    /** Events the rows fire (context `id` = the row's install id). */
    object Events {
        const val APPROVE = "approve"
        const val REMOVE = "remove"
    }

    object TestIds {
        const val PENDING = "pending-installs"
        const val WAITING = "install-waiting"
    }

    /** A row's title: the install's label; a team-mate's reset leads with
     *  their name. Nothing = the code. */
    fun title(row: TrustRow, memberName: String?): String {
        val label = row.label.orEmpty()
        if (row.kind != TrustRow.Kind.Reset) return label.ifEmpty { row.code }
        return listOf(memberName.orEmpty(), label).filter { it.isNotEmpty() }.joinToString(" · ").ifEmpty { row.code }
    }

    private fun node(
        id: String,
        component: String,
        props: Map<String, String>,
        children: List<JsonElement>? = null,
        extra: Map<String, JsonElement> = emptyMap(),
    ): JsonObject {
        val out = linkedMapOf<String, JsonElement>(
            "id" to JsonPrimitive(id),
            "component" to JsonPrimitive(component),
            "props" to JsonObject(props.mapValues { JsonPrimitive(it.value) }),
        )
        out.putAll(extra)
        if (children != null) out["children"] = JsonArray(children)
        return JsonObject(out)
    }

    private fun press(name: String, installId: String): Map<String, JsonElement> = mapOf(
        "on" to JsonObject(
            mapOf(
                "press" to JsonObject(
                    mapOf("event" to JsonObject(mapOf("name" to JsonPrimitive(name), "context" to JsonObject(mapOf("id" to JsonPrimitive(installId)))))),
                ),
            ),
        ),
    )

    /** One Stack per install (vertical, gap sm; fixture `row`): the Row
     *  (title, the code as its subtitle, trailing ONLY the Badge), then the
     *  actions under it, Approve (Button sm) and Remove (Button sm ghost). The
     *  buttons never sit in the Row's trailing slot, so the code the person
     *  must compare shows whole down to 320 pt; only the title may truncate. */
    fun row(row: TrustRow, title: String): JsonObject {
        val id = "install-${row.installId}"
        val badge = node("$id-badge", "Badge", mapOf("text" to Copy.badge(row.kind), "variant" to "secondary"))
        val line = node("$id-row", "Row", mapOf("title" to title, "subtitle" to row.code), extra = mapOf("slots" to JsonObject(mapOf("trailing" to badge))))
        val actions = node(
            "$id-actions",
            "Stack",
            mapOf("direction" to "horizontal", "gap" to "sm"),
            children = listOf(
                node("$id-approve", "Button", mapOf("label" to Copy.APPROVE, "size" to "sm"), extra = press(Events.APPROVE, row.installId)),
                node("$id-remove", "Button", mapOf("label" to Copy.REMOVE, "size" to "sm", "variant" to "ghost"), extra = press(Events.REMOVE, row.installId)),
            ),
        )
        return node(id, "Stack", mapOf("direction" to "vertical", "gap" to "sm"), children = listOf(line, actions))
    }

    /** The band of installs waiting for this install's approval. */
    fun pendingTree(rows: List<Pair<TrustRow, String>>): JsonObject =
        node(TestIds.PENDING, "Section", emptyMap(), children = rows.map { (row, title) -> row(row, title) })

    /** This install, not approved yet: THE page EmptyState with its own code. */
    fun waitingTree(code: String): JsonObject =
        node(TestIds.WAITING, "EmptyState", mapOf("icon" to ICON, "title" to Copy.WAITING, "description" to code))

    /** The capture specimens' rows (`install-approval`, web `?specimen=`):
     *  install-trust.json `keys.N` (a New install) and `keys.T3` (a
     *  team-mate's Reset); InstallTrustFixtureTest holds them to the file. */
    val SPECIMEN_ROWS = listOf(
        TrustRow(
            kind = TrustRow.Kind.NewInstall,
            installId = "install-N",
            endpointId = "3f7708d5f5cc2bc633b59d2b3a2ed92e7479220c6f08ade208bebcd8580ab93b",
            userId = "user-me",
            label = "Pixel 9",
            code = "HNMBN A0319",
        ),
        TrustRow(
            kind = TrustRow.Kind.Reset,
            installId = "install-T3",
            endpointId = "74fca2a3b389fb1a64d9bf52cc0dd4c2964f3804c0cf7c755e8513c6db8198dc",
            userId = "user-mate",
            label = "Mira's new iPhone",
            code = "776AF T3NNF",
        ),
    )
}
