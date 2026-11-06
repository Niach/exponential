// EXP-1708: the git seam of the train engine (a runner with git; no forge).

export interface Run {
  ok: boolean
  out: string
  err: string
}

export function sh(cwd: string, cmd: string[], input?: string, env?: Record<string, string>): Run {
  const p = Bun.spawnSync(cmd, {
    cwd,
    stdin: input === undefined ? `ignore` : new TextEncoder().encode(input),
    env: { ...process.env, ...env },
  })
  return { ok: p.exitCode === 0, out: p.stdout.toString(), err: p.stderr.toString() }
}

export class Git {
  constructor(readonly dir: string) {}

  run(...args: string[]): Run {
    return sh(this.dir, [`git`, ...args])
  }

  must(...args: string[]): string {
    const r = this.run(...args)
    if (!r.ok) throw new Error(`git ${args.join(` `)}: ${r.err.trim() || r.out.trim()}`)
    return r.out.trim()
  }

  lines(...args: string[]): string[] {
    return this.must(...args).split(`\n`).filter(Boolean)
  }

  /** A path's content at stage 1 (base), 2 (ours) or 3 (theirs); null = absent. */
  stage(n: 1 | 2 | 3, path: string): string | null {
    const r = this.run(`show`, `:${n}:${path}`)
    return r.ok ? r.out : null
  }

  show(rev: string, path: string): string | null {
    const r = this.run(`show`, `${rev}:${path}`)
    return r.ok ? r.out : null
  }
}
