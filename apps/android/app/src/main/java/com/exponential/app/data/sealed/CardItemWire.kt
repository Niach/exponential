package com.exponential.app.data.sealed

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * #EXP-1958 (sealed-content.json `cardStamps`): a tap on a `replies` /
 * `suggestions` card. The Kotlin twin of the TS reference `cardItemWire`
 * (packages/domain-contract/src/sealed-content.ts), replayed by
 * `tapCases`: decided by the STORED card (sealed or not) and whether this
 * install opened it.
 */
enum class CardItemAction(val wire: String) {
    Reply("reply"),
    Here("here"),
    Start("start"),
    StartAll("startAll"),
    Dismiss("dismiss"),
    DismissReplies("dismissReplies"),
    File("file"),
}

enum class CardTapWire(val wire: String) {
    /** An unsealed card: today's server actions (the server composes the text). */
    Server("server"),
    /** `sessions.send` through the sealing writer, `cardItem` beside it. */
    Sealed("sealed"),
    /** The server only stamps the item. */
    Stamp("stamp"),
    /** A sealed card this install cannot open: nothing leaves. */
    Refused("refused"),
}

data class CardTapRoute(val wire: CardTapWire, val call: String?)

object CardItemWire {
    private val serverCalls = mapOf(
        CardItemAction.Reply to "sessions.sendReply",
        CardItemAction.Here to "sessions.startSuggestion",
        CardItemAction.Start to "sessions.startSuggestion",
        CardItemAction.StartAll to "sessions.startAllSuggestions",
        CardItemAction.Dismiss to "sessions.dismissSuggestion",
        CardItemAction.DismissReplies to "sessions.dismissReplies",
        CardItemAction.File to "sessions.fileSuggestion",
    )

    fun route(sealed: Boolean, opened: Boolean, action: CardItemAction): CardTapRoute = when {
        !sealed -> CardTapRoute(CardTapWire.Server, serverCalls.getValue(action))
        !opened -> CardTapRoute(CardTapWire.Refused, null)
        action == CardItemAction.Reply || action == CardItemAction.Here ||
            action == CardItemAction.Start || action == CardItemAction.StartAll ->
            CardTapRoute(CardTapWire.Sealed, "sessions.send")
        else -> CardTapRoute(CardTapWire.Stamp, serverCalls.getValue(action))
    }

    /**
     * `cardStamps` NEVER IN CLEAR: a tap's prepared wire is sealed: [text] an
     * `exp-sealed:` envelope, no clear [name] (a Start's title leaves only
     * as [sealedName], itself an envelope).
     */
    fun leavesSealed(text: String?, name: String?, sealedName: String?): Boolean =
        text?.startsWith(SealedContent.BODY_PREFIX) == true && name == null &&
            (sealedName == null || sealedName.startsWith(SealedContent.BODY_PREFIX))

    /** The request a started / here item carries: its title, a blank line, its text. */
    fun request(title: String, text: String): String = "$title\n\n$text"

    /** One opened item of a card, by id (null = none). */
    fun item(card: JsonObject, itemId: String): JsonObject? =
        (card["items"] as? JsonArray)?.firstOrNull { (it as? JsonObject)?.str("id") == itemId } as? JsonObject

    /** A replies item still open (not sent). */
    fun replyOpen(item: JsonObject): Boolean = !item.isTrue("sent")

    /** A suggestion still open (`suggestionState` = open: not started, here, filed or dismissed). */
    fun suggestionOpen(item: JsonObject): Boolean =
        item.str("sessionId") == null && !item.isTrue("here") && item.str("issueId") == null && !item.isTrue("dismissed")

    /** The card's open item ids, in card order. */
    fun openIds(card: JsonObject, open: (JsonObject) -> Boolean): List<String> =
        (card["items"] as? JsonArray).orEmpty().mapNotNull { element ->
            val raw = element as? JsonObject ?: return@mapNotNull null
            raw.str("id")?.takeIf { open(raw) }
        }

    /** An item's `issueIds` (strings only). */
    fun issueIds(item: JsonObject): List<String> =
        (item["issueIds"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }

    private val uuid = Regex("^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")

    /** Start's `issueIds`: uuids as they are, an `#IDENT` [resolve] names, the rest dropped. */
    fun startIssueIds(ids: List<String>, resolve: (List<String>) -> Map<String, String>): List<String> {
        val idents = ids.filterNot(uuid::matches).map { it.removePrefix("#") }
        val resolved = if (idents.isEmpty()) emptyMap() else resolve(idents)
        return ids.mapNotNull { id -> if (uuid.matches(id)) id else resolved[id.removePrefix("#")] }.distinct()
    }

    fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    /** JS `String(x ?? "")` for a card field. */
    fun JsonObject.text(key: String): String = when (val value = this[key]) {
        null, is kotlinx.serialization.json.JsonNull -> ""
        is JsonPrimitive -> value.content
        else -> value.toString()
    }

    private fun JsonObject.isTrue(key: String): Boolean =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true"
}
