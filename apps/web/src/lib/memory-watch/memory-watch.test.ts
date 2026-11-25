// @vitest-environment node
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { MemoryAlertState, REPEAT_MS, memoryAlerts } from "./evaluate"
import { alertRecipients, createMemoryWatch, memoryWatchEnabled, thresholdsFrom } from "./index"
import { readMemory, type MemoryReading } from "./readings"

const GB = 1024 ** 3
const healthy: MemoryReading = {
  host: { totalBytes: 8 * GB, availableBytes: 4 * GB, swapTotalBytes: 4 * GB, swapFreeBytes: 3 * GB },
  containers: [{ name: `aaaaaaaaaaaa`, usedBytes: GB, limitBytes: 2 * GB, oomKills: 0 }],
}
const low = (): MemoryReading => ({ ...healthy, host: { ...healthy.host!, availableBytes: 0.5 * GB } })

let dir: string
beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), `memory-watch-`))
})
afterEach(async () => {
  vi.restoreAllMocks()
  await rm(dir, { recursive: true, force: true })
})

async function cgroup(path: string, files: Record<string, string>) {
  await mkdir(join(dir, path), { recursive: true })
  for (const [name, text] of Object.entries(files)) await writeFile(join(dir, path, name), text)
}

describe(`readMemory`, () => {
  it(`reads the host and every Docker container, without the file cache`, async () => {
    await writeFile(join(dir, `meminfo`), `MemTotal:        7943168 kB\nMemFree:  100 kB\nMemAvailable:    3826688 kB\nSwapTotal:       4194300 kB\nSwapFree:         683008 kB\n`)
    await cgroup(`system.slice/docker-${`a1`.repeat(32)}.scope`, {
      "memory.current": `1000\n`, "memory.max": `2000\n`, "memory.stat": `anon 400\ninactive_file 250\nactive_file 50\n`, "memory.events": `oom 3\noom_kill 2\noom_group_kill 0\n`,
    })
    await cgroup(`docker/${`b2`.repeat(32)}`, { "memory.current": `5\n`, "memory.max": `max\n` })
    await cgroup(`system.slice/ssh.service`, { "memory.current": `5\n`, "memory.max": `6\n` })
    const reading = await readMemory({ meminfo: join(dir, `meminfo`), cgroupRoot: dir })
    expect(reading.host).toEqual({ totalBytes: 7943168 * 1024, availableBytes: 3826688 * 1024, swapTotalBytes: 4194300 * 1024, swapFreeBytes: 683008 * 1024 })
    expect(reading.containers).toEqual([
      { name: `a1a1a1a1a1a1`, usedBytes: 700, limitBytes: 2000, oomKills: 2 },
      { name: `b2b2b2b2b2b2`, usedBytes: 5, limitBytes: null, oomKills: 0 },
    ])
  })

  it(`falls back to this container, and to nothing where there is no cgroup v2`, async () => {
    await cgroup(`own`, { "memory.current": `10\n`, "memory.max": `20\n` })
    const own = await readMemory({ meminfo: join(dir, `nope`), ownCgroup: join(dir, `own`) })
    expect(own).toEqual({ host: null, containers: [{ name: `this web container`, usedBytes: 10, limitBytes: 20, oomKills: 0 }] })
    expect(await readMemory({ meminfo: join(dir, `nope`), ownCgroup: join(dir, `nope`) })).toEqual({ host: null, containers: [] })
  })
})

describe(`memoryAlerts`, () => {
  it(`is quiet on a healthy host, and without a limit or swap`, () => {
    expect(memoryAlerts(healthy)).toEqual([])
    expect(memoryAlerts({ host: { ...healthy.host!, swapTotalBytes: 0, swapFreeBytes: 0 }, containers: [{ ...healthy.containers[0]!, usedBytes: 99 * GB, limitBytes: null }] })).toEqual([])
    expect(memoryAlerts({ host: null, containers: [] })).toEqual([])
  })

  it(`names low memory, full swap and a container at its limit`, () => {
    // The shepherd's reading of 2026-11-25: swap 3,430 of 4,095 MB used.
    const keys = memoryAlerts({
      host: { totalBytes: 7757, availableBytes: 900, swapTotalBytes: 4095, swapFreeBytes: 600 },
      containers: [{ name: `c`, usedBytes: 1.9 * GB, limitBytes: 2 * GB, oomKills: 0 }],
    }).map((alert) => alert.key)
    expect(keys).toEqual([`host-memory`, `host-swap`, `container:c`])
  })
})

