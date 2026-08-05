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
})
