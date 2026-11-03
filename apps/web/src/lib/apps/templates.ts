// VAPP-234 (fixtures/app-templates.json): the template gallery's files,
// bundled into the server (never the browser: only `apps.importTemplate`
// imports this module). A template imports only when its signature verifies
// against the PINNED signer; then `templateImportFile` is imported exactly
// like an `.app.json` file.

import { APP_TEMPLATES_SIGNER, templateImportFile } from "@exp/domain-contract/app-templates"
import { verifyPackageFile } from "@exponential_app/eui"
import crm from "../../../../../samples/apps/crm.app.json" with { type: "json" }
import standup from "../../../../../samples/apps/standup.app.json" with { type: "json" }
import timeTracker from "../../../../../samples/apps/time-tracker.app.json" with { type: "json" }
import leavePlanner from "../../../../../samples/apps/leave-planner.app.json" with { type: "json" }

const FILES: Record<string, unknown> = { "time-tracker": timeTracker, crm, standup, "leave-planner": leavePlanner }

/** The text `apps.import` takes for template `id`, or why it cannot be
 *  imported (an unknown id, a signature that does not verify against the
 *  pinned key). */
export async function templateImportText(id: string, includeRecords: boolean): Promise<{ text: string } | { error: string }> {
  const file = Object.hasOwn(FILES, id) ? FILES[id] : undefined
  if (!file || typeof file !== `object`) return { error: `There is no template "${id}"` }
  const verified = await verifyPackageFile(file)
  if (verified.status !== `valid` || verified.author.key !== APP_TEMPLATES_SIGNER.key) return { error: `The template "${id}" is not signed by ${APP_TEMPLATES_SIGNER.name}` }
  return { text: JSON.stringify(templateImportFile(file as Record<string, unknown>, includeRecords)) }
}
