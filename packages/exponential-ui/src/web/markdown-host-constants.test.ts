import { describe, expect, test } from "bun:test"

import host from "../../catalog/host.json" with { type: "json" }
import { ATTACHMENT_KINDS, CHIP_KINDS, MARKDOWN_IMAGE_MAX_HEIGHT, MARKDOWN_LAZY_BLOCKS } from "./markdown"

// The parser inlines these so its bundle never carries the whole host
// contract; they must stay the contract's.
describe(`markdown constants = catalog/host.json`, () => {
  test(`lazyBlocks, chipKinds, attachmentKinds, imageMaxHeight`, () => {
    expect(MARKDOWN_LAZY_BLOCKS).toBe(host.markdown.lazyBlocks)
    expect(CHIP_KINDS).toEqual(host.markdown.chipKinds)
    expect(ATTACHMENT_KINDS).toEqual(host.markdown.attachmentKinds)
    expect(MARKDOWN_IMAGE_MAX_HEIGHT).toBe(host.markdown.imageMaxHeight)
  })
})
