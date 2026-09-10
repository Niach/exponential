import { describe, expect, it } from "vitest"
import {
  dropPendingImage,
  markPendingImageUploaded,
  stagePendingImages,
  type PendingImage,
} from "@/lib/pending-images"
import { MAX_STEER_IMAGES } from "@/lib/steer-image-message"

// EXP-825: the launch composer stages images exactly like the steer composer
// does for a live run — same filter, same cap, same positional markers.

const png = (name: string, size = 10) =>
  new File([new Uint8Array(size)], name, { type: `image/png` })

let urls = 0
const fakeUrl = () => `blob:${++urls}`

describe(`stagePendingImages`, () => {
  it(`drops one marker per accepted image at the caret, numbered from the strip length`, () => {
    const first = stagePendingImages([], [png(`a.png`)], `crop`, 4, fakeUrl)
    expect(first.images).toHaveLength(1)
    expect(first.text).toBe(`crop [Image #1]`)
    expect(first.added).toBe(1)

    const second = stagePendingImages(
      first.images,
      [png(`b.png`), png(`c.png`)],
      first.text,
      first.caret,
      fakeUrl
    )
    expect(second.images).toHaveLength(3)
    expect(second.text).toBe(`crop [Image #1] [Image #2] [Image #3]`)
    expect(second.caret).toBe(second.text.length)
  })

  it(`refuses non-images and oversized files, and caps the strip`, () => {
    const oversized = new File([new Uint8Array(10)], `big.png`, {
      type: `image/png`,
    })
    Object.defineProperty(oversized, `size`, { value: 11 * 1024 * 1024 })
    const text = new File([`x`], `notes.txt`, { type: `text/plain` })
    const files = [
      oversized,
      text,
      ...Array.from({ length: MAX_STEER_IMAGES + 1 }, (_, i) =>
        png(`${i}.png`)
      ),
    ]
    const staged = stagePendingImages([], files, ``, 0, fakeUrl)
    expect(staged.rejected).toBe(2)
    expect(staged.overflow).toBe(1)
    expect(staged.added).toBe(MAX_STEER_IMAGES)
    expect(staged.images).toHaveLength(MAX_STEER_IMAGES)
  })

  it(`returns the same strip when nothing was added`, () => {
    const images: PendingImage[] = [{ file: png(`a.png`), url: `blob:a` }]
    const staged = stagePendingImages(images, [], `hi`, 2, fakeUrl)
    expect(staged.images).toBe(images)
    expect(staged.text).toBe(`hi`)
  })
})

describe(`dropPendingImage`, () => {
  it(`removes the image's markers and renumbers the higher ones`, () => {
    const images: PendingImage[] = [
      { file: png(`a.png`), url: `blob:a` },
      { file: png(`b.png`), url: `blob:b` },
      { file: png(`c.png`), url: `blob:c` },
    ]
    const dropped = dropPendingImage(
      images,
      `blob:b`,
      `see [Image #1] and [Image #2] then [Image #3]`
    )
    expect(dropped.images.map((image) => image.url)).toEqual([
      `blob:a`,
      `blob:c`,
    ])
    expect(dropped.text).toBe(`see [Image #1] and then [Image #2]`)
  })

  it(`is a no-op for an unknown url`, () => {
    const images: PendingImage[] = [{ file: png(`a.png`), url: `blob:a` }]
    const dropped = dropPendingImage(images, `blob:zzz`, `[Image #1]`)
    expect(dropped.images).toBe(images)
    expect(dropped.text).toBe(`[Image #1]`)
  })
})

describe(`markPendingImageUploaded`, () => {
  it(`stamps the uploaded id on the one entry so a retry skips it`, () => {
    const images: PendingImage[] = [
      { file: png(`a.png`), url: `blob:a` },
      { file: png(`b.png`), url: `blob:b` },
    ]
    const stamped = markPendingImageUploaded(images, `blob:a`, `att-1`)
    expect(stamped[0]!.uploadedId).toBe(`att-1`)
    expect(stamped[1]!.uploadedId).toBeUndefined()
  })
})
