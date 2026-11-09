import { describe, expect, it } from "bun:test"
import { composerPaste } from "./composer-paste"

const file = (name: string, size: number, type: string) => new File([`x`.repeat(size)], name, { type })

describe(`composerPaste (#EDE-65 R)`, () => {
  it(`takes image files, in order, and drops the rest`, () => {
    const p = composerPaste({ files: [file(`a.png`, 3, `image/png`), file(`s.pdf`, 7, `application/pdf`), file(`b.jpg`, 5, `image/jpeg`)] as unknown as FileList })
    expect(p.take).toBe(true)
    expect(p.files).toEqual([{ name: `a.png`, size: 3, type: `image/png` }, { name: `b.jpg`, size: 5, type: `image/jpeg` }])
    expect(p.images.map((f) => f.name)).toEqual([`a.png`, `b.jpg`])
  })
  it(`files without an image are taken, nothing attached`, () => {
    const p = composerPaste({ files: [file(`s.pdf`, 7, `application/pdf`)] as unknown as FileList })
    expect(p).toMatchObject({ take: true, files: [] })
  })
  it(`text alone is the field's own`, () => {
    expect(composerPaste({ files: [] as unknown as FileList })).toMatchObject({ take: false, files: [] })
    expect(composerPaste(null).take).toBe(false)
  })
})
