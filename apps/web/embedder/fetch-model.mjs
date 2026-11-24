// #NEW-134: fetches the pinned embedding model into `embeddingModelDir()` and
// verifies every file's sha256. Runs at image build (Dockerfile), in CI and
// once per checkout (`bun run embeddings:model`); NEVER at request time.
// Idempotent: a file whose hash already matches is kept.
//
//   bun embedder/fetch-model.mjs [--check]   (node works too)
import { createHash } from "node:crypto"
import { createReadStream, existsSync } from "node:fs"
import { mkdir, rename, rm, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { EMBEDDING_MODEL_SOURCE, embeddingModelDir } from "./model.mjs"

const checkOnly = process.argv.includes(`--check`)

function sha256File(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash(`sha256`)
    createReadStream(path)
      .on(`data`, (chunk) => hash.update(chunk))
      .on(`end`, () => resolve(hash.digest(`hex`)))
      .on(`error`, reject)
  })
}

const dir = embeddingModelDir()
await mkdir(dir, { recursive: true })
for (const file of EMBEDDING_MODEL_SOURCE.files) {
  const target = join(dir, file.name)
  if (existsSync(target) && (await sha256File(target)) === file.sha256) {
    console.log(`ok       ${file.name}`)
    continue
  }
  if (checkOnly) {
    console.error(`missing or wrong hash: ${target}`)
    process.exit(1)
  }
  const url = `https://huggingface.co/${EMBEDDING_MODEL_SOURCE.repo}/resolve/${EMBEDDING_MODEL_SOURCE.revision}/${file.path}`
  console.log(`fetching ${file.name} (${Math.round(file.bytes / 1e6)} MB)`)
  const response = await fetch(url, { redirect: `follow` })
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`)
  const bytes = Buffer.from(await response.arrayBuffer())
  const sha256 = createHash(`sha256`).update(bytes).digest(`hex`)
  if (sha256 !== file.sha256) {
    throw new Error(`${file.name}: sha256 ${sha256}, expected ${file.sha256}`)
  }
  const partial = `${target}.partial`
  await writeFile(partial, bytes)
  await rm(target, { force: true })
  await rename(partial, target)
  console.log(`verified ${file.name}`)
}
