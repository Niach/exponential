// #NEW-134: the backfill script's flags (scripts/backfill-issue-embeddings.ts),
// parsed apart from it so the rules are unit-tested.

export interface BackfillArgs {
  /** Issues per batch (1 to 500, default 25). */
  batch: number
  /** Pause between two batches in ms (default 250). */
  sleepMs: number
  /** Stop after this many issues (default: all). */
  limit: number
  /** Count what is missing or stale, write nothing. */
  dryRun: boolean
  /** Only this team's issues. */
  teamId?: string
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export function parseBackfillArgs(argv: readonly string[]): BackfillArgs {
  const args: BackfillArgs = { batch: 25, sleepMs: 250, limit: Number.POSITIVE_INFINITY, dryRun: false }
  const number = (flag: string, raw: string | undefined, min: number, max: number): number => {
    const value = Number(raw)
    if (raw === undefined || !Number.isInteger(value) || value < min || value > max) {
      throw new Error(`${flag} needs a whole number from ${min} to ${max}`)
    }
    return value
  }
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i]!
    if (flag === `--dry-run`) args.dryRun = true
    else if (flag === `--batch`) args.batch = number(flag, argv[++i], 1, 500)
    else if (flag === `--sleep-ms`) args.sleepMs = number(flag, argv[++i], 0, 600_000)
    else if (flag === `--limit`) args.limit = number(flag, argv[++i], 1, Number.MAX_SAFE_INTEGER)
    else if (flag === `--team`) {
      const teamId = argv[++i]
      if (!teamId || !UUID.test(teamId)) throw new Error(`--team needs a team id (uuid)`)
      args.teamId = teamId
    } else throw new Error(`unknown flag ${flag}`)
  }
  return args
}
