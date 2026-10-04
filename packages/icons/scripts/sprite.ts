// EXP-1184: split a vertical sprite sheet (one <path>, frames stacked in
// 100-unit rows of a `0 0 100 (100·n)` viewBox) into n single-frame paths on
// a `0 0 100 100` grid. The path is re-emitted ABSOLUTE (M/L/C/Q/A/Z) so every platform's path parser — browsers, usvg, SwiftUI's SVG
// assets, Android VectorDrawable — reads the same geometry.

type Cmd = { op: string; args: number[] }

const ARITY: Record<string, number> = {
  m: 2, l: 2, h: 1, v: 1, c: 6, s: 4, q: 4, t: 2, z: 0, a: 7,
}

function tokenize(d: string): Cmd[] {
  // Character-level so minified arc flags (`a1 1 0 011 1`) split right.
  const out: Cmd[] = []
  const num = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/
  let i = 0
  let cur: Cmd | null = null
  const skip = () => {
    while (i < d.length && /[\s,]/.test(d[i])) i++
  }
  while (true) {
    skip()
    if (i >= d.length) break
    if (/[a-zA-Z]/.test(d[i])) {
      cur = { op: d[i], args: [] }
      out.push(cur)
      i++
      continue
    }
    if (!cur) throw new Error(`sprite: path starts with a number`)
    const slot = cur.args.length % 7
    if (cur.op.toLowerCase() === `a` && (slot === 3 || slot === 4)) {
      cur.args.push(Number(d[i]))
      i++
      continue
    }
    const m = d.slice(i).match(num)
    if (!m) throw new Error(`sprite: bad token at ${i}`)
    cur.args.push(Number(m[0]))
    i += m[0].length
  }
  return out
}

/** Absolute M/L/C/Q/Z subpaths, each a list of [op, ...points]. */
function absolutize(d: string): Array<Array<[string, ...number[]]>> {
  const subpaths: Array<Array<[string, ...number[]]>> = []
  let x = 0, y = 0, sx = 0, sy = 0
  let lastCtrl: [number, number] | null = null
  let lastOp = ``
  let path: Array<[string, ...number[]]> = []
  for (const { op, args } of tokenize(d)) {
    const lower = op.toLowerCase()
    const rel = op !== op.toUpperCase()
    const n = ARITY[lower]
    if (n === undefined) throw new Error(`sprite: unknown path op "${op}"`)
    if (lower === `z`) {
      path.push([`Z`])
      x = sx; y = sy; lastCtrl = null; lastOp = `z`
      continue
    }
    for (let i = 0; i < args.length; i += n) {
      const a = args.slice(i, i + n)
      const first = i === 0
      const ox = rel ? x : 0, oy = rel ? y : 0
      let eff = lower
      if (lower === `m` && !first) eff = `l`
      switch (eff) {
        case `m`: {
          if (path.length) subpaths.push(path)
          path = []
          x = a[0] + ox; y = a[1] + oy; sx = x; sy = y
          path.push([`M`, x, y]); lastCtrl = null
          break
        }
        case `l`: x = a[0] + ox; y = a[1] + oy; path.push([`L`, x, y]); lastCtrl = null; break
        case `h`: x = a[0] + (rel ? x : 0); path.push([`L`, x, y]); lastCtrl = null; break
        case `v`: y = a[0] + (rel ? y : 0); path.push([`L`, x, y]); lastCtrl = null; break
        case `c`: {
          const p = [a[0] + ox, a[1] + oy, a[2] + ox, a[3] + oy, a[4] + ox, a[5] + oy]
          path.push([`C`, ...p] as [string, ...number[]])
          lastCtrl = [p[2], p[3]]; x = p[4]; y = p[5]; break
        }
        case `s`: {
          const c1: [number, number] = lastCtrl && /[cs]/.test(lastOp) ? [2 * x - lastCtrl[0], 2 * y - lastCtrl[1]] : [x, y]
          const p = [c1[0], c1[1], a[0] + ox, a[1] + oy, a[2] + ox, a[3] + oy]
          path.push([`C`, ...p] as [string, ...number[]])
          lastCtrl = [p[2], p[3]]; x = p[4]; y = p[5]; break
        }
        case `q`: {
          const p = [a[0] + ox, a[1] + oy, a[2] + ox, a[3] + oy]
          path.push([`Q`, ...p] as [string, ...number[]])
          lastCtrl = [p[0], p[1]]; x = p[2]; y = p[3]; break
        }
        case `a`: {
          const p = [a[0], a[1], a[2], a[3], a[4], a[5] + ox, a[6] + oy]
          path.push([`A`, ...p] as [string, ...number[]])
          lastCtrl = null; x = p[5]; y = p[6]; break
        }
        case `t`: {
          const c1: [number, number] = lastCtrl && /[qt]/.test(lastOp) ? [2 * x - lastCtrl[0], 2 * y - lastCtrl[1]] : [x, y]
          const p = [c1[0], c1[1], a[0] + ox, a[1] + oy]
          path.push([`Q`, ...p] as [string, ...number[]])
          lastCtrl = c1; x = p[2]; y = p[3]; break
        }
      }
      lastOp = eff
    }
  }
  if (path.length) subpaths.push(path)
  return subpaths
}

const fmt = (n: number): string => {
  const r = Math.round(n * 1000) / 1000
  return Object.is(r, -0) ? `0` : String(r)
}

/** The frames of a sprite sheet `<svg viewBox="0 0 100 H"><path d=…/></svg>`. */
export function splitSprite(svg: string, frameCount: number): string[] {
  const viewBox = svg.match(/viewBox="0 0 100 (\d+)"/)
  if (!viewBox || Number(viewBox[1]) !== frameCount * 100) {
    throw new Error(`sprite: expected a 0 0 100 ${frameCount * 100} viewBox`)
  }
  const ds = [...svg.matchAll(/<path[^>]*\sd="([^"]+)"/g)].map((m) => m[1])
  if (ds.length !== 1) throw new Error(`sprite: expected exactly one <path>`)
  const frames: string[][] = Array.from({ length: frameCount }, () => [])
  for (const sub of absolutize(ds[0])) {
    const row = Math.floor((sub[0][2] as number) / 100)
    if (row < 0 || row >= frameCount) throw new Error(`sprite: subpath outside the sheet`)
    const dy = row * 100
    frames[row].push(
      sub
        .map(([op, ...p]) =>
          op === `A`
            ? op + p.map((v, i) => fmt(i === 6 ? v - dy : v)).join(` `)
            : op + p.map((v, i) => fmt(i % 2 ? v - dy : v)).join(` `)
        )
        .join(``)
    )
  }
  frames.forEach((f, i) => {
    if (!f.length) throw new Error(`sprite: frame ${i} draws nothing`)
  })
  return frames.map((f) => f.join(``))
}
