import { describe, expect, it } from "vitest"
import {
  dropPendingImage,
  markPendingImageUploaded,
  stagePendingImages,
  type PendingAttachment as PendingImage,
  classifyPendingFile,
  uploadedWireParts,
  ATTACHMENT_REJECTED_TOAST,
  FILE_CAP_TOAST,
} from "@/lib/pending-images"
import { MAX_STEER_FILES, MAX_STEER_IMAGES } from "@/lib/steer-image-message"

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

  it(`refuses oversized images and caps the strip`, () => {
    const oversized = new File([new Uint8Array(10)], `big.png`, {
      type: `image/png`,
    })
    Object.defineProperty(oversized, `size`, { value: 11 * 1024 * 1024 })
    const files = [
      oversized,
      ...Array.from({ length: MAX_STEER_IMAGES + 1 }, (_, i) =>
        png(`${i}.png`)
      ),
    ]
    const staged = stagePendingImages([], files, ``, 0, fakeUrl)
    expect(staged.rejected).toBe(1)
    expect(staged.overflow).toBe(1)
    expect(staged.added).toBe(MAX_STEER_IMAGES)
    expect(staged.images).toHaveLength(MAX_STEER_IMAGES)
  })

  it(`returns the same strip when nothing was added`, () => {
    const images: PendingImage[] = [{ kind: `image`, file: png(`a.png`), url: `blob:a` }]
    const staged = stagePendingImages(images, [], `hi`, 2, fakeUrl)
    expect(staged.images).toBe(images)
    expect(staged.text).toBe(`hi`)
  })
})

describe(`dropPendingImage`, () => {
  it(`removes the image's markers and renumbers the higher ones`, () => {
    const images: PendingImage[] = [
      { kind: `image`, file: png(`a.png`), url: `blob:a` },
      { kind: `image`, file: png(`b.png`), url: `blob:b` },
      { kind: `image`, file: png(`c.png`), url: `blob:c` },
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
    const images: PendingImage[] = [{ kind: `image`, file: png(`a.png`), url: `blob:a` }]
    const dropped = dropPendingImage(images, `blob:zzz`, `[Image #1]`)
    expect(dropped.images).toBe(images)
    expect(dropped.text).toBe(`[Image #1]`)
  })
})

describe(`markPendingImageUploaded`, () => {
  it(`stamps the uploaded id on the one entry so a retry skips it`, () => {
    const images: PendingImage[] = [
      { kind: `image`, file: png(`a.png`), url: `blob:a` },
      { kind: `image`, file: png(`b.png`), url: `blob:b` },
    ]
    const stamped = markPendingImageUploaded(images, `blob:a`, `att-1`)
    expect(stamped[0]!.uploadedId).toBe(`att-1`)
    expect(stamped[1]!.uploadedId).toBeUndefined()
  })
})

// Wave D: any file rides beside the images — no marker, its own cap.
const doc = (name: string, type = `application/pdf`, size = 10) => {
  const file = new File([new Uint8Array(1)], name, { type })
  Object.defineProperty(file, `size`, { value: size })
  return file
}

describe(`files beside images`, () => {
  it(`classifies by type and the per-kind cap`, () => {
    expect(classifyPendingFile(png(`a.png`))).toBe(`image`)
    expect(classifyPendingFile(doc(`a.pdf`))).toBe(`file`)
    expect(classifyPendingFile(doc(`big.png`, `image/png`, 11 * 1024 * 1024))).toBeNull()
    expect(classifyPendingFile(doc(`a.zip`, `application/zip`, 40 * 1024 * 1024))).toBe(`file`)
    expect(classifyPendingFile(doc(`huge.zip`, `application/zip`, 51 * 1024 * 1024))).toBeNull()
    expect(classifyPendingFile(doc(`empty.txt`, `text/plain`, 0))).toBeNull()
  })

  it(`stages a file with no marker and numbers images past it`, () => {
    const staged = stagePendingImages(
      [],
      [doc(`notes.pdf`), png(`a.png`)],
      `look`,
      4,
      fakeUrl
    )
    expect(staged.images.map((entry) => entry.kind)).toEqual([`file`, `image`])
    expect(staged.text).toBe(`look [Image #1]`)
    expect(staged.added).toBe(2)
  })

  it(`caps files at four on their own`, () => {
    const files = Array.from({ length: MAX_STEER_FILES + 1 }, (_, i) =>
      doc(`${i}.txt`, `text/plain`)
    )
    const staged = stagePendingImages([], [...files, png(`a.png`)], ``, 0, fakeUrl)
    expect(staged.fileOverflow).toBe(1)
    expect(staged.overflow).toBe(0)
    expect(staged.added).toBe(MAX_STEER_FILES + 1)
  })

  it(`drops a file without touching the markers, and renumbers by image position`, () => {
    const images: PendingImage[] = [
      { kind: `file`, file: doc(`a.pdf`), url: `blob:f` },
      { kind: `image`, file: png(`a.png`), url: `blob:a` },
      { kind: `image`, file: png(`b.png`), url: `blob:b` },
    ]
    expect(dropPendingImage(images, `blob:f`, `[Image #1] [Image #2]`).text).toBe(
      `[Image #1] [Image #2]`
    )
    expect(dropPendingImage(images, `blob:a`, `[Image #1] [Image #2]`).text).toBe(
      `[Image #1]`
    )
  })

  it(`splits the uploaded strip into image ids and named files`, () => {
    const images = markPendingImageUploaded(
      markPendingImageUploaded(
        [
          { kind: `image`, file: png(`a.png`), url: `blob:a` },
          { kind: `file`, file: doc(`raw name.pdf`), url: `blob:f` },
        ],
        `blob:a`,
        `img-1`
      ),
      `blob:f`,
      `file-1`,
      `raw name.pdf`
    )
    expect(uploadedWireParts(images)).toEqual({
      imageIds: [`img-1`],
      files: [{ id: `file-1`, name: `raw name.pdf` }],
    })
  })

  it(`pins the toast copy`, () => {
    expect(ATTACHMENT_REJECTED_TOAST).toBe(
      `Images up to 10 MB and files up to 50 MB can be attached`
    )
    expect(FILE_CAP_TOAST).toBe(`Up to 4 files per message`)
  })
})
