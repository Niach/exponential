import { describe, expect, it } from "vitest"
import {
  FILES_NEED_NEWER_DEVICE,
  STEER_FILES_CAP,
  deviceAcceptsFiles,
  filePickAccept,
  gateFilesForDevice,
} from "@/lib/steer-files-gate"

// F6 (release train 2026-10-10): the composers refuse non-image files for a
// device without the `steer-files` cap, with the server's own sentence.

const png = new File([new Uint8Array(16)], `shot.png`, { type: `image/png` })
const pdf = new File([new Uint8Array(16)], `spec.pdf`, { type: `application/pdf` })
const empty = new File([], `empty.txt`, { type: `text/plain` })

describe(`deviceAcceptsFiles`, () => {
  it(`reads the cap`, () => {
    expect(STEER_FILES_CAP).toBe(`steer-files`)
    expect(deviceAcceptsFiles({ caps: [`actions`, `steer-files`] })).toBe(true)
    expect(deviceAcceptsFiles({ caps: [`actions`] })).toBe(false)
    expect(deviceAcceptsFiles({ caps: [] })).toBe(false)
  })

  it(`lets an unknown device through: the server gate decides`, () => {
    expect(deviceAcceptsFiles(null)).toBe(true)
    expect(deviceAcceptsFiles(undefined)).toBe(true)
    expect(deviceAcceptsFiles({})).toBe(true)
    expect(deviceAcceptsFiles({ caps: null })).toBe(true)
  })
})

describe(`gateFilesForDevice`, () => {
  it(`passes everything for a device with the cap`, () => {
    expect(gateFilesForDevice([png, pdf, empty], true)).toEqual({
      files: [png, pdf, empty],
      droppedFiles: 0,
    })
  })

  it(`keeps images (and what the size toast refuses) and drops files without it`, () => {
    expect(gateFilesForDevice([png, pdf, empty, pdf], false)).toEqual({
      files: [png, empty],
      droppedFiles: 2,
    })
  })

  it(`pins the server's sentence`, () => {
    expect(FILES_NEED_NEWER_DEVICE).toBe(
      `Attaching files needs the device on 0.14.66 or newer; images still work`
    )
  })
})

describe(`filePickAccept`, () => {
  it(`narrows the picker to images while files are refused`, () => {
    expect(filePickAccept(true)).toBeUndefined()
    expect(filePickAccept(false)).toBe(`image/*`)
  })
})
