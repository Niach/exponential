// FEED-75: the object store throttled us and every retry inside
// `lib/storage` still hit the wall. Its own module (not `lib/storage/index`)
// so route tests that mock the storage client whole can still throw and
// match it.
//
// Hetzner's Ceph front answers `503 SlowDown` under load: the SDK's three
// sub-second attempts all land inside the same throttle window, which is how
// big PNGs failed while a JPEG a few seconds later went through. The HTTP
// routes map this to 503 + `Retry-After` (`lib/http-errors.ts`), never the
// generic 500 that hid the cause; MCP tools surface the message as-is.

export const STORAGE_RETRY_AFTER_SECONDS = 5

export class StorageThrottledError extends Error {
  readonly retryAfterSeconds = STORAGE_RETRY_AFTER_SECONDS

  constructor(options?: { cause?: unknown }) {
    super(
      `The file store is rate-limiting requests right now. Retry the same command in ${STORAGE_RETRY_AFTER_SECONDS} seconds.`,
      options
    )
    this.name = `StorageThrottledError`
  }
}
