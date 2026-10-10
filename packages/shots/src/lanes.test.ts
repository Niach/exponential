/**
 * EXP-1267: the lane scheduler. These pin what the capture run relies on:
 * independent lanes overlap, a shared key serializes in declaration order,
 * readers overlap each other but never a writer, `--jobs` caps it, a failure
 * stays in its lane, and a failed gate fails its lanes without running them.
 */
import { describe, expect, test } from "bun:test"
import {
  describeSchedule,
  parseJobs,
  prefixLines,
  rerunLines,
  runLanes,
  UNLIMITED,
  type LaneSpec,
  type ResourceClaim,
} from "./lanes.ts"

const tick = () => new Promise((resolve) => setTimeout(resolve, 5))

/** Lanes that record a start/end timeline instead of doing work. */
function recorder() {
  const events: string[] = []
  let live = 0
  let peak = 0
  const lane = (
    id: string,
    resources: ResourceClaim[] = [],
    extra: Partial<LaneSpec> & { fail?: boolean } = {}
  ): LaneSpec => ({
    id,
    summary: id,
    resources,
    ...extra,
    run: async () => {
      events.push(`+${id}`)
      live++
      peak = Math.max(peak, live)
      await tick()
      live--
      events.push(`-${id}`)
      if (extra.fail) throw new Error(`${id} broke`)
    },
  })
  return { events, lane, peak: () => peak }
}

const ex = (key: ResourceClaim[`key`]): ResourceClaim => ({ key, mode: `exclusive` })
const sh = (key: ResourceClaim[`key`]): ResourceClaim => ({ key, mode: `shared` })

describe(`runLanes`, () => {
  test(`independent lanes all run at once`, async () => {
    const r = recorder()
    const runs = await runLanes([r.lane(`web`), r.lane(`desktop`), r.lane(`ios`), r.lane(`android`)])
    expect(r.peak()).toBe(4)
    expect(runs.map((run) => run.ok)).toEqual([true, true, true, true])
  })

  test(`an exclusive key serializes, in declaration order`, async () => {
    const r = recorder()
    await runLanes([
      r.lane(`ios:package`, [ex(`ios-simulator`)]),
      r.lane(`web`),
      r.lane(`ios:store`, [ex(`ios-simulator`)]),
      r.lane(`ios:styleguide`, [ex(`ios-simulator`)]),
    ])
    const ios = r.events.filter((event) => event.includes(`ios`))
    expect(ios).toEqual([`+ios:package`, `-ios:package`, `+ios:store`, `-ios:store`, `+ios:styleguide`, `-ios:styleguide`])
    // …while web overlapped the chain.
    expect(r.events.indexOf(`+web`)).toBeLessThan(r.events.indexOf(`-ios:package`))
  })

  test(`shared holders overlap each other but never the exclusive one`, async () => {
    const r = recorder()
    await runLanes([
      r.lane(`desktop`, [ex(`fleet`)]),
      r.lane(`web:fleet`, [sh(`fleet`)]),
      r.lane(`ios:styleguide`, [sh(`fleet`)]),
    ])
    expect(r.events.slice(0, 2)).toEqual([`+desktop`, `-desktop`])
    expect(r.events.slice(2, 4).sort()).toEqual([`+ios:styleguide`, `+web:fleet`])
  })

  test(`a blocked lane holds its place: a later lane cannot overtake it on the same key`, async () => {
    const r = recorder()
    await runLanes([
      r.lane(`web`, [ex(`capture-views`)]),
      r.lane(`web:fleet`, [ex(`capture-views`), sh(`fleet`)]),
      // Declared after web:fleet, which is waiting on capture-views and claims
      // `fleet` meanwhile — desktop must not slip in first.
      r.lane(`desktop`, [ex(`fleet`)]),
    ])
    expect(r.events.indexOf(`+desktop`)).toBeGreaterThan(r.events.indexOf(`-web:fleet`))
  })

  test(`--jobs caps the overlap`, async () => {
    const r = recorder()
    await runLanes([r.lane(`a`), r.lane(`b`), r.lane(`c`), r.lane(`d`)], { jobs: 2 })
    expect(r.peak()).toBe(2)
  })

  test(`--serial runs in declaration order`, async () => {
    const r = recorder()
    await runLanes([r.lane(`a`), r.lane(`b`), r.lane(`c`)], { jobs: 1 })
    expect(r.events).toEqual([`+a`, `-a`, `+b`, `-b`, `+c`, `-c`])
  })

  test(`one lane's failure leaves the others running, and is reported`, async () => {
    const r = recorder()
    const runs = await runLanes([
      r.lane(`android`, [ex(`adb-device`)], { fail: true }),
      r.lane(`android:store`, [ex(`adb-device`)]),
      r.lane(`web`),
    ])
    expect(runs.map((run) => [run.id, run.ok])).toEqual([
      [`android`, false],
      [`android:store`, true],
      [`web`, true],
    ])
    expect(runs[0]!.error).toBe(`android broke`)
    expect(runs[0]!.started).toBe(true)
  })

  test(`a gate holds its lanes until it opens; ungated lanes start at once`, async () => {
    const r = recorder()
    let open: () => void = () => {}
    const gate = new Promise<void>((resolve) => {
      open = resolve
    })
    const done = runLanes([r.lane(`ios:package`), r.lane(`web`, [], { gate })])
    await tick()
    await tick()
    expect(r.events).toEqual([`+ios:package`, `-ios:package`])
    open()
    const runs = await done
    expect(runs.every((run) => run.ok)).toBe(true)
    expect(r.events).toContain(`+web`)
  })

  test(`a failed gate fails its lanes unrun and the rest still finish`, async () => {
    const r = recorder()
    const gate = Promise.reject(new Error(`seeding failed`))
    const runs = await runLanes([
      r.lane(`web`, [], { gate, gateLabel: `seed` }),
      r.lane(`android:package`, [ex(`adb-device`)]),
    ])
    expect(r.events).toEqual([`+android:package`, `-android:package`])
    expect(runs[0]).toMatchObject({ ok: false, started: false })
    expect(runs[0]!.error).toContain(`seed failed: seeding failed`)
    expect(runs[1]!.ok).toBe(true)
  })

  test(`no lanes resolves at once`, async () => {
    expect(await runLanes([])).toEqual([])
  })
})

