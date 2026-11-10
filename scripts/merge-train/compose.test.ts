import { afterAll, describe, expect, it } from "bun:test"
import { mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync, existsSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import type { TrainRepoConfig } from "../../packages/domain-contract/src/merge-train"
import { rustChangelog, webChangelog } from "./changelog-shapes"
import { compose } from "./compose"
import { Git, sh } from "./git"

// EXP-1708: the engine against real git repos (no forge, no network).

const dirs: string[] = []
afterAll(() => dirs.forEach((d) => rmSync(d, { recursive: true, force: true })))

function repo(files: Record<string, string>, config?: TrainRepoConfig) {
  const dir = mkdtempSync(join(tmpdir(), `train-test-`))
  dirs.push(dir)
  const git = new Git(dir)
  git.must(`init`, `--quiet`, `-b`, `master`)
  git.must(`config`, `user.email`, `train@example.com`)
  git.must(`config`, `user.name`, `train`)
  git.must(`config`, `commit.gpgsign`, `false`)
  const write = (f: Record<string, string | null>) => {
    for (const [p, body] of Object.entries(f)) {
      const abs = join(dir, p)
      if (body === null) rmSync(abs, { force: true })
      else {
        mkdirSync(dirname(abs), { recursive: true })
        writeFileSync(abs, body)
      }
    }
  }
  write({ ...files, ...(config ? { ".exponential/merge-train.json": JSON.stringify(config) } : {}) })
  git.must(`add`, `-A`)
  git.must(`commit`, `--quiet`, `-m`, `base`)
  const base = git.must(`rev-parse`, `HEAD`)
  /** A PR branch off `from` (default the base): → its head sha. */
  const branch = (f: Record<string, string | null>, from = base) => {
    git.must(`checkout`, `--quiet`, `--detach`, from)
    write(f)
    git.must(`add`, `-A`)
    git.must(`commit`, `--quiet`, `-m`, `wip`)
    const sha = git.must(`rev-parse`, `HEAD`)
    git.must(`checkout`, `--quiet`, `--detach`, base)
    return sha
  }
  const read = (p: string) => readFileSync(join(dir, p), `utf8`)
  return { dir, git, base, branch, read, has: (p: string) => existsSync(join(dir, p)) }
}

const pr = (number: number, head_sha: string) => ({ number, head_sha, title: `EXP-${number}: change` })

const CHANGELOG = `export const CHANGELOG = [\n  { v: 1 },\n]\n`
const POLICY: TrainRepoConfig = {
  ci: { workflows: [`ci.yml`] },
  generated: [
    { name: `changelog`, paths: [`changelog.ts`, `changelog.rs`], strategy: `prepend-union` },
    { name: `bindings`, paths: [`gen/**`], strategy: `regenerate`, command: `cp src/enum.txt gen/enum.txt` },
    { name: `migrations`, paths: [`db/*.sql`, `db/meta/**`], strategy: `renumber`, command: `bash gen-migration.sh`, verify: [`db/*.sql`] },
    { name: `index`, paths: [`shots/index.json`], strategy: `json-union` },
    { name: `shots`, paths: [`shots/**/*.webp`], strategy: `take-later` },
  ],
}

describe(`compose: a sibling in the way is no bounce (#EXP-1711)`, () => {
  it(`a unit that conflicts only behind an earlier unit of the batch reports against: batch`, () => {
    const r = repo({ "a.txt": `a\n` }, POLICY)
    const first = r.branch({ "a.txt": `first\n` })
    const second = r.branch({ "a.txt": `second\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `s1`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, first)] }, { prs: [pr(2, second)] }] })
    expect(outcome.units.map((u) => [u.outcome, u.against ?? null])).toEqual([[`composed`, null], [`conflict`, `batch`]])
    expect(r.read(`a.txt`)).toBe(`first\n`)
  })

  it(`a unit that conflicts with the base itself stays a plain conflict`, () => {
    const r = repo({ "a.txt": `a\n`, "c.txt": `c\n` }, POLICY)
    const stale = r.branch({ "a.txt": `stale\n` })
    const moved = r.branch({ "a.txt": `moved\n` })
    const other = r.branch({ "c.txt": `c1\n` }, moved)
    const { outcome } = compose({ dir: r.dir, trainId: `s2`, baseSha: moved, mode: `test`, units: [{ prs: [pr(3, other)] }, { prs: [pr(4, stale)] }] })
    expect(outcome.units.map((u) => [u.outcome, u.against ?? null])).toEqual([[`composed`, null], [`conflict`, null]])
  })
})

describe(`compose: changelogs prepended at one spot (#LOGS-2)`, () => {
  const web = (ids: string[]) => webChangelog(ids)
  const rs = (id: string) => rustChangelog(id, [[233, `a`], [232, `z`]])
  const files = (id: string) => ({ "changelog.ts": web([id, `a`]), "changelog.rs": rs(id) })
  const BASE = { "changelog.ts": web([`a`]), "changelog.rs": rustChangelog(`a`, [[232, `z`]]) }

  it(`three units = three whole entries, newest first, the mirror renumbered`, () => {
    const r = repo(BASE, POLICY)
    const [b, c, d] = [`b`, `c`, `d`].map((id) => r.branch(files(id)))
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, b!)] }, { prs: [pr(2, c!)] }, { prs: [pr(3, d!)] }] })
    expect(outcome.units.map((u) => [u.outcome, u.resolved])).toEqual([[`composed`, []], [`composed`, [`changelog`]], [`composed`, [`changelog`]]])
    expect(r.read(`changelog.ts`)).toBe(web([`d`, `c`, `b`, `a`]))
    expect(r.read(`changelog.rs`)).toBe(rustChangelog(`d`, [[235, `c`], [234, `b`], [233, `a`], [232, `z`]]))
    // the old resolver, a line union, fused them into one object
    const dir = mkdtempSync(join(tmpdir(), `union-`))
    dirs.push(dir)
    for (const [n, body] of Object.entries({ base: web([`a`]), ours: web([`b`, `a`]), theirs: web([`c`, `a`]) })) writeFileSync(join(dir, n), body)
    const fused = sh(dir, [`git`, `merge-file`, `-p`, `--union`, `theirs`, `base`, `ours`]).out
    expect(fused).toContain(`    title: \`C\`,\n    id: \`b\`,`)
  })

  it(`an edit beside the prepend bounces the unit instead of a line union`, () => {
    const r = repo(BASE, POLICY)
    const b = r.branch(files(`b`))
    const c = r.branch({ "changelog.ts": web([`c`, `a`]).replace(`title: \`A\``, `title: \`A2\``) })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, b)] }, { prs: [pr(2, c)] }] })
    expect(outcome.units[1]).toMatchObject({ outcome: `conflict`, files: [`changelog.ts`] })
    expect(r.read(`changelog.ts`)).toBe(web([`b`, `a`]))
  })

  it(`a Merge now that had to resolve a conflict runs CI; a clean one stays instant`, () => {
    const r = repo(BASE, POLICY)
    const landed = r.branch(files(`b`))
    const c = r.branch(files(`c`))
    const resolved = compose({ dir: r.dir, trainId: `t`, baseSha: landed, mode: `untested`, units: [{ prs: [pr(2, c)] }] })
    expect(resolved.outcome.units[0]!.resolved).toEqual([`changelog`])
    expect([resolved.outcome.checks, resolved.dispatch]).toEqual([`dispatched`, [`ci.yml`]])
    const clean = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `untested`, units: [{ prs: [pr(2, c)] }] })
    expect([clean.outcome.checks, clean.dispatch]).toEqual([`skipped`, []])
  })

  it(`2026-11-10: a Merge now composed on a base that moved past its PR runs CI; once rebased it stays instant`, () => {
    // #2590 was green on master B; #2597 (a new test #2590 fails) was
    // admin-merged (B → B'); #2590's Merge now composed cleanly on B' and
    // landed untested: master red.
    const r = repo({ "a.txt": `a\n`, "test.txt": `old\n` }, POLICY)
    const pr2590 = r.branch({ "a.txt": `a1\n` })
    const moved = r.branch({ "test.txt": `new test\n` })
    const onMoved = compose({ dir: r.dir, trainId: `t`, baseSha: moved, mode: `untested`, units: [{ prs: [pr(2590, pr2590)] }] })
    expect(onMoved.outcome.units[0]).toEqual({ prs: [2590], outcome: `composed`, files: [], resolved: [] })
    expect([onMoved.outcome.checks, onMoved.dispatch]).toEqual([`dispatched`, [`ci.yml`]])
    const rebased = r.branch({ "a.txt": `a1\n` }, moved)
    const onRebased = compose({ dir: r.dir, trainId: `t`, baseSha: moved, mode: `untested`, units: [{ prs: [pr(2590, rebased)] }] })
    expect([onRebased.outcome.checks, onRebased.dispatch]).toEqual([`skipped`, []])
  })
})

