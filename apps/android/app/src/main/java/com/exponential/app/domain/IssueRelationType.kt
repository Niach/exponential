package com.exponential.app.domain

import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.ui.icons.ExpIcons

/**
 * The four issue relation kinds (EXP-736). A stored row is always the
 * CANONICAL direction — `issue_id` blocks / is the parent of / is the
 * duplicate of `related_issue_id`; `related` is symmetric — so each of the two
 * issues renders the SAME row under a different label. [label] picks the side:
 * `inverse = true` is the target issue's reading of it.
 *
 * Entry order and both label lists are locked to
 * DomainContract.issueRelationType* by DomainContractLockTest.
 */
enum class IssueRelationType(
    val wire: String,
    val forwardLabel: String,
    val inverseLabel: String,
) {
    Blocks(DomainContract.issueRelationTypeBlocks, "blocks", "blocked by"),
    Parent(DomainContract.issueRelationTypeParent, "parent of", "sub-issue of"),
    Duplicate(DomainContract.issueRelationTypeDuplicate, "duplicate of", "duplicated by"),
    Related(DomainContract.issueRelationTypeRelated, "related to", "related to");

    fun label(inverse: Boolean): String = if (inverse) inverseLabel else forwardLabel

    companion object {
        /** Null for an unknown value from a newer server (the row degrades to
         *  a plain link rather than mislabeling itself). */
        fun fromWire(value: String?): IssueRelationType? = entries.firstOrNull { it.wire == value }
    }
}

// The `direction` values a relation event payload carries — which SIDE of the
// edge the event was recorded for, hence which label its phrase reads.
const val RELATION_DIRECTION_FORWARD = "forward"
const val RELATION_DIRECTION_INVERSE = "inverse"

/**
 * One entry of the "Add relation" picker: a human title over the (type,
 * inverse) pair the create mutation sends. Ordering and wording are shared by
 * all four clients.
 */
data class RelationPick(
    val title: String,
    val type: IssueRelationType,
    val inverse: Boolean,
) {
    val icon: ImageVector
        get() = when {
            type == IssueRelationType.Parent && !inverse -> ExpIcons.relationParent
            type == IssueRelationType.Parent -> ExpIcons.relationSubIssue
            type == IssueRelationType.Blocks && !inverse -> ExpIcons.relationBlocks
            type == IssueRelationType.Blocks -> ExpIcons.relationBlockedBy
            type == IssueRelationType.Duplicate -> ExpIcons.relationDuplicate
            else -> ExpIcons.relationRelated
        }
}

/** The six picks, in the order every client offers them. */
val relationPicks: List<RelationPick> = listOf(
    RelationPick("Parent of", IssueRelationType.Parent, inverse = false),
    RelationPick("Sub-issue of", IssueRelationType.Parent, inverse = true),
    RelationPick("Blocking", IssueRelationType.Blocks, inverse = false),
    RelationPick("Blocked by", IssueRelationType.Blocks, inverse = true),
    RelationPick("Duplicate of", IssueRelationType.Duplicate, inverse = false),
    RelationPick("Related to", IssueRelationType.Related, inverse = false),
)

/** Where a (type, side) pair sorts in a relation list — the picker's order. */
fun relationSortKey(type: IssueRelationType?, inverse: Boolean): Int {
    val index = relationPicks.indexOfFirst { it.type == type && it.inverse == inverse }
    return if (index >= 0) index else relationPicks.size
}

/** The glyph one SIDE of a relation reads as (the picker's, per side). */
fun relationIcon(type: IssueRelationType?, inverse: Boolean): ImageVector =
    relationPicks.firstOrNull { it.type == type && it.inverse == inverse }?.icon
        ?: ExpIcons.relationRelated

/**
 * The phrase a `relation_added` / `relation_removed` timeline event reads,
 * byte-identical on all four clients: `related` names the act ("added related
 * issue EXP-12"), every other type names the resulting state ("marked as
 * blocked by EXP-3").
 *
 * A thin payload never drops the row — it degrades exactly as the web
 * `relationEventPhrase` does, because an old row or a hard-deleted counterpart
 * must still read as something: an unknown/missing [type] reads as the
 * symmetric `related`, and a missing [identifier] is named "an issue".
 */
fun relationEventPhrase(
    added: Boolean,
    type: String?,
    identifier: String?,
    direction: String?,
): String {
    val named = identifier?.takeIf { it.isNotBlank() } ?: "an issue"
    val relation = IssueRelationType.fromWire(type)
    if (relation == null || relation == IssueRelationType.Related) {
        return if (added) "added related issue $named" else "removed related issue $named"
    }
    val label = relation.label(inverse = direction == RELATION_DIRECTION_INVERSE)
    return if (added) "marked as $label $named" else "no longer $label $named"
}
