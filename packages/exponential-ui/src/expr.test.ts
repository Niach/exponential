// VAPP-85: the macro template value language, op by op.

import { describe, expect, test } from "bun:test"
import { CORE_FUNCTIONS, evalCondition, evalConditionValue, evalValue, type ExprContext } from "./expr"

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

describe(`round 1: bound inputs emit calls`, () => {
  const bound: ExprContext = { id: `n1`, props: { page: { path: `/page` }, open: { path: `/ui/open` }, value: { call: `len`, args: { value: { path: `/items` } } }, total: 5, label: `Hi` }, vars: {} }

  test(`a whole-string member passes a binding through untouched (fallback dropped)`, () => {
    expect(evalValue(`{props.page}`, bound)).toEqual({ path: `/page` })
    expect(evalValue(`{props.page|1}`, bound)).toEqual({ path: `/page` })
    expect(evalValue({ open: `{props.open|false}` }, bound)).toEqual({ open: { path: `/ui/open` } })
    expect(evalValue(`{props.value}`, bound)).toEqual({ call: `len`, args: { value: { path: `/items` } } })
  })

  test(`as an operand the fallback stays: fallback{value, default}`, () => {
    expect(evalValue({ $eq: [`{props.page|1}`, 1] }, bound)).toEqual({ call: `eq`, args: { a: { call: `fallback`, args: { value: { path: `/page` }, default: 1 } }, b: 1 } })
    expect(evalValue({ $add: [`{props.page}`, -1] }, bound)).toEqual({ call: `add`, args: { a: { path: `/page` }, b: -1 } })
    expect(evalValue({ $sub: [`{props.total}`, `{props.page}`] }, bound)).toEqual({ call: `sub`, args: { a: 5, b: { path: `/page` } } })
    expect(evalValue({ $lt: [0, `{props.page}`] }, bound)).toEqual({ call: `lt`, args: { a: 0, b: { path: `/page` } } })
    expect(evalValue({ $percent: [`{props.page}`, `{props.total}`] }, bound)).toEqual({ call: `percent`, args: { value: { path: `/page` }, max: 5 } })
  })

  test(`interpolation, negation, $text, $map, $cond, $coalesce, len`, () => {
    expect(evalValue(`{props.page|1} / {props.total}`, bound)).toEqual({ call: `concat`, args: { values: [{ call: `fallback`, args: { value: { path: `/page` }, default: 1 } }, ` / `, 5] } })
    expect(evalValue(`{!props.open}`, bound)).toEqual({ call: `not`, args: { value: { path: `/ui/open` } } })
    expect(evalValue({ $text: `props.page` }, bound)).toEqual({ call: `text`, args: { value: { path: `/page` } } })
    expect(evalValue({ $map: { from: `props.page`, cases: { "1": `first` }, default: `other` } }, bound)).toEqual({ call: `map`, args: { value: { path: `/page` }, cases: { "1": `first` }, default: `other` } })
    expect(evalValue({ $cond: [`props.open`, `a`, `b`] }, bound)).toEqual({ call: `cond`, args: { if: { path: `/ui/open` }, then: `a`, else: `b` } })
    expect(evalValue({ $coalesce: [`{props.missing}`, `{props.page}`, `x`] }, bound)).toEqual({ call: `coalesce`, args: { values: [{ path: `/page` }, `x`] } })
    expect(evalValue({ $coalesce: [`{props.label}`, `{props.page}`] }, bound)).toBe(`Hi`)
    expect(evalValue(`{len(props.page)}`, bound)).toEqual({ call: `len`, args: { value: { path: `/page` } } })
    expect(evalValue(`{range(props.page)}`, bound)).toBeUndefined()
  })

  test(`conditions: decided when literal, the dynamic value when bound`, () => {
    expect(evalConditionValue(`props.open`, bound)).toEqual({ path: `/ui/open` })
    expect(evalConditionValue(`props.label`, bound)).toBe(true)
    expect(evalConditionValue({ $eq: [`{props.page}`, 2] }, bound)).toEqual({ call: `eq`, args: { a: { path: `/page` }, b: 2 } })
    expect(evalCondition(`props.open`, bound)).toBe(true)
  })

  test(`the core functions compute what the expander would have`, () => {
    expect(CORE_FUNCTIONS.percent({ value: 65, max: 100 })).toBe(`65%`)
    expect(CORE_FUNCTIONS.percent({ value: 1, max: 0 })).toBe(`0%`)
    expect(CORE_FUNCTIONS.fallback({ value: undefined, default: 1 })).toBe(1)
    expect(CORE_FUNCTIONS.fallback({ value: 0, default: 1 })).toBe(0)
    expect(CORE_FUNCTIONS.coalesce({ values: [``, null, 0, 3] })).toBe(0)
    expect(CORE_FUNCTIONS.concat({ values: [`a`, undefined, 2] })).toBe(`a2`)
    expect(CORE_FUNCTIONS.map({ value: 2, cases: { "2": `two` }, default: `?` })).toBe(`two`)
    expect(CORE_FUNCTIONS.len({ value: `abc` })).toBe(3)
    expect(CORE_FUNCTIONS.cond({ if: 0, then: `yes`, else: `no` })).toBe(`yes`)
    expect(CORE_FUNCTIONS.text({ value: undefined })).toBeUndefined()
  })
})
