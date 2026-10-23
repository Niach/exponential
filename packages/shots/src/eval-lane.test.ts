import { describe, expect, test } from "bun:test"
import { corpusIds, evalPlatforms, evalScope, evalShotPath, evalSpecimenId, isEvalView, specimenIds } from "./eval-lane.ts"

describe(`the eval lane (VAPP-127)`, () => {
  const ids = [`login`, `chat`, `kanban`]

  test(`--views without an eval entry is not an eval run`, () => {
    expect(evalScope(undefined, ids)).toBeNull()
    expect(evalScope([`board`, `agents`], ids)).toBeNull()
    expect(isEvalView(`evaluation`)).toBe(false)
  })

  test(`eval = every surface, eval:<id> = the named ones in corpus order`, () => {
    expect(evalScope([`eval`], ids)).toEqual(ids)
    expect(evalScope([`eval:kanban`, `eval:login`], ids)).toEqual([`login`, `kanban`])
  })

  test(`an unknown surface or a catalog view beside an eval one is refused`, () => {
    expect(() => evalScope([`eval:nope`], ids)).toThrow(/unknown eval surface/)
    expect(() => evalScope([`eval:login`, `board`], ids)).toThrow(/mixes/)
  })

  test(`the web is eval:render's, never this lane's`, () => {
    expect(evalPlatforms([`web`, `android`, `desktop`])).toEqual([`android`, `desktop`])
  })

  test(`every corpus surface has a specimen the hosts can paint, and lands beside the web renders`, () => {
    expect(specimenIds()).toEqual(corpusIds().map(evalSpecimenId))
    expect(evalShotPath(`login`, `android`)).toEndWith(`packages/exponential-ui/eval/out/login/android.webp`)
  })
})
