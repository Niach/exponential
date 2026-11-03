import { describe, expect, test } from "vitest"
import fixture from "@exp/domain-contract/fixtures/apps.json"
import { screenPath } from "@exp/domain-contract/apps"
import { routeScreenParams } from "./app-screens"

describe(`routeScreenParams (VAPP-218)`, () => {
  test.each(fixture.screenQueryCases)(`the contract's query cases %j`, (c) => {
    expect(routeScreenParams(c.query)).toEqual(c.params)
  })
  test(`the router's JSON-parsed values read back as strings`, () => {
    expect(routeScreenParams({ x: 1, flag: true, none: null, skip: undefined, tab: `data` })).toEqual({ flag: `true`, none: `null`, x: `1` })
  })
  test(`a route built by screenPath reads back its params`, () => {
    const params = { stage: `won & lost`, company: `Ünï` }
    const path = screenPath(`acme`, `crm`, `deals`, params)
    const query = Object.fromEntries(new URL(path, `https://x.test`).searchParams)
    expect(routeScreenParams(query)).toEqual({ company: `Ünï`, stage: `won & lost` })
  })
})
