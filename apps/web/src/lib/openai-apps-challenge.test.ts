import { describe, expect, it } from "vitest"
import { handleOpenAiAppsChallenge } from "./openai-apps-challenge"

// EXP-1153: the directory's domain check reads the body byte for byte.
describe(`openai-apps-challenge`, () => {
  it(`serves the bare token as plain text, uncached`, async () => {
    const res = handleOpenAiAppsChallenge(`  abc123-token \n`)
    expect(res.status).toBe(200)
    expect(res.headers.get(`content-type`)).toBe(`text/plain; charset=utf-8`)
    expect(res.headers.get(`cache-control`)).toBe(`no-store`)
    expect(await res.text()).toBe(`abc123-token`)
  })

  it(`is a 404 on an instance that never submitted a plugin`, async () => {
    expect(handleOpenAiAppsChallenge(undefined).status).toBe(404)
    expect(handleOpenAiAppsChallenge(``).status).toBe(404)
    expect(handleOpenAiAppsChallenge(`   `).status).toBe(404)
  })
})
