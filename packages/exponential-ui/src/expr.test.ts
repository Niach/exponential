// VAPP-85: the macro template value language, op by op.

import { describe, expect, test } from "bun:test"
import { evalCondition, evalValue, type ExprContext } from "./expr"

const ctx: ExprContext = { id: `n1`, props: { gap: `lg`, count: 0, title: ``, items: [`a`, `b`], selected: `b`, page: 2 }, vars: { item: { value: `b`, label: `Bee` }, index: 1 } }

describe(`template values`, () => {
  test(`paths, fallbacks, length, negation and the id`, () => {
    expect(evalValue(`{props.gap}`, ctx)).toBe(`lg`)
    expect(evalValue(`{props.missing|md}`, ctx)).toBe(`md`)
    expect(evalValue(`{props.missing|3}`, ctx)).toBe(3)
    expect(evalValue(`{props.count}`, ctx)).toBe(0)
    expect(evalValue(`{len(props.items)}`, ctx)).toBe(2)
    expect(evalValue(`{!props.title}`, ctx)).toBe(true)
    expect(evalValue(`{id}`, ctx)).toBe(`n1`)
    expect(evalValue(`{item.label} #{index}`, ctx)).toBe(`Bee #1`)
    expect(evalValue(`$spacing.{props.gap|md}`, ctx)).toBe(`$spacing.lg`)
    expect(evalValue(`{props.missing}`, ctx)).toBeUndefined()
  })

  test(`conditions: 0 is true, empty string and missing are false`, () => {
    expect(evalCondition(`props.count`, ctx)).toBe(true)
    expect(evalCondition(`props.title`, ctx)).toBe(false)
    expect(evalCondition(`props.missing`, ctx)).toBe(false)
    expect(evalCondition(`!props.missing`, ctx)).toBe(true)
    expect(evalCondition({ $eq: [`{item.value}`, `{props.selected}`] }, ctx)).toBe(true)
  })

  test(`the value objects`, () => {
    expect(evalValue({ $map: { from: `props.gap`, cases: { lg: 16 }, default: 8 } }, ctx)).toBe(16)
    expect(evalValue({ $map: { from: `props.missing`, cases: { lg: 16 }, default: `{props.gap}` } }, ctx)).toBe(`lg`)
    expect(evalValue({ $cond: [`props.title`, `yes`, `no`] }, ctx)).toBe(`no`)
    expect(evalValue({ $add: [`{props.page}`, -1] }, ctx)).toBe(1)
    expect(evalValue({ $percent: [`{props.page}`, 8] }, ctx)).toBe(`25%`)
    expect(evalValue({ $percent: [150, 100] }, ctx)).toBe(`100%`)
    expect(evalValue({ $percent: [1, 3] }, ctx)).toBe(`33.33%`)
    expect(evalValue({ $coalesce: [`{props.title}`, `{props.missing}`, `Choose`] }, ctx)).toBe(`Choose`)
    expect(evalValue({ $text: `props.count` }, ctx)).toBe(`0`)
    expect(evalValue({ $text: `props.missing` }, ctx)).toBeUndefined()
  })

  test(`objects drop undefined members and arrays drop undefined items`, () => {
    expect(evalValue({ a: `{props.missing}`, b: `{props.gap}` }, ctx)).toEqual({ b: `lg` })
    expect(evalValue([`{props.missing}`, `{props.gap}`], ctx)).toEqual([`lg`])
  })
})
