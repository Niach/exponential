import { describe, expect, it, vi, afterEach } from "vitest"
import { render, waitFor } from "@testing-library/react"
import {
  AttachmentMarkdownPreviewDialog,
  loadMarkdownPreview,
} from "@/components/attachment-markdown-preview"
import { MARKDOWN_PREVIEW_MAX_BYTES } from "@/lib/attachment-files"

// EXP-955: a `.md` attachment previews IN the app through the shared
// read-only markdown renderer instead of round-tripping the byte route as a
// download.

const target = {
  id: `a1`,
  url: `/api/attachments/a1`,
  filename: `tree-swarms-research.md`,
  sizeBytes: 22_345,
}

function fetchWith(body: string, status = 200) {
  return vi.fn(async () => new Response(body, { status })) as unknown as typeof fetch
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe(`loadMarkdownPreview`, () => {
  it(`fetches the bytes through the attachment route with cookies`, async () => {
    const fetchImpl = fetchWith(`# Hello\n\nworld`)
    const state = await loadMarkdownPreview(target, fetchImpl)
    expect(state).toEqual({ status: `ready`, markdown: `# Hello\n\nworld` })
    expect(fetchImpl).toHaveBeenCalledWith(`/api/attachments/a1`, {
      credentials: `same-origin`,
    })
  })

  it(`refuses to fetch a row already known to be over the ceiling`, async () => {
    const fetchImpl = fetchWith(``)
    const state = await loadMarkdownPreview(
      { ...target, sizeBytes: MARKDOWN_PREVIEW_MAX_BYTES + 1 },
      fetchImpl
    )
    expect(state).toEqual({ status: `too-large` })
    expect(fetchImpl).not.toHaveBeenCalled()
  })

  it(`bounds an unsized (legacy 0-byte) row by the body it fetched`, async () => {
    const state = await loadMarkdownPreview(
      { ...target, sizeBytes: 0 },
      fetchWith(`x`.repeat(MARKDOWN_PREVIEW_MAX_BYTES + 1))
    )
    expect(state).toEqual({ status: `too-large` })
  })

  it(`names a deleted file and other HTTP failures`, async () => {
    expect(await loadMarkdownPreview(target, fetchWith(``, 404))).toEqual({
      status: `error`,
      message: `This file is no longer available.`,
    })
    expect(await loadMarkdownPreview(target, fetchWith(``, 500))).toEqual({
      status: `error`,
      message: `Couldn't load this file (HTTP 500).`,
    })
  })
})

describe(`AttachmentMarkdownPreviewDialog`, () => {
  it(`renders the fetched markdown through the read-only editor`, async () => {
    vi.stubGlobal(
      `fetch`,
      fetchWith(`## Swarms\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n`)
    )
    const { baseElement, getByText } = render(
      <AttachmentMarkdownPreviewDialog
        attachment={target}
        open
        onOpenChange={() => {}}
      />
    )
    await waitFor(() => {
      expect(baseElement.querySelector(`.tiptap-content h2`)).toBeTruthy()
    })
    expect(baseElement.querySelector(`.tiptap-content table`)).toBeTruthy()
    // The header names the file and its synced size; download stays a click
    // away and forces the attachment disposition.
    expect(getByText(`tree-swarms-research.md`)).toBeTruthy()
    expect(getByText(`Markdown · 22 KB`)).toBeTruthy()
    const download = baseElement.querySelector(
      `a[href="/api/attachments/a1?download=1"]`
    )
    expect(download).toBeTruthy()
    expect(download!.getAttribute(`download`)).toBe(`tree-swarms-research.md`)
  })

  it(`shows the download hint for a file over the ceiling`, async () => {
    const fetchImpl = fetchWith(``)
    vi.stubGlobal(`fetch`, fetchImpl)
    const { getByText } = render(
      <AttachmentMarkdownPreviewDialog
        attachment={{ ...target, sizeBytes: MARKDOWN_PREVIEW_MAX_BYTES * 2 }}
        open
        onOpenChange={() => {}}
      />
    )
    await waitFor(() => {
      expect(getByText(/too large to preview/)).toBeTruthy()
    })
    expect(fetchImpl).not.toHaveBeenCalled()
  })
})