describe(`MemoryAlertState`, () => {
  it(`alerts on the third reading in a row, once, and again after a day`, () => {
    const state = new MemoryAlertState()
    expect(state.next(low(), 0)).toEqual([])
    expect(state.next(low(), 1)).toEqual([])
    expect(state.next(low(), 2).map((a) => a.key)).toEqual([`host-memory`])
    expect(state.next(low(), 3)).toEqual([])
    expect(state.next(low(), 2 + REPEAT_MS)).toHaveLength(1)
  })

  it(`a recovery starts over`, () => {
    const state = new MemoryAlertState()
    state.next(low(), 0)
    state.next(low(), 1)
    expect(state.next(healthy, 2)).toEqual([])
    expect(state.next(low(), 3)).toEqual([])
    state.next(low(), 4)
    expect(state.next(low(), 5)).toHaveLength(1)
  })

  it(`an OOM kill is said at once, one before the start or of a new container's zero never`, () => {
    const killed = (n: number): MemoryReading => ({ ...healthy, containers: [{ ...healthy.containers[0]!, oomKills: n }] })
    const state = new MemoryAlertState()
    expect(state.next(killed(4), 0)).toEqual([])
    expect(state.next(killed(4), 1)).toEqual([])
    expect(state.next(killed(5), 2).map((a) => a.key)).toEqual([`oom:aaaaaaaaaaaa`])
    expect(state.next(killed(5), 3)).toEqual([])
  })
})

describe(`the watch`, () => {
  it(`an injected reading reaches diagnostics and every mailbox, a failed mail notwithstanding`, async () => {
    vi.spyOn(console, `error`).mockImplementation(() => {})
    await writeFile(join(dir, `meminfo`), `MemTotal: 8000000 kB\nMemAvailable: 400000 kB\nSwapTotal: 4000000 kB\nSwapFree: 4000000 kB\n`)
    const report = vi.fn()
    const mail = vi.fn().mockRejectedValueOnce(new Error(`smtp down`)).mockResolvedValue(undefined)
    const watch = createMemoryWatch({
      env: { MEMORY_ALERT_EMAIL: `a@example.com, b@example.com`, BETTER_AUTH_URL: `https://app.example.com` },
      read: () => readMemory({ meminfo: join(dir, `meminfo`), ownCgroup: join(dir, `nope`) }),
      report,
      mail,
    })
    await watch.tick()
    await watch.tick()
    expect(mail).not.toHaveBeenCalled()
    await watch.tick()
    expect(report).toHaveBeenCalledExactlyOnceWith({ key: `host-memory`, text: `The host has 391 MB of 7813 MB memory available` })
    expect(mail.mock.calls.map((call) => call[0])).toEqual([`a@example.com`, `b@example.com`])
    expect(mail.mock.calls[1]![1]).toBe(`Memory alert: The host has 391 MB of 7813 MB memory available`)
    await watch.tick()
    expect(mail).toHaveBeenCalledTimes(2)
  })

  it(`a threshold of 100 fires on real readings (the rollout's proof), bad values keep the default`, () => {
    expect(memoryAlerts(healthy, thresholdsFrom({ MEMORY_WATCH_HOST_AVAILABLE_PERCENT: `100` })).map((a) => a.key)).toEqual([`host-memory`])
    expect(thresholdsFrom({ MEMORY_WATCH_HOST_AVAILABLE_PERCENT: `abc`, MEMORY_WATCH_SWAP_FREE_PERCENT: `` })).toEqual(thresholdsFrom({}))
  })

  it(`is off by default`, () => {
    expect(memoryWatchEnabled({})).toBe(false)
    expect(memoryWatchEnabled({ MEMORY_WATCH: `on` })).toBe(true)
    expect(alertRecipients({})).toEqual([])
  })
})
