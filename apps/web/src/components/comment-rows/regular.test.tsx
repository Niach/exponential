import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Comment } from "@/db/schema"
import { RegularCommentRow } from "@/components/comment-rows/regular"

// SLOP-4: a reporter has no users row, so the avatar colour is keyed on the
// reporter's name. Every comment from one reporter wears the same hue.
function reporterComment(id: string): Comment {
  return {
    id,
    issueId: `issue-1`,
    authorId: null,
    parentId: null,
    body: `hello`,
    source: `reporter`,
    audience: `reporter`,
    createdAt: new Date(`2026-10-01T10:00:00Z`),
    updatedAt: new Date(`2026-10-01T10:00:00Z`),
  } as unknown as Comment
}

function fallbackColour(id: string, reporterName: string | null) {
  const { container, unmount } = render(
    <RegularCommentRow
      author={undefined}
      comment={reporterComment(id)}
      attachments={[]}
      canEdit={false}
      canDelete={false}
      reporterName={reporterName}
      editing={false}
      onDelete={vi.fn()}
      onEdit={vi.fn()}
      onCancelEdit={vi.fn()}
      onSaveEdit={vi.fn()}
      users={[]}
    />
  )
  const fallback = container.querySelector<HTMLElement>(
    `[data-slot="avatar-fallback"]`
  )
  const colour = fallback?.getAttribute(`style`) ?? ``
  unmount()
  return colour
}

describe(`RegularCommentRow reporter avatar`, () => {
  it(`keeps one hue across a reporter's comments`, () => {
    const first = fallbackColour(`c-1`, `Emma Fischer`)
    expect(first).toContain(`--avatar-`)
    for (const id of [`c-2`, `c-3`, `c-4`, `c-5`, `c-6`]) {
      expect(fallbackColour(id, `Emma Fischer`)).toBe(first)
    }
  })
})
