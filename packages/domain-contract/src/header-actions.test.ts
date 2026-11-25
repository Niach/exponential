import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/header-actions.json" with { type: "json" }
import { headerActions, headerMenu, orderHeaderActions, type HeaderAction, type HeaderPage } from "./header-actions"

describe(`header actions (#EXP-2109)`, () => {
  for (const c of fixture.orderCases) {
    test(`order: ${c.name}`, () => expect(orderHeaderActions(c.actions as HeaderAction[])).toEqual(c.expected as HeaderAction[]))
  }
  for (const c of fixture.menuCases) test(`menu: ${c.name}`, () => expect(headerMenu(c.entries)).toEqual(c.expected))
  test(`every page's lists are in the one order`, () => {
    for (const [page, forms] of Object.entries(fixture.pages)) {
      for (const form of [`phone`, `wide`] as const) {
        expect(headerActions(page as HeaderPage, form)).toEqual(forms[form] as never)
      }
    }
  })
})
