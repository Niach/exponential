// Round 9 §9.1: fixtures/markdown-inline.json.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/markdown-inline.json"
import { markdownInlineRecipes } from "./recipes"
import { recipeStyle } from "./theme"
import { builtinTheme } from "./themes"

describe(`fixtures/markdown-inline.json`, () => {
  const neutral = builtinTheme(`neutral`)
  for (const c of fixture.cases) {
    test(`${c.part} ${c.variant ?? c.level}`, () => {
      const recipes = markdownInlineRecipes(c.part as `paragraph` | `heading`, c)
      expect(recipes).toEqual(c.recipes)
      expect(Object.assign({}, ...recipes.map((q) => recipeStyle(neutral, q)))).toEqual(c.neutral)
    })
  }
})
