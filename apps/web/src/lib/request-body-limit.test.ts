import { describe, expect, it } from 'vitest'

import { MAX_REQUEST_BODY_BYTES } from './request-body-limit'
import { maxFileUploadBytes } from './storage/issue-attachments'
import { maxSubmitRequestBytes } from './widget/service'

// Bun rejects over-cap bodies BEFORE any handler runs, and its 413 carries no
// CORS headers — a cross-origin widget submit over the cap surfaces as an
// opaque "Network error". Every advertised upload contract must therefore fit
// under the server-wide body cap, with headroom for multipart framing.
const multipartHeadroom = 2 * 1024 * 1024

describe(`MAX_REQUEST_BODY_BYTES`, () => {
  it(`covers the widget submit budget`, () => {
    expect(maxSubmitRequestBytes).toBeLessThanOrEqual(MAX_REQUEST_BODY_BYTES)
  })

  it(`covers the issue file upload cap`, () => {
    expect(maxFileUploadBytes + multipartHeadroom).toBeLessThanOrEqual(
      MAX_REQUEST_BODY_BYTES,
    )
  })

  it(`covers a base64-inflated MCP attachments_upload of the max file size`, () => {
    // exponential_attachments_upload rides the file as base64 inside a
    // JSON-RPC envelope: ceil(n/3)*4 bytes of payload plus envelope headroom.
    const base64Inflated = Math.ceil(maxFileUploadBytes / 3) * 4
    expect(base64Inflated + multipartHeadroom).toBeLessThanOrEqual(
      MAX_REQUEST_BODY_BYTES,
    )
  })
})
