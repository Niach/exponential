// @vitest-environment node
import { describe, expect, it } from "vitest"
import { relayTicket } from "./relay-ticket"

// EXP-1471: the same vector as apps/desktop/crates/enet-core/tests/net.rs
// `relay_ticket_vector` (identity [7; 32] on the production relay).
const ID = `ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c`
const TICKET = `enet15jfgyy7ctrjavpxvkb5rglwf7gkuo5vox27hxescd3vgsfcg2iwachlior2ha4z2f4xxezlmmf4s4zlyobxw4zlooruwc3bomf2c6aa`

describe(`relayTicket`, () => {
  it(`encodes the enet-core ticket of a relay-homed peer`, () => {
    expect(relayTicket(ID, `https://relay.exponential.at`)).toBe(TICKET)
    expect(relayTicket(ID, `https://relay.exponential.at/`)).toBe(TICKET)
    expect(relayTicket(ID.toUpperCase(), `https://relay.exponential.at`)).toBe(TICKET)
  })

  it(`refuses a malformed endpoint id`, () => {
    expect(relayTicket(`a1`, `https://relay.exponential.at`)).toBeUndefined()
  })
})