describe(`compose: a Merge now whose lockfile drifted runs CI (#LOGS-2)`, () => {
  const LOCKS: TrainRepoConfig = {
    ...POLICY,
    lockfiles: { paths: [`**/Cargo.lock`], manifests: [{ paths: [`**/Cargo.toml`], lock: `Cargo.lock` }] },
  }
  const lock = (top: string, bottom: string) => `${top}\n${[`b`, `c`, `d`, `e`, `f`, `g`].join(`\n`)}\n${bottom}\n`
  const BASE = { "ede/Cargo.lock": lock(`smithay 1`, `sentry 0`), "ede/crates/x/Cargo.toml": `[deps]\n` }

  it(`a clean lockfile merge onto a moved base = dispatched; onto an unmoved base = skipped`, () => {
    const r = repo(BASE, LOCKS)
    const moved = r.branch({ "ede/Cargo.lock": lock(`smithay git`, `sentry 0`) })
    const unit = r.branch({ "ede/Cargo.lock": lock(`smithay 1`, `sentry 1`) })
    const onMoved = compose({ dir: r.dir, trainId: `t`, baseSha: moved, mode: `untested`, units: [{ prs: [pr(2, unit)] }] })
    expect(onMoved.outcome.units[0]).toMatchObject({ outcome: `composed`, resolved: [], drifted: [`ede/Cargo.lock`] })
    expect([onMoved.outcome.checks, onMoved.dispatch]).toEqual([`dispatched`, [`ci.yml`]])
    const onBase = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `untested`, units: [{ prs: [pr(2, unit)] }] })
    expect(onBase.outcome.units[0]).toEqual({ prs: [2], outcome: `composed`, files: [], resolved: [] })
    expect([onBase.outcome.checks, onBase.dispatch]).toEqual([`skipped`, []])
  })

  it(`a manifest changed while the base moved its paired lock = dispatched`, () => {
    const r = repo(BASE, LOCKS)
    const moved = r.branch({ "ede/Cargo.lock": lock(`smithay git`, `sentry 0`) })
    const unit = r.branch({ "ede/crates/x/Cargo.toml": `[deps]\nsentry = "1"\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: moved, mode: `untested`, units: [{ prs: [pr(2, unit)] }] })
    expect([outcome.units[0]!.drifted, outcome.checks]).toEqual([[`ede/Cargo.lock`], `dispatched`])
    const onBase = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `untested`, units: [{ prs: [pr(2, unit)] }] })
    expect(onBase.outcome.checks).toBe(`skipped`)
  })
})

describe(`compose`, () => {
  it(`squashes every PR in order with its title, a stack bottom first`, () => {
    const r = repo({ "a.txt": `a\n`, "b.txt": `b\n` }, POLICY)
    const one = r.branch({ "a.txt": `a1\n` })
    const low = r.branch({ "b.txt": `b1\n` })
    const high = r.branch({ "c.txt": `c\n` }, low)
    const { outcome, dispatch } = compose({
      dir: r.dir,
      trainId: `t1`,
      baseSha: r.base,
      mode: `test`,
      units: [{ prs: [pr(1, one)] }, { prs: [pr(2, low), pr(3, high)] }],
    })
    expect(outcome.units.map((u) => u.outcome)).toEqual([`composed`, `composed`])
    expect(outcome.checks).toBe(`dispatched`)
    expect(dispatch).toEqual([`ci.yml`])
    expect(r.git.lines(`log`, `--format=%s`, `${r.base}..${outcome.head}`)).toEqual([
      `EXP-3: change (#3)`,
      `EXP-2: change (#2)`,
      `EXP-1: change (#1)`,
    ])
    expect(r.read(`c.txt`)).toBe(`c\n`)
  })

  it(`a changelog both prepended keeps both entries, the later unit's first`, () => {
    const r = repo({ "changelog.ts": CHANGELOG }, POLICY)
    const a = r.branch({ "changelog.ts": CHANGELOG.replace(`[\n`, `[\n  { v: 2 },\n`) })
    const b = r.branch({ "changelog.ts": CHANGELOG.replace(`[\n`, `[\n  { v: 3 },\n`) })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }] })
    expect(outcome.units[1]).toEqual({ prs: [2], outcome: `composed`, files: [], resolved: [`changelog`] })
    expect(r.read(`changelog.ts`)).toBe(`export const CHANGELOG = [\n  { v: 3 },\n  { v: 2 },\n  { v: 1 },\n]\n`)
  })

  it(`the shots index unions, the later .webp wins`, () => {
    const r = repo({ "shots/index.json": `{"a":1}\n`, "shots/v/web.webp": `0` }, POLICY)
    const a = r.branch({ "shots/index.json": `{"a":1,"b":1}\n`, "shots/v/web.webp": `A` })
    const b = r.branch({ "shots/index.json": `{"a":1,"c":1}\n`, "shots/v/web.webp": `B` })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }] })
    expect(outcome.units[1].resolved.sort()).toEqual([`index`, `shots`])
    expect(JSON.parse(r.read(`shots/index.json`))).toEqual({ a: 1, b: 1, c: 1 })
    expect(r.read(`shots/v/web.webp`)).toBe(`B`)
  })

  it(`generated bindings regenerate from the merged source`, () => {
    const r = repo({ "src/enum.txt": `a\nm\nz\n`, "gen/enum.txt": `a\nm\nz\n` }, POLICY)
    const a = r.branch({ "src/enum.txt": `a\nb\nm\nz\n`, "gen/enum.txt": `a\nb\nm\nz\n` })
    const b = r.branch({ "src/enum.txt": `a\nm\ny\nz\n`, "gen/enum.txt": `a\nm\nY\nz\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }] })
    expect(outcome.units[1].outcome).toBe(`composed`)
    expect(r.read(`gen/enum.txt`)).toBe(`a\nb\nm\ny\nz\n`)
  })

  it(`a regenerate that keeps the base's blob for a path the unit changed bounces it (#FEED-212)`, () => {
    // The lockfiles rule claimed both workspaces' locks but regenerated only
    // one: the other came out as master's, the unit's change silently gone.
    const LOCKS: TrainRepoConfig = {
      generated: [{ name: `lockfiles`, paths: [`desktop/Cargo.lock`, `ede/Cargo.lock`], strategy: `regenerate`, command: `cp desktop/want desktop/Cargo.lock` }],
    }
    const r = repo({ "desktop/want": `d0\n`, "desktop/Cargo.lock": `d0\n`, "ede/Cargo.lock": `e0\n` }, LOCKS)
    const moved = r.branch({ "ede/Cargo.lock": `e-master\n` })
    const unit = r.branch({ "ede/Cargo.lock": `e-unit\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: moved, mode: `test`, units: [{ prs: [pr(1, unit)] }] })
    expect(outcome.units[0]).toMatchObject({ outcome: `conflict`, files: [`ede/Cargo.lock`] })
    expect(outcome.head).not.toBe(moved)
    expect(r.read(`ede/Cargo.lock`)).toBe(`e-master\n`)
    // a regenerate that does rewrite it (here: to the unit's side) still composes
    const fixed: TrainRepoConfig = { generated: [{ ...LOCKS.generated[0]!, command: `git checkout ${unit} -- ede/Cargo.lock` }] }
    const ok = compose({ dir: r.dir, trainId: `t`, baseSha: moved, mode: `test`, units: [{ prs: [pr(1, unit)] }], config: fixed })
    expect(ok.outcome.units[0]).toMatchObject({ outcome: `composed`, resolved: [`lockfiles`] })
  })

  it(`any other conflict drops that whole unit, naming only the unmatched files, and the next unit still rides`, () => {
    const r = repo({ "src/x.ts": `1\n`, "gen/enum.txt": `0\n`, "y.ts": `y\n` }, POLICY)
    const a = r.branch({ "src/x.ts": `2\n`, "gen/enum.txt": `2\n` })
    const b = r.branch({ "src/x.ts": `3\n`, "gen/enum.txt": `3\n` })
    const c = r.branch({ "y.ts": `y2\n` })
    const { outcome } = compose({
      dir: r.dir,
      trainId: `t`,
      baseSha: r.base,
      mode: `test`,
      units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }, { prs: [pr(3, c)] }],
    })
    expect(outcome.units.map((u) => [u.outcome, u.files])).toEqual([
      [`composed`, []],
      [`conflict`, [`src/x.ts`]],
      [`composed`, []],
    ])
    expect(r.read(`src/x.ts`)).toBe(`2\n`)
    expect(r.git.lines(`log`, `--format=%s`, `${r.base}..${outcome.head}`)).toEqual([`EXP-3: change (#3)`, `EXP-1: change (#1)`])
  })

  it(`a failing command bounces the unit with the rule's paths`, () => {
    const r = repo({ "src/enum.txt": `a\n`, "gen/enum.txt": `a\n` }, POLICY)
    const a = r.branch({ "gen/enum.txt": `b\n` })
    const b = r.branch({ "gen/enum.txt": `c\n` })
    const { outcome } = compose({
      dir: r.dir,
      trainId: `t`,
      baseSha: r.base,
      mode: `test`,
      units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }],
      runCommand: () => false,
    })
    expect(outcome.units[1]).toEqual({ prs: [2], outcome: `conflict`, files: [`gen/enum.txt`], resolved: [], against: `batch` })
  })

  // A toy generator: the next migration = the schema lines no migration has yet.
  const GEN = `n=$(ls db/*.sql | wc -l); n=$(printf '%04d' $((n+1)))\nnew=$(grep -x 't[0-9]*' schema.txt | grep -vxF -f <(cat db/*.sql) || true)\n[ -z "$new" ] && exit 0\necho "$new" > db/\${n}_gen.sql\necho "$n" >> db/meta/journal.txt\n`
  const MIG = { "schema.txt": `t1\n1\n2\n3\nend\n`, "db/0001_init.sql": `t1\n`, "db/meta/journal.txt": `0001\n`, "gen-migration.sh": GEN }

  it(`migrations: the later unit's migration takes the next number`, () => {
    const r = repo(MIG, POLICY)
    const a = r.branch({ "schema.txt": `t1\nt2\n1\n2\n3\nend\n`, "db/0002_owl.sql": `t2\n`, "db/meta/journal.txt": `0001\n0002\n` })
    const b = r.branch({ "schema.txt": `t1\n1\n2\n3\nt3\nend\n`, "db/0002_cat.sql": `t3\n`, "db/meta/journal.txt": `0001\n0002\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }] })
    expect(outcome.units[1]).toEqual({ prs: [2], outcome: `composed`, files: [], resolved: [`migrations`] })
    expect(r.has(`db/0002_cat.sql`)).toBe(false)
    expect(r.read(`db/0003_gen.sql`)).toBe(`t3\n`)
    expect(r.read(`db/meta/journal.txt`)).toBe(`0001\n0002\n0003\n`)
  })

  it(`migrations: a hand-edited migration the command cannot regenerate bounces`, () => {
    const r = repo(MIG, POLICY)
    const a = r.branch({ "schema.txt": `t1\nt2\n1\n2\n3\nend\n`, "db/0002_owl.sql": `t2\n`, "db/meta/journal.txt": `0001\n0002\n` })
    const b = r.branch({ "schema.txt": `t1\n1\n2\n3\nt3\nend\n`, "db/0002_cat.sql": `t3\n-- backfill\n`, "db/meta/journal.txt": `0001\n0002\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }] })
    expect(outcome.units[1].outcome).toBe(`conflict`)
    expect(outcome.units[1].files).toEqual([`db/0002_cat.sql`, `db/meta/journal.txt`])
    expect(r.has(`db/0002_owl.sql`)).toBe(true)
  })

  it(`a lone unit tested on this base is proven; Merge now is untested`, () => {
    const r = repo({ "a.txt": `a\n` }, POLICY)
    const a = r.branch({ "a.txt": `a1\n` })
    const proven = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)], tested_sha: a }] })
    expect([proven.outcome.checks, proven.dispatch]).toEqual([`proven`, []])
    const stale = r.branch({ "a.txt": `a1\n`, "z.txt": `older base\n` })
    expect(compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)], tested_sha: stale }] }).outcome.checks).toBe(`dispatched`)
    expect(compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `untested`, units: [{ prs: [pr(1, a)] }] }).outcome.checks).toBe(`skipped`)
  })

  it(`nothing composed = one empty marker commit on the base, no CI`, () => {
    const r = repo({ "x.ts": `1\n` })
    const a = r.branch({ "x.ts": `2\n` })
    const b = r.branch({ "x.ts": `3\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `t9`, baseSha: a, mode: `test`, units: [{ prs: [pr(2, b)] }] })
    expect(outcome.checks).toBe(`none`)
    expect(r.git.must(`rev-parse`, `${outcome.head}^`)).toBe(a)
    expect(r.git.must(`log`, `-1`, `--format=%s`, outcome.head!)).toBe(`exp-train t9: nothing composed`)
  })

  it(`no repo config: no resolvers, no CI`, () => {
    const r = repo({ "a.txt": `a\n` })
    const a = r.branch({ "a.txt": `a1\n` })
    expect(compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)] }] }).outcome.checks).toBe(`none`)
  })

  it(`the base's policy rules, never a unit's`, () => {
    const r = repo({ "changelog.ts": CHANGELOG })
    const a = r.branch({ "changelog.ts": CHANGELOG.replace(`[\n`, `[\n  { v: 2 },\n`), ".exponential/merge-train.json": JSON.stringify(POLICY) })
    const b = r.branch({ "changelog.ts": CHANGELOG.replace(`[\n`, `[\n  { v: 3 },\n`) })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, a)] }, { prs: [pr(2, b)] }] })
    expect(outcome.units[1]).toEqual({ prs: [2], outcome: `conflict`, files: [`changelog.ts`], resolved: [], against: `batch` })
  })
})

void sh

describe(`compose: licence inventories regenerate on the composed tree (#LOGS-2)`, () => {
  // THIS repo's rules (paths, order, commands), each command stubbed by what
  // it does: lockfiles = bun.lock from the manifest, collect:npm = the
  // inventory from bun.lock, notices = NOTICES.txt from the inventory.
  const REPO = JSON.parse(readFileSync(join(import.meta.dirname, `../../.exponential/merge-train.json`), `utf8`)) as TrainRepoConfig
  const rule = (name: string) => REPO.generated.find((r) => r.name === name)!
  const INVENTORY = `packages/licenses/inventory/npm-web.json`
  const NOTICES = `apps/web/public/NOTICES.txt`
  const lockOf = (deps: string[]) => deps.map((d) => `${d}@1`).join(`\n`) + `\n`
  const inventoryOf = (lock: string) => `${JSON.stringify({ components: lock.split(`\n`).filter(Boolean) }, null, 2)}\n`
  const noticesOf = (inventory: string) => (JSON.parse(inventory) as { components: string[] }).components.map((c) => `== ${c}\n`).join(``)
  const files = (deps: string[]) => {
    const lock = lockOf(deps)
    const inventory = inventoryOf(lock)
    return { "apps/web/package.json": `${deps.join(`\n`)}\n`, "bun.lock": lock, [INVENTORY]: inventory, [NOTICES]: noticesOf(inventory) }
  }
  const ran: string[] = []
  const runCommand = (command: string, dir: string): boolean => {
    ran.push(command)
    const at = (p: string) => join(dir, p)
    if (command === REPO.setup) return true
    if (command === rule(`lockfiles`).command) {
      writeFileSync(at(`bun.lock`), lockOf(readFileSync(at(`apps/web/package.json`), `utf8`).split(`\n`).filter(Boolean)))
    } else if (command === rule(`npm licence inventory`).command) {
      writeFileSync(at(INVENTORY), inventoryOf(readFileSync(at(`bun.lock`), `utf8`)))
    } else if (command === rule(`licence notices`).command) {
      writeFileSync(at(NOTICES), noticesOf(readFileSync(at(INVENTORY), `utf8`)))
    } else return false
    return true
  }
  const BASE = [`a`, `m`, `z`]

  it(`two units that each add an npm dependency compose, the inventory and the notices carry both, CI runs`, () => {
    const r = repo(files(BASE), REPO)
    const one = r.branch(files([`a`, `b`, `m`, `z`]))
    const two = r.branch(files([`a`, `m`, `y`, `z`]))
    ran.length = 0
    const { outcome, dispatch } = compose({ dir: r.dir, trainId: `t`, baseSha: r.base, mode: `test`, units: [{ prs: [pr(1, one)] }, { prs: [pr(2, two)] }], runCommand })
    expect(outcome.units.map((u) => [u.outcome, u.files, u.resolved])).toEqual([
      [`composed`, [], []],
      [`composed`, [], [`lockfiles`, `npm licence inventory`, `licence notices`]],
    ])
    expect(r.read(INVENTORY)).toBe(inventoryOf(lockOf([`a`, `b`, `m`, `y`, `z`])))
    expect(r.read(NOTICES)).toBe(noticesOf(r.read(INVENTORY)))
    // the composed tree, in policy order: the lock, then its inventory, then the notices
    expect(ran).toEqual([REPO.setup, rule(`lockfiles`).command, rule(`npm licence inventory`).command, rule(`licence notices`).command])
    expect([outcome.checks, dispatch]).toEqual([`dispatched`, [`test.yml`]])
  })

  it(`a Merge now the rule fired for runs CI`, () => {
    const r = repo(files(BASE), REPO)
    const landed = r.branch(files([`a`, `b`, `m`, `z`]))
    const unit = r.branch(files([`a`, `m`, `y`, `z`]))
    const { outcome, dispatch } = compose({ dir: r.dir, trainId: `t`, baseSha: landed, mode: `untested`, units: [{ prs: [pr(2, unit)] }], runCommand })
    expect(outcome.units[0]).toMatchObject({ outcome: `composed`, resolved: [`lockfiles`, `npm licence inventory`, `licence notices`] })
    expect(r.read(INVENTORY)).toBe(inventoryOf(lockOf([`a`, `b`, `m`, `y`, `z`])))
    expect([outcome.checks, dispatch]).toEqual([`dispatched`, [`test.yml`]])
  })

  it(`an inventory the runner cannot collect (iOS) bounces instead of keeping ours`, () => {
    const IOS = `packages/licenses/inventory/ios.json`
    const r = repo({ ...files(BASE), [IOS]: `{"v":0}\n` }, REPO)
    const landed = r.branch({ [IOS]: `{"v":1}\n` })
    const unit = r.branch({ [IOS]: `{"v":2}\n` })
    const { outcome } = compose({ dir: r.dir, trainId: `t`, baseSha: landed, mode: `test`, units: [{ prs: [pr(2, unit)] }], runCommand })
    expect(outcome.units[0]).toMatchObject({ outcome: `conflict`, files: [IOS] })
  })
})
