import { render, within } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import type { Attachment } from "@/db/schema"
import { TooltipProvider } from "@/components/ui/tooltip"
import { CommentAttachments } from "@/components/comment-rows/attachments"

// EXP-723: a comment's images are LARGE inline tiles, not 64px icons. The two
// things that can silently regress are the size cap and the reserved aspect
// ratio (without it the feed jumps as every screenshot decodes), so both are
// pinned here.

function attachment(overrides: Partial<Attachment>): Attachment {
  return {
    id: `a1`,
    teamId: `t1`,
    issueId: `i1`,
    boardId: `b1`,
    boardDeletedAt: null,
    boardArchivedAt: null,
    commentId: `c1`,
    uploaderId: `u1`,
    filename: `shot.png`,
    contentType: `image/png`,
    sizeBytes: 1024,
    storageKey: `k`,
    url: `/api/attachments/a1`,
    width: null,
    height: null,
    createdAt: new Date(),
    updatedAt: new Date(),
    ...overrides,
  } as Attachment
}

describe(`CommentAttachments`, () => {
  it(`renders an image as a large tile with its probed aspect ratio`, () => {
    const { container } = render(
      <CommentAttachments
        attachments={[attachment({ width: 1600, height: 900 })]}
        canModify={false}
      />
    )
    const img = container.querySelector(`img`)
    expect(img).not.toBeNull()
    expect(img!.className).toContain(`max-h-[480px]`)
    expect(img!.className).toContain(`max-w-full`)
    expect(img!.className).not.toContain(`size-16`)
    expect(img!.style.aspectRatio).toBe(`1600 / 900`)
    // The intrinsic size reserves the box before the bytes land.
    expect(img!.getAttribute(`width`)).toBe(`1600`)
    expect(img!.getAttribute(`height`)).toBe(`900`)
  })

  it(`leaves the ratio unset for a row with no probed dimensions`, () => {
    const { container } = render(
      <CommentAttachments attachments={[attachment({})]} canModify={false} />
    )
    const img = container.querySelector(`img`)
    expect(img!.style.aspectRatio).toBe(``)
    expect(img!.getAttribute(`width`)).toBeNull()
  })

  it(`stacks images vertically rather than wrapping them into a row`, () => {
    const { container } = render(
      <CommentAttachments
        attachments={[
          attachment({ id: `a1`, width: 800, height: 600 }),
          attachment({ id: `a2`, filename: `two.png`, width: 800, height: 600 }),
        ]}
        canModify={false}
      />
    )
    expect(container.querySelectorAll(`img`).length).toBe(2)
    const root = container.firstElementChild as HTMLElement
    expect(root.className).toContain(`flex-col`)
  })

  it(`keeps a non-image as a chip, not a tile`, () => {
    const { container, getByText } = render(
      // The chip's open/download buttons carry tooltips.
      <TooltipProvider>
        <CommentAttachments
          attachments={[
            attachment({
              id: `a3`,
              filename: `notes.pdf`,
              contentType: `application/pdf`,
            }),
          ]}
          canModify={false}
        />
      </TooltipProvider>
    )
    expect(container.querySelector(`img`)).toBeNull()
    expect(getByText(`notes.pdf`)).toBeTruthy()
  })

  it(`offers the remove badge only to the author`, () => {
    const rows = [attachment({ width: 400, height: 300 })]
    const mine = render(<CommentAttachments attachments={rows} canModify />)
    expect(
      within(mine.container).queryByLabelText(`Delete shot.png`)
    ).not.toBeNull()

    const theirs = render(
      <CommentAttachments attachments={rows} canModify={false} />
    )
    expect(
      within(theirs.container).queryByLabelText(`Delete shot.png`)
    ).toBeNull()
  })

  it(`renders nothing without attachments`, () => {
    const { container } = render(
      <CommentAttachments attachments={[]} canModify />
    )
    expect(container.firstChild).toBeNull()
  })
})
