// @vitest-environment node
//
// #NEW-134: the REAL model, tokenizer and worker. Needs the weights on disk
// (`bun run embeddings:model`); skips without them, and FAILS instead when
// EMBEDDING_MODEL_REQUIRED is set (the CI job that fetched them).
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { afterAll, describe, expect, it } from "vitest"
import { Tokenizer } from "@huggingface/tokenizers"
import embeddingModel from "../../../embedder/model.json"
// @ts-expect-error -- plain .mjs beside the worker, no types
import { buildTokenizer, encodeForModel } from "../../../embedder/tokenizer.mjs"
import { embedQuery, embedText, embedderConfigured, embedderStats, stopEmbedder } from "./embedder"

const modelDir = join(process.cwd(), `embedder`, `models`, `multilingual-e5-small`)
const tokenizerPath = join(modelDir, `tokenizer.json`)
const present = embeddingModel.source.files.every((file) => existsSync(join(modelDir, file.name)))
if (!present && process.env.EMBEDDING_MODEL_REQUIRED) {
  throw new Error(`the embedding model is required but not in ${modelDir} (bun run embeddings:model)`)
}
if (!present) console.info(`[embedder.model] skipped: no model in ${modelDir}`)

const cosine = (a: ArrayLike<number>, b: ArrayLike<number>) => {
  let sum = 0
  for (let i = 0; i < a.length; i++) sum += a[i]! * b[i]!
  return sum
}

describe.skipIf(!present)(`the embedding tokenizer`, () => {
  it(`the map-backed prefix search tokenizes exactly like the library's trie`, () => {
    const json = readFileSync(tokenizerPath, `utf8`)
    const compact = buildTokenizer(JSON.parse(json), { maxTokens: 512 })
    const stock = new Tokenizer(JSON.parse(json), { model_max_length: 512 })
    const samples = [
      `passage: Login button does nothing on Safari`,
      `query: Der Anmelde-Knopf reagiert in Safari nicht — Größe, Straße, Übergrößenträger`,
      `passage: 日本語のテキストと中文混合 🙂👍🏽 emoji\n\n- [ ] list **bold** \`code\` https://example.com/a?b=c#d`,
      `passage: ${`naïve café ﬁ ½ ①  ​ tab\there `.repeat(40)}`,
      `passage: xqzvw𝔘𝔫𝔦𝔠𝔬𝔡𝔢 � \u0000 ‮rtl Ελληνικά العربية עברית हिन्दी ไทย`,
      `passage: fn main() { let x: Vec<u8> = vec![0xff; 16]; } // #EXP-42 @mia@example.com`,
      ``,
      `   `,
    ]
    for (const sample of samples) {
      expect(compact.encode(sample).ids, sample.slice(0, 40)).toEqual(stock.encode(sample).ids)
    }
  }, 60_000)

  it(`cuts to the model's 512 positions and keeps </s> last`, () => {
    const compact = buildTokenizer(JSON.parse(readFileSync(tokenizerPath, `utf8`)), { maxTokens: 512 })
    const ids = encodeForModel(compact, `passage: ${`the session list reorders itself `.repeat(200)}`, 512)
    expect(ids).toHaveLength(512)
    expect(ids[0]).toBe(0)
    expect(ids[511]).toBe(2)
    expect(encodeForModel(compact, `short`, 512).length).toBeLessThan(8)
  }, 60_000)
})

describe.skipIf(!present)(`the embedder (worker thread, real model)`, () => {
  afterAll(() => stopEmbedder())

  it(`embeds to 384 L2-normalised dimensions, lazily, in a worker`, async () => {
    expect(embedderConfigured()).toBe(true)
    expect(embedderStats().loaded).toBe(false)
    const vector = await embedText(`passage`, `Login button does nothing on Safari`)
    expect(vector).toBeInstanceOf(Float32Array)
    expect(vector).toHaveLength(embeddingModel.dimensions)
    expect(cosine(vector!, vector!)).toBeCloseTo(1, 4)
    expect(embedderStats()).toMatchObject({ loaded: true, loads: 1, embeds: 1 })
  }, 120_000)

  it(`is deterministic, and a very long input still embeds (cut at 512 tokens)`, async () => {
    const [a, b, long] = await Promise.all([
      embedText(`passage`, `Der Anmelde-Knopf reagiert nicht`),
      embedText(`passage`, `Der Anmelde-Knopf reagiert nicht`),
      embedText(`passage`, `word `.repeat(5000)),
    ])
    expect(Array.from(a!)).toEqual(Array.from(b!))
    expect(long).toHaveLength(embeddingModel.dimensions)
  }, 120_000)

  it(`a German and an English text about the same bug are nearer than unrelated ones`, async () => {
    const [en, de, other, query] = await Promise.all([
      embedText(`passage`, `Login button does nothing on Safari`),
      embedText(`passage`, `Der Anmelde-Knopf reagiert in Safari nicht`),
      embedText(`passage`, `Export the board as a CSV file`),
      embedQuery(`  Anmeldung   funktioniert nicht im Browser `),
    ])
    expect(cosine(en!, de!)).toBeGreaterThan(cosine(en!, other!) + 0.05)
    expect(cosine(query!, de!)).toBeGreaterThan(cosine(query!, other!) + 0.05)
  }, 120_000)

  it(`a search query overtakes the queued passages`, async () => {
    const order: string[] = []
    const passages = Array.from({ length: 6 }, (_, i) =>
      embedText(`passage`, `background issue ${i} ${`text `.repeat(200)}`).then(() => order.push(`passage`))
    )
    const query = embedQuery(`find me first`).then(() => order.push(`query`))
    await Promise.all([...passages, query])
    // One passage was already running; the query ran before the rest.
    expect(order.indexOf(`query`)).toBeLessThanOrEqual(1)
  }, 120_000)

  it(`an empty query is no query`, async () => {
    expect(await embedQuery(`   `)).toBeNull()
  })

  it(`ISSUE_EMBEDDINGS=off answers null without touching the model`, async () => {
    process.env.ISSUE_EMBEDDINGS = `off`
    try {
      expect(embedderConfigured()).toBe(false)
      expect(await embedText(`passage`, `anything`)).toBeNull()
      expect(await embedQuery(`anything`)).toBeNull()
    } finally {
      delete process.env.ISSUE_EMBEDDINGS
    }
  })
})
