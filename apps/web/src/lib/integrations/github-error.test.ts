import { describe, expect, it } from "vitest"
import {
  describeGithubErrorBody,
  formatGithubError,
  githubErrorMessage,
} from "@/lib/integrations/github-error"

// FEED-74: the 2026-10-06 release train saw `GitHub update failed:` and
// `GitHub merge failed (HTTP 500):  (request …)` — GitHub's 500 sent an empty
// body and the old per-call blocks surfaced it verbatim. The formatter's text
// is never empty, carries GitHub's `errors[]` detail, and keeps FEED-64's
// request id on a 5xx only.

const headers = new Headers({ "x-github-request-id": `1234:ABCD` })

describe(`describeGithubErrorBody`, () => {
  it(`is null for an empty or whitespace body`, () => {
    expect(describeGithubErrorBody(``)).toBeNull()
    expect(describeGithubErrorBody(`  \n`)).toBeNull()
  })

  it(`keeps a plain message`, () => {
    expect(describeGithubErrorBody(`{"message":"Not Found"}`)).toBe(`Not Found`)
  })

  it(`appends the errors[] detail after the message`, () => {
    expect(
      describeGithubErrorBody(
        JSON.stringify({
          message: `Validation Failed`,
          errors: [{ resource: `PullRequest`, field: `head`, code: `invalid` }],
          documentation_url: `https://docs.github.com/rest`,
        })
      )
    ).toBe(`Validation Failed: PullRequest.head invalid`)
    expect(
      describeGithubErrorBody(
        JSON.stringify({
          message: `Validation Failed`,
          errors: [
            { message: `A pull request already exists for o:exp/x.` },
            `base branch is protected`,
          ],
        })
      )
    ).toBe(
      `Validation Failed: A pull request already exists for o:exp/x.; base branch is protected`
    )
  })

  it(`uses the detail alone when the message is missing`, () => {
    expect(
      describeGithubErrorBody(
        JSON.stringify({ errors: [{ message: `No commits between a and b` }] })
      )
    ).toBe(`No commits between a and b`)
  })

  it(`falls back to the raw excerpt for JSON without a message or a non-JSON body`, () => {
    expect(describeGithubErrorBody(`{"message":""}`)).toBe(`{"message":""}`)
    expect(describeGithubErrorBody(`{"status":"500"}`)).toBe(`{"status":"500"}`)
    expect(describeGithubErrorBody(`<html><body>Bad Gateway</body></html>`)).toBe(
      `<html><body>Bad Gateway</body></html>`
    )
    expect(describeGithubErrorBody(`x`.repeat(500))).toHaveLength(300)
  })
})

describe(`formatGithubError`, () => {
  it(`names an empty body and carries the request id on a 5xx`, () => {
    expect(formatGithubError({ status: 500, headers, text: `` })).toBe(
      `empty response body (request 1234:ABCD)`
    )
    expect(
      formatGithubError({ status: 502, headers, text: `{"message":"Server Error"}` })
    ).toBe(`Server Error (request 1234:ABCD)`)
  })

  it(`never adds the request id to a 4xx refusal`, () => {
    expect(
      formatGithubError({
        status: 405,
        headers,
        text: `{"message":"Pull Request is not mergeable"}`,
      })
    ).toBe(`Pull Request is not mergeable`)
    expect(formatGithubError({ status: 404, headers, text: `` })).toBe(
      `empty response body`
    )
  })

  it(`tolerates a stub without headers`, () => {
    expect(formatGithubError({ status: 500, text: `` })).toBe(`empty response body`)
  })
})

describe(`githubErrorMessage`, () => {
  it(`reads the body once and formats it`, async () => {
    await expect(
      githubErrorMessage({
        status: 500,
        headers,
        text: async () => ``,
      })
    ).resolves.toBe(`empty response body (request 1234:ABCD)`)
  })

  it(`treats a missing or failing text() as an empty body`, async () => {
    await expect(githubErrorMessage({ status: 503 })).resolves.toBe(
      `empty response body`
    )
    await expect(
      githubErrorMessage({
        status: 503,
        text: async () => {
          throw new Error(`socket closed`)
        },
      })
    ).resolves.toBe(`empty response body`)
  })
})
