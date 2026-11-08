import type { Comment } from "@/db/schema"

/**
 * EXP-741: one issue's comments nested one level — the top-level cards in
 * their input order, and each card's replies (also in input order) keyed by
 * the parent id. Replies are ONE level deep by construction (`comments.create`
 * re-parents a reply-to-a-reply onto the root), so a row is a reply exactly
 * when `parentId` is set.
 *
 * A reply whose parent is NOT in the list (still syncing, or the parent row
 * vanished from a partial snapshot) surfaces as a top-level card rather than
 * disappearing — the row is still real activity. Mirrored by the desktop
 * `nest_comments`, iOS `nestComments` and Android `nestComments`.
 */
export interface CommentReplies<C> {
  topLevel: C[]
  repliesByParent: Map<string, C[]>
}

export function nestComments<C extends Pick<Comment, `id` | `parentId`>>(
  comments: readonly C[]
): CommentReplies<C> {
  const ids = new Set(comments.map((comment) => comment.id))
  const topLevel: C[] = []
  const repliesByParent = new Map<string, C[]>()
  for (const comment of comments) {
    const parentId = comment.parentId
    if (parentId && ids.has(parentId) && parentId !== comment.id) {
      const list = repliesByParent.get(parentId) ?? []
      list.push(comment)
      repliesByParent.set(parentId, list)
    } else {
      topLevel.push(comment)
    }
  }
  return { topLevel, repliesByParent }
}

/** How many rows a nested list holds in total — the "Activity (N)" count. */
export function countNestedComments<C>(nested: CommentReplies<C>): number {
  let replies = 0
  for (const list of nested.repliesByParent.values()) replies += list.length
  return nested.topLevel.length + replies
}
