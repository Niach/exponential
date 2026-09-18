import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { AttachmentThumb } from "./attachment-thumb"

// EXP-962: the composer tile and the posted inline image are ONE thumb.

const SRC = `/api/attachments/a1`

describe(`AttachmentThumb`, () => {
  it(`is a 64px tile with a corner remove badge by default`, () => {
    const onRemove = vi.fn()
    const { container } = render(
      <AttachmentThumb src={SRC} removeLabel="Remove image" onRemove={onRemove} />
    )
    const img = container.querySelector(`img`)!
    expect(img.className).toContain(`size-16`)
    expect(img.className).toContain(`object-cover`)
    expect(container.querySelector(`[data-slot="attachment-thumb"]`)!.getAttribute(`data-size`)).toBe(`tile`)
    fireEvent.click(screen.getByRole(`button`, { name: `Remove image` }))
    expect(onRemove).toHaveBeenCalledTimes(1)
    expect(container.querySelector(`[data-slot="attachment-open"]`)).toBeNull()
  })

  it(`renders no badge without onRemove`, () => {
    const { container } = render(<AttachmentThumb src={SRC} />)
    expect(container.querySelector(`[data-slot="attachment-remove"]`)).toBeNull()
  })

  it(`is the posted image at its own size inline, reserving the probed box`, () => {
    const { container } = render(
      <AttachmentThumb src={SRC} size="inline" alt="shot.png" width={800} height={600} />
    )
    const img = container.querySelector(`img`)!
    expect(img.className).toContain(`max-h-[480px]`)
    expect(img.className).toContain(`object-contain`)
    expect(img.className).not.toContain(`size-16`)
    expect(img.getAttribute(`width`)).toBe(`800`)
    expect(img.getAttribute(`height`)).toBe(`600`)
    expect(img.style.aspectRatio).toBe(`800 / 600`)
    expect(img.getAttribute(`loading`)).toBe(`lazy`)
    expect(img.getAttribute(`alt`)).toBe(`shot.png`)
  })

  it(`wraps the media in a zoom button with onOpen, beside the badge`, () => {
    const onOpen = vi.fn()
    const onRemove = vi.fn()
    render(
      <AttachmentThumb
        src={SRC}
        size="inline"
        alt="shot.png"
        openLabel="View shot.png"
        onOpen={onOpen}
        removeLabel="Delete shot.png"
        onRemove={onRemove}
        removeClassName="hidden group-hover/attachment:block"
      />
    )
    const open = screen.getByRole(`button`, { name: `View shot.png` })
    expect(open.getAttribute(`data-slot`)).toBe(`attachment-open`)
    expect(open.className).toContain(`cursor-zoom-in`)
    expect(open.querySelector(`img`)).not.toBeNull()
    fireEvent.click(open)
    expect(onOpen).toHaveBeenCalledTimes(1)
    const remove = screen.getByRole(`button`, { name: `Delete shot.png` })
    expect(remove.className).toContain(`group-hover/attachment:block`)
    fireEvent.click(remove)
    expect(onRemove).toHaveBeenCalledTimes(1)
  })

  it(`draws a muted first-frame video for the video kind`, () => {
    const { container } = render(
      <AttachmentThumb src={SRC} kind="video" removeLabel="Remove clip" onRemove={vi.fn()} />
    )
    const video = container.querySelector(`video`)!
    expect(video.className).toContain(`bg-black`)
    expect(video.getAttribute(`preload`)).toBe(`metadata`)
  })
})
