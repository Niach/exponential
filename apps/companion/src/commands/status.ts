import { loadConfig } from "../config"
import { heartbeat } from "../exponential-api"
import { openState } from "../state"

export async function runStatus(): Promise<void> {
  const config = await loadConfig().catch((e) => {
    console.error(`Could not load config:`, e instanceof Error ? e.message : e)
    process.exit(1)
  })

  try {
    await heartbeat(config)
    const state = openState()
    const inFlight = state.listIssues({
      status: [`claimed`, `coding`, `pushed`],
    }).length
    state.close()
    console.log(`Companion API OK · ${inFlight} issue(s) in local state`)
  } catch (e) {
    console.error(`Companion check failed:`, e instanceof Error ? e.message : e)
    process.exit(1)
  }
}
