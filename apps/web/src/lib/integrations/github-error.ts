// FEED-74: ONE formatter for a refused GitHub call. Every GitHub error the
// server raises (PR create/merge/update/retarget/close, the installation token
// mint, the state reads) goes through here so its text is never empty: the
// 2026-10-06 release train saw `GitHub update failed:` and `GitHub merge failed
// (HTTP 500):  (request …)` — GitHub's 500 came with an EMPTY body and the old
// per-call blocks kept `text.slice(0, 300)` verbatim.
//
// Pure (no db import): `github-app.ts` and `github-pr.ts` both import it, and
// `github-pr` already imports `github-app`.

const EXCERPT_MAX = 300

interface GithubErrorEntry {
  resource?: string
  field?: string
  code?: string
  message?: string
}

/** What GitHub's error body says: `message` plus the `errors[]` detail a 422
 *  carries (`Validation Failed: PullRequest.head invalid`), or a trimmed
 *  excerpt of a non-JSON body. Null when the body says nothing. */
export function describeGithubErrorBody(text: string): string | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(trimmed)
  } catch {
    // Non-JSON error body (an HTML gateway page) — surface the raw text.
    return trimmed.slice(0, EXCERPT_MAX)
  }
  if (!parsed || typeof parsed !== `object`) return trimmed.slice(0, EXCERPT_MAX)
  const body = parsed as { message?: unknown; errors?: unknown }
  const message = typeof body.message === `string` ? body.message.trim() : ``
  const details = Array.isArray(body.errors)
    ? body.errors
        .map((entry) => describeErrorEntry(entry))
        .filter((entry): entry is string => Boolean(entry))
    : []
  if (!message && details.length === 0) return trimmed.slice(0, EXCERPT_MAX)
  if (details.length === 0) return message
  const joined = details.join(`; `)
  return message ? `${message}: ${joined}` : joined
}

function describeErrorEntry(entry: unknown): string | null {
  if (typeof entry === `string`) return entry.trim() || null
  if (!entry || typeof entry !== `object`) return null
  const e = entry as GithubErrorEntry
  if (typeof e.message === `string` && e.message.trim()) return e.message.trim()
  const subject = [e.resource, e.field].filter(Boolean).join(`.`)
  const parts = [subject, e.code].filter(Boolean)
  return parts.length ? parts.join(` `) : null
}

export interface GithubErrorSource {
  status?: number
  // Optional so unit stubs stay two-liners; the real `fetch` always has it.
  headers?: { get: (name: string) => string | null }
}

/** The one-line reason for a refused GitHub call: the body's description or
 *  `empty response body`, plus GitHub's request id on a 5xx (what their
 *  support can trace, FEED-64; a 4xx refusal explains itself). */
export function formatGithubError(
  opts: GithubErrorSource & { text: string }
): string {
  const message = describeGithubErrorBody(opts.text) ?? `empty response body`
  const requestId =
    opts.status !== undefined && opts.status >= 500
      ? opts.headers?.get(`x-github-request-id`)
      : null
  return requestId ? `${message} (request ${requestId})` : message
}

/** `formatGithubError` over a response whose body is still unread. */
export async function githubErrorMessage(
  res: GithubErrorSource & { text?: () => Promise<string> }
): Promise<string> {
  let text = ``
  if (typeof res.text === `function`) {
    try {
      text = await res.text()
    } catch {
      // An unreadable body is an empty one for the message's purposes.
    }
  }
  return formatGithubError({ status: res.status, headers: res.headers, text })
}