describe(`parseJobs`, () => {
  test(`defaults to unlimited`, () => expect(parseJobs([])).toBe(UNLIMITED))
  test(`--serial is one`, () => expect(parseJobs([`--serial`])).toBe(1))
  test(`--jobs N`, () => expect(parseJobs([`--platform`, `web`, `--jobs`, `3`])).toBe(3))
  test(`--jobs needs a positive integer`, () => {
    expect(() => parseJobs([`--jobs`])).toThrow(/positive integer/)
    expect(() => parseJobs([`--jobs`, `0`])).toThrow(/positive integer/)
    expect(() => parseJobs([`--jobs`, `two`])).toThrow(/positive integer/)
  })
})

describe(`describeSchedule`, () => {
  test(`says what each lane waits for, and why`, () => {
    const noop = async () => {}
    const lanes: LaneSpec[] = [
      { id: `ios:package`, summary: `pkg`, resources: [ex(`ios-simulator`)], run: noop },
      { id: `desktop`, summary: `desk`, resources: [ex(`screen`), ex(`fleet`)], gateLabel: `seed`, run: noop },
      { id: `web`, summary: `web`, resources: [ex(`capture-views`)], gateLabel: `seed`, run: noop },
      { id: `ios:store`, summary: `store`, resources: [ex(`ios-simulator`), sh(`fleet`)], gateLabel: `seed`, run: noop },
    ]
    const text = describeSchedule(lanes, UNLIMITED).join(`\n`)
    expect(text).toContain(`jobs: unlimited`)
    expect(text).toMatch(/ios:package\s+pkg\n\s+→ starts at once/)
    expect(text).toMatch(/web\s+web\n\s+→ after seed\n/)
    expect(text).toContain(`→ after seed · after ios:package (ios-simulator) · after desktop (fleet)`)
    expect(text).toContain(`fleet          the desktop lane registers its own machine`)
  })
})

describe(`rerunLines`, () => {
  test(`one line per platform, only the failed views, deduped`, () => {
    const lines = rerunLines(
      new Map([
        [`web`, [`board`, `inbox`, `board`]],
        [`ios`, []],
        [`desktop`, [`agents`]],
      ]),
      [`--repos-root`, `/r`]
    )
    expect(lines).toEqual([
      `bun run shots -- --platform web --views board,inbox --repos-root /r`,
      `bun run shots -- --platform desktop --views agents --repos-root /r`,
    ])
  })
})

describe(`prefixLines`, () => {
  test(`tags untagged lines, keeps tagged and blank ones`, () => {
    expect(prefixLines(`ios:package`, `\n── ios ──\n[ios:package] build\n  ok    x`)).toBe(
      `\n[ios:package] ── ios ──\n[ios:package] build\n[ios:package]   ok    x`
    )
  })
})
