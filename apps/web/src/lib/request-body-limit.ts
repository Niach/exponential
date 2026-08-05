// Hard cap on request bodies, enforced by Bun BEFORE any handler buffers the
// stream — the per-route Content-Length checks are only fast-path rejects and
// are trivially bypassed by chunked transfer encoding. Must stay ABOVE every
// advertised upload contract (widget submit: screenshot + 3 pictures at 10 MB
// each; issue file uploads: 50 MB per file) — Bun's own 413 carries no CORS
// headers, so an over-cap widget submit surfaces as an opaque network error.
// 64 MB covers both contracts with multipart headroom while shutting the door
// on Bun's ~128 MB default. Lock-tested in request-body-limit.test.ts.
export const MAX_REQUEST_BODY_BYTES = 64 * 1024 * 1024
