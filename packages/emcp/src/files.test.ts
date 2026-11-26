// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/files.json" with { type: "json" }
import { downloadMeta, fileInputSchema, fileInputSpec, filesDeclared, filesEnabled, planFiles, retryVerdict, stagingKeys, uploadCapabilityError, uploadRequired, type FileInput } from "./files"
import { argsDigest, sealRequestState } from "./request-state"

/** `{repeat, times}` = that string repeated; `{concat: [...]}` = the parts joined. */
function expand(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(expand)
  if (value && typeof value === `object`) {
    const v = value as Record<string, unknown>
    if (typeof v.repeat === `string` && typeof v.times === `number`) return v.repeat.repeat(v.times)
    if (Array.isArray(v.concat)) return v.concat.map(expand).join(``)
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, expand(x)]))
  }
  return value
}

describe(`flag and declaration`, () => {
  for (const entry of fixture.flagCases) test(JSON.stringify(entry.env), () => expect(filesEnabled(entry.env)).toBe(entry.expected))
  for (const entry of fixture.specCases) test(`${entry.tool} ${entry.op}`, () => expect(fileInputSpec(entry.tool, entry.op)).toEqual(entry.expected as never))
  for (const entry of fixture.capabilityCases) {
    test(entry.name, () => expect(uploadCapabilityError(entry.files as FileInput[], entry.declaredUpload)).toEqual(entry.expected as never))
  }
})

describe(`planFiles`, () => {
  const maxSize = fixture.fileInputs.exponential_attachments_write.maxSize
  for (const entry of fixture.planCases) {
    test(entry.name, () => {
      const plan = planFiles(expand(entry.files) as unknown[], maxSize)
      const view = plan.kind === `run` ? { kind: `run`, inline: plan.inline.map((f) => ({ id: f.id, size: f.bytes.length })) } : plan
      expect(view).toEqual(expand(entry.expected) as never)
    })
  }
})

describe(`upload_required`, () => {
  for (const entry of fixture.uploadRequiredCases) {
    test(entry.name, () => expect(uploadRequired(entry.files as FileInput[], entry.staged, entry.appUrl, entry.expiresAt, entry.requestState)).toEqual(entry.expected as never))
  }
})

describe(`retry`, () => {
  const g = fixture.retryGiven
  const seal = (kind: `upload` | `elicit`) =>
    sealRequestState(g.key, { principal: g.principal, tool: g.tool, args: argsDigest(g.arguments), kind, keys: stagingKeys(g.staged), exp: g.exp, nonce: `n-1` })
  for (const entry of fixture.retryCases) {
    test(entry.name, () => {
      const e = entry as typeof entry & { changeArgs?: object; principal?: string; now?: number }
      const token =
        entry.state === `none` ? undefined : entry.state === `elicit` ? seal(`elicit`) : entry.state === `tampered` ? `${seal(`upload`).slice(0, -2)}AA` : seal(`upload`)
      const args = { ...g.arguments, ...(e.changeArgs ?? {}) }
      const verdict = retryVerdict(
        token,
        { key: g.key, principal: e.principal ?? g.principal, tool: g.tool, args, now: e.now ?? g.now, consumed: new Set(entry.state === `consumed` ? [`n-1`] : []) },
        [args.file as FileInput],
        () => entry.object
      )
      expect(verdict).toEqual(entry.expected as never)
    })
  }
})

describe(`schema, declaration, downloads (#NEW-149)`, () => {
  for (const entry of fixture.schemaCases) test(`${entry.tool} ${entry.op}`, () => expect(fileInputSchema(entry.tool, entry.op)).toEqual(entry.expected as never))
  for (const entry of fixture.declaredCases) test(entry.name, () => expect(filesDeclared(entry.extensions)).toEqual(entry.expected))
  for (const entry of fixture.downloadCases) test(entry.name, () => expect(downloadMeta(entry.attachment, entry.declaredDownload)).toEqual(entry.expected as never))
})
