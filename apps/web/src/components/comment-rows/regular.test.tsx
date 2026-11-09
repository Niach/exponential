import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { avatarHueIndex } from "@exponential_app/eui-react/primitives"
import type { Comment } from "@/db/schema"
import { RegularCommentRow } from "@/components/comment-rows/regular"

// SLOP-4: a reporter has no users row, so the avatar colour is keyed on the
// reporter's name. Every comment from one reporter wears the same hue.
// #EXP-1754 (fixtures/avatar.json `keys`): the name, else the email, with no
// prefix; an anonymous reporter has no key: the muted disc.
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

function fallbackOf(id: string, reporterName: string | null, reporterEmail: string | null = null) {
  const { container, unmount } = render(
    <RegularCommentRow
      author={undefined}
      comment={reporterComment(id)}
      attachments={[]}
      canEdit={false}
      canDelete={false}
      reporterName={reporterName}
      reporterEmail={reporterEmail}
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
  const result = {
    style: fallback?.getAttribute(`style`) ?? ``,
    className: fallback?.className ?? ``,
    text: fallback?.textContent ?? ``,
  }
  unmount()
  return result
}

function fallbackColour(id: string, reporterName: string | null) {
  return fallbackOf(id, reporterName).style
}

describe(`RegularCommentRow reporter avatar`, () => {
  it(`keeps one hue across a reporter's comments`, () => {
    const first = fallbackColour(`c-1`, `Emma Fischer`)
    expect(first).toContain(`--avatar-`)
    for (const id of [`c-2`, `c-3`, `c-4`, `c-5`, `c-6`]) {
      expect(fallbackColour(id, `Emma Fischer`)).toBe(first)
    }
  })

  it(`keys on the name, else the email, without a prefix`, () => {
    expect(fallbackOf(`c-1`, `Jane Reporter`, `jane@x.dev`).style).toContain(
      `--avatar-${avatarHueIndex(`Jane Reporter`)}`
    )
    expect(avatarHueIndex(`Jane Reporter`)).toBe(6)
    expect(fallbackOf(`c-1`, null, `jane@x.dev`).style).toContain(
      `--avatar-${avatarHueIndex(`jane@x.dev`)}`
    )
  })

  it(`paints an anonymous reporter grey, never keyed on the shown copy`, () => {
    const anonymous = fallbackOf(`c-1`, null)
    expect(anonymous.style).not.toContain(`--avatar-`)
    expect(anonymous.className).toContain(`bg-muted`)
    expect(anonymous.text).toBe(`AV`)
  })
})
