import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { ImagePreviewDialog, PreviewMedia } from "./image-preview-dialog"

// EXP-316/EXP-824: the shared lightbox — one dialog, three bodies. The switch
// lives in `PreviewMedia` so a host that cannot run a Radix portal (the
// styleguide's islands) still renders the specimen.

describe(`PreviewMedia`, () => {
  it(`draws an image by default`, () => {
    const { container } = render(
      <PreviewMedia src="/x.png" alt="a shot" label="x.png" />
    )
    const img = container.querySelector(`img`)
    expect(img?.getAttribute(`src`)).toBe(`/x.png`)
    expect(img?.getAttribute(`alt`)).toBe(`a shot`)
    expect(container.querySelector(`video`)).toBeNull()
  })

  it(`plays a video with its poster, and an audio clip under its name`, () => {
    const video = render(
      <PreviewMedia src="/clip.mp4" label="clip.mp4" kind="video" poster="/p.jpg" />
    )
    const node = video.container.querySelector(`video`)
    expect(node?.getAttribute(`src`)).toBe(`/clip.mp4`)
    expect(node?.getAttribute(`poster`)).toBe(`/p.jpg`)

    const audio = render(
      <PreviewMedia src="/voice.m4a" label="voice.m4a" kind="audio" />
    )
    expect(audio.container.querySelector(`audio`)?.getAttribute(`src`)).toBe(
      `/voice.m4a`
    )
    // The audio arm is the only one that names the file visibly.
    expect(audio.getByText(`voice.m4a`)).toBeTruthy()
  })
})

describe(`ImagePreviewDialog`, () => {
  it(`titles the dialog with the label, invisibly`, () => {
    render(
      <ImagePreviewDialog
        open
        onOpenChange={() => {}}
        src="/x.png"
        alt="a shot"
        label="x.png"
      />
    )
    const dialog = document.querySelector(`[role="dialog"]`)
    expect(dialog).toBeTruthy()
    expect(screen.getByText(`x.png`).className).toContain(`sr-only`)
    expect(dialog?.querySelector(`img`)?.getAttribute(`src`)).toBe(`/x.png`)
  })

  it(`renders nothing while closed`, () => {
    render(
      <ImagePreviewDialog
        open={false}
        onOpenChange={() => {}}
        src="/x.png"
        label="x.png"
      />
    )
    expect(document.querySelector(`[role="dialog"]`)).toBeNull()
  })
})
