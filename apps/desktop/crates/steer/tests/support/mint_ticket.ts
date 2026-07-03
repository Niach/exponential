// Test-support ticket mint for the steer relay integration test
// (crates/steer/tests/relay_integration.rs).
//
// The desktop is a ticket CONSUMER (masterplan-v3 §8.2) — production tickets
// come from the web `steer.mintTicket` router. In the integration test there
// is no web server, so we sign directly with @exp/steer-ticket (resolved from
// the repo-root bun workspace) using the same shared secret the locally-run
// relay verifies with: the ticket FORMAT is the shared package's contract.
//
// Usage: bun mint_ticket.ts '<claims-json-without-iat-exp>' <secret>
// Prints the signed ticket on stdout.

import { signSteerTicket, type SteerTicketClaims } from "@exp/steer-ticket"

const [claimsJson, secret] = process.argv.slice(2)
if (!claimsJson || !secret) {
  console.error(`usage: bun mint_ticket.ts '<claims-json>' <secret>`)
  process.exit(2)
}

const now = Math.floor(Date.now() / 1000)
const claims: SteerTicketClaims = {
  iat: now,
  exp: now + 60,
  ...JSON.parse(claimsJson),
}
console.log(signSteerTicket(claims, secret))
