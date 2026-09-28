// Renders spike/vapp-3/RESULTS.md from results/*.jsonl. Idempotent.
import { readdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"

const here = new URL("..", import.meta.url).pathname
const dir = join(here, "results")
type Row = Record<string, any>
const rows: Row[] = []
for (const f of readdirSync(dir).filter((f) => f.endsWith(".jsonl")).sort()) {
  for (const line of readFileSync(join(dir, f), "utf8").split("\n")) {
    const t = line.trim()
    if (!t) continue
    try { rows.push({ ...JSON.parse(t), _file: f }) } catch { /* skip */ }
  }
}
const q = (xs: number[], p: number) => { if (!xs.length) return NaN; const s = [...xs].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(p * (s.length - 1) + 0.5))] }
const f1 = (n: number) => Number.isFinite(n) ? n.toFixed(1) : "–"
const f2 = (n: number) => Number.isFinite(n) ? n.toFixed(2) : "–"
const mb = (b: number) => Number.isFinite(b) ? `${(b / 1024 / 1024).toFixed(2)} MB` : "–"

let out = `# VAPP-3 results (generated ${new Date().toISOString()} by scripts/report.ts)\n\n`

// Bench matrix
const bench = rows.filter((r) => r.kind === "bench")
const groups = new Map<string, Row[]>()
for (const r of bench) { const k = `${r.source}|${r.scenario}|${r.policy}`; groups.set(k, [...(groups.get(k) ?? []), r]) }
out += `## Connectivity + performance\n\n| source | scenario | policy | runs | ok | direct (no relay) | connect p50/p95 ms | RTT p50/p95 ms | up / down Mbps (${bench[0]?.bodyBytes ?? "2 MB"} B) | paths seen |\n|---|---|---|---|---|---|---|---|---|---|\n`
for (const [k, rs] of [...groups.entries()].sort()) {
  const [source, scenario, policy] = k.split("|")
  const ok = rs.filter((r) => r.ok)
  const direct = ok.filter((r) => r.localPath !== "relay" && r.remotePath !== "relay")
  const paths = [...new Set(ok.map((r) => `${r.localPath}↔${r.remotePath}`))].join(", ")
  out += `| ${source} | ${scenario} | ${policy} | ${rs.length} | ${ok.length} | ${ok.length ? Math.round(100 * direct.length / ok.length) : 0}% | ${f1(q(ok.map((r) => r.connectMs), 0.5))} / ${f1(q(ok.map((r) => r.connectMs), 0.95))} | ${f2(q(ok.map((r) => r.rttP50Ms), 0.5))} / ${f2(q(ok.map((r) => r.rttP95Ms), 0.5))} | ${f1(q(ok.map((r) => r.upMbps), 0.5))} / ${f1(q(ok.map((r) => r.downMbps), 0.5))} | ${paths} |\n`
}
const rec = bench.filter((r) => r.reconnect)
if (rec.length) {
  out += `\n### Reconnect\n\n| source | scenario | trigger | runs | reconnect p50/p95 ms |\n|---|---|---|---|---|\n`
  const g2 = new Map<string, Row[]>()
  for (const r of rec) { const k = `${r.source}|${r.scenario}|${r.reconnect.trigger}`; g2.set(k, [...(g2.get(k) ?? []), r]) }
  for (const [k, rs] of [...g2.entries()].sort()) { const [s, sc, t] = k.split("|"); const ms = rs.map((r) => r.reconnect.ms); out += `| ${s} | ${sc} | ${t} | ${rs.length} | ${f1(q(ms, 0.5))} / ${f1(q(ms, 0.95))} |\n` }
}
const fails = bench.filter((r) => !r.ok)
if (fails.length) { out += `\n### Failures\n\n`; for (const r of fails) out += `- ${r.source} ${r.scenario} ${r.policy}: ${r.error ?? "?"} ${r.notes ?? ""}\n` }

// Tamper
const tamper = rows.filter((r) => r.kind === "tamper")
if (tamper.length) {
  out += `\n## Signed fingerprints: tamper test\n\n| source | variant | rejected | reason |\n|---|---|---|---|\n`
  for (const r of tamper) out += `| ${r.source} | ${r.variant} | ${r.rejected ? "yes" : "NO"} | ${r.reason ?? ""} |\n`
}

// Sizes
const size = rows.filter((r) => r.kind === "size")
if (size.length) {
  out += `\n## Binary cost\n\n| platform | artifact | variant | bytes | size | notes |\n|---|---|---|---|---|---|\n`
  for (const r of size.sort((a, b) => `${a.platform}${a.artifact}${a.variant}`.localeCompare(`${b.platform}${b.artifact}${b.variant}`))) out += `| ${r.platform} | ${r.artifact} | ${r.variant} | ${r.bytes} | ${mb(r.bytes)} | ${r.notes ?? ""} |\n`
}
const cold = rows.filter((r) => r.kind === "coldstart")
if (cold.length) {
  out += `\n## Cold start\n\n| platform | variant | samples | median ms | method |\n|---|---|---|---|---|\n`
  for (const r of cold) out += `| ${r.platform} | ${r.variant} | ${r.samplesMs?.length ?? 0} | ${f1(r.medianMs)} | ${r.method ?? ""} |\n`
}
const turn = rows.filter((r) => r.kind === "turn_bandwidth")
if (turn.length) {
  out += `\n## TURN bandwidth per session\n\n| source | bytes relayed | seconds | notes |\n|---|---|---|---|\n`
  for (const r of turn) out += `| ${r.source} | ${r.sessionBytesRelayed} | ${r.seconds} | ${r.notes ?? ""} |\n`
}
writeFileSync(join(here, "RESULTS.md"), out)
console.log(`wrote RESULTS.md from ${rows.length} rows`)
