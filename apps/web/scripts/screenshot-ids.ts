/**
 * Print the seeded demo instance's real ids as JSON (EXP-566).
 *
 * A thin printer over `lib/demo-ids.ts` — that module owns every query and
 * every "absent, so the view skips" decision, because `capture-views.ts` needs
 * the same answers in-process while the desktop lane can only shell out.
 *
 * Usage (from apps/web, after `bun run seed:screenshots`):
 *   bun run screenshots:ids
 *
 * Emits, on stdout, exactly one JSON object — see `DemoIds`. Note that it
 * carries `supportToken`, the reporter's magic link: it is a CREDENTIAL, so the
 * pipeline parses this output and never echoes it.
 */
import { resolveDemoIds } from "./lib/demo-ids"

async function main() {
  console.log(JSON.stringify(await resolveDemoIds(), null, 2))
  process.exit(0)
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err)
  process.exit(1)
})
