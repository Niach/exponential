import type { ReactNode } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { IssueEditorAttachmentButton } from "@/components/issue-editor-dialog-shell"

vi.mock(`@/components/ui/tooltip`, () => ({
  Tooltip: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  TooltipContent: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  TooltipTrigger: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
}))

describe(`IssueEditorAttachmentButton`, () => {
  it(`forwards selected files to the upload handler`, () => {
    const onFiles = vi.fn()
    const { container } = render(
      <IssueEditorAttachmentButton onFiles={onFiles} />
    )

    const input = container.querySelector(`input[type="file"]`)

    expect(input).toBeTruthy()

    fireEvent.change(input as HTMLInputElement, {
      target: {
        files: [new File([`image`], `test.png`, { type: `image/png` })],
      },
    })

    expect(onFiles).toHaveBeenCalledWith([
      expect.objectContaining({
        name: `test.png`,
        type: `image/png`,
      }),
    ])
  })

  it(`disables the button when image upload is unavailable`, () => {
    render(
      <IssueEditorAttachmentButton
        disabled
        disabledReason="Create the issue first to add images"
      />
    )

    expect(
      screen.getByLabelText(`Add image`).getAttribute(`disabled`)
    ).not.toBeNull()
  })
})
