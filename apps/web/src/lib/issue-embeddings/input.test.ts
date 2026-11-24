// @vitest-environment node
import { describe, expect, it } from "vitest"
import {
  buildIssueEmbeddingInput,
  ISSUE_EMBEDDING_INPUT,
  ISSUE_EMBEDDING_MODEL,
  issueEmbeddingHash,
  plainForEmbedding,
  vectorLiteral,
} from "./input"

describe(`plainForEmbedding`, () => {
  it(`keeps the text of images and links and drops their URLs`, () => {
    expect(
      plainForEmbedding(
        `See ![the crash](/api/attachments/0b6f3f0e-8a52-4a55-9d53-1c1f6f3f2b11) and [the docs](https://example.com/a?b=c "Title").`
      )
    ).toBe(`See the crash and the docs.`)
  })

  it(`drops a bare attachment URL and collapses whitespace`, () => {
    expect(plainForEmbedding(`a\n\n\n  b\t/api/attachments/0b6f3f0e-8a52-4a55-9d53-1c1f6f3f2b11  c`)).toBe(`a b c`)
  })

  it(`keeps mentions and issue references: people search for them`, () => {
    expect(plainForEmbedding(`@mia@example.com broke #EXP-42`)).toBe(`@mia@example.com broke #EXP-42`)
  })
})

describe(`buildIssueEmbeddingInput`, () => {
  it(`orders title, description, then the comments oldest first`, () => {
    expect(
      buildIssueEmbeddingInput({ title: `Title`, description: `Body`, comments: [`first`, `second`] })
    ).toBe(`Title\nBody\nfirst\nsecond`)
  })

  it(`leaves out an empty description and empty comments`, () => {
    expect(buildIssueEmbeddingInput({ title: `Title`, description: null, comments: [`  `, `only`] })).toBe(`Title\nonly`)
    expect(buildIssueEmbeddingInput({ title: `Title`, description: ``, comments: [] })).toBe(`Title`)
  })

  it(`caps the description so the first comments still reach the model`, () => {
    const input = buildIssueEmbeddingInput({
      title: `Title`,
      description: `d`.repeat(5000),
      comments: [`the repro steps`],
    })
    expect(input).toBe(`Title\n${`d`.repeat(ISSUE_EMBEDDING_INPUT.descriptionChars)}\nthe repro steps`)
  })

  it(`cuts each comment and stops at the total budget`, () => {
    const comments = Array.from({ length: 40 }, (_, i) => `${i}`.padStart(2, `0`) + `c`.repeat(1000))
    const input = buildIssueEmbeddingInput({ title: `T`, description: null, comments })
    const parts = input.split(`\n`)
    expect(parts[1]).toHaveLength(ISSUE_EMBEDDING_INPUT.commentChars)
    expect(parts[1]!.startsWith(`00`)).toBe(true)
    expect(input.length).toBeLessThanOrEqual(ISSUE_EMBEDDING_INPUT.totalChars)
    // 2400 total / (240 + newline): nine whole comments after the title.
    expect(parts).toHaveLength(10)
  })

  it(`never cuts through a surrogate pair`, () => {
    const title = `${`a`.repeat(ISSUE_EMBEDDING_INPUT.titleChars - 1)}😀tail`
    const input = buildIssueEmbeddingInput({ title, description: null, comments: [] })
    expect(input).toBe(`a`.repeat(ISSUE_EMBEDDING_INPUT.titleChars - 1))
  })
})

describe(`issueEmbeddingHash`, () => {
  it(`is stable for the same input and model`, () => {
    expect(issueEmbeddingHash(`Title\nBody`)).toBe(issueEmbeddingHash(`Title\nBody`))
    expect(issueEmbeddingHash(`Title\nBody`)).toMatch(/^[0-9a-f]{64}$/)
  })

  it(`changes with the text and with the model`, () => {
    expect(issueEmbeddingHash(`Title\nBody`)).not.toBe(issueEmbeddingHash(`Title\nBody.`))
    expect(issueEmbeddingHash(`Title`, `other-model/1`)).not.toBe(issueEmbeddingHash(`Title`, ISSUE_EMBEDDING_MODEL))
  })

  it(`ignores what the input ignores: a new attachment URL, reflowed whitespace`, () => {
    const a = buildIssueEmbeddingInput({ title: `T`, description: `see ![shot](/api/attachments/0b6f3f0e-8a52-4a55-9d53-1c1f6f3f2b11)`, comments: [] })
    const b = buildIssueEmbeddingInput({ title: `T`, description: `see  ![shot](/api/attachments/11111111-8a52-4a55-9d53-1c1f6f3f2b11)\n`, comments: [] })
    expect(issueEmbeddingHash(a)).toBe(issueEmbeddingHash(b))
  })
})

describe(`vectorLiteral`, () => {
  it(`writes pgvector's text form`, () => {
    expect(vectorLiteral(new Float32Array([0.5, -1, 0]))).toBe(`[0.5,-1,0]`)
    expect(vectorLiteral([Number.NaN, 1])).toBe(`[0,1]`)
  })
})
