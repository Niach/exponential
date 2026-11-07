// Round 9 §9.1: fixtures/markdown-inline.json.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/markdown-inline.json"
import { markdownInlineRecipes } from "./recipes"
import { recipeStyle } from "./theme"
import { builtinTheme } from "./themes"
import { BASE_CSS } from "./web/base-css"

describe(`fixtures/markdown-inline.json`, () => {
  const neutral = builtinTheme(`neutral`)
  for (const c of fixture.cases) {
    test(`${c.part} ${c.variant ?? c.level}`, () => {
      const recipes = markdownInlineRecipes(c.part as `paragraph` | `heading`, c)
      expect(recipes).toEqual(c.recipes)
      expect(Object.assign({}, ...recipes.map((q) => recipeStyle(neutral, q)))).toEqual(c.neutral)
    })
  }

  test(`documentPaint: inline code is the recipe, lists are base-css`, () => {
    const { code, list } = fixture.documentPaint
    expect(recipeStyle(neutral, { component: `Markdown`, part: `code` })).toEqual(code.neutral)
    expect(BASE_CSS).toContain(`padding-inline-start:${list.indentEm}em`)
    expect(BASE_CSS).toContain(`.xui-md ul{list-style:disc}`)
    expect(recipeStyle(neutral, { component: `Markdown`, part: `listItem` })).toEqual({})
    expect(list.itemGap).toBe(0)
  })
})
