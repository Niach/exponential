import {
  assertDatabaseReachable,
  assertElectricReachable,
  assertIssueNumberTriggerExists,
} from "./helpers/db"

export default async function globalSetup() {
  await assertDatabaseReachable()
  await assertElectricReachable()
  await assertIssueNumberTriggerExists()
}
