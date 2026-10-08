// VAPP-85: the Box style whitelist, the VAPP-4 spike's rules as tests.

import { describe, expect, test } from "bun:test"
import styleJson from "../catalog/style.json" with { type: "json" }
import conditionsFixture from "../fixtures/style-conditions.json" with { type: "json" }
import { STYLE_KEYS, STYLE_LAYOUT_EFFECT_KEYS, STYLE_STATES, activeBreakpoint, create, mediaMatches, parseMediaCondition, props, resolveConditions, validateStyle, type ConditionContext } from "./style"

describe(`Box style whitelist`, () => {
  test(`the spike's v1 whitelist plus the logical inline properties`, () => {
    const spike = [
      `display`, `flexDirection`, `flexWrap`, `justifyContent`, `alignItems`, `alignContent`, `alignSelf`, `justifySelf`,
      `flexGrow`, `flexShrink`, `flexBasis`, `gap`, `rowGap`, `columnGap`, `direction`, `overflow`, `width`, `height`,
      `minWidth`, `minHeight`, `maxWidth`, `maxHeight`, `aspectRatio`, `position`, `top`, `right`, `bottom`, `left`, `inset`,
      `padding`, `paddingTop`, `paddingRight`, `paddingBottom`, `paddingLeft`, `paddingHorizontal`, `paddingVertical`,
      `margin`, `marginTop`, `marginRight`, `marginBottom`, `marginLeft`, `marginHorizontal`, `marginVertical`,
      `gridTemplateColumns`, `gridTemplateRows`, `gridTemplateAreas`, `gridArea`, `gridColumn`, `gridRow`,
      `backgroundColor`, `color`, `borderWidth`, `borderColor`, `borderRadius`, `opacity`, `boxShadow`, `fontSize`,
      `fontWeight`, `lineHeight`, `textAlign`,
    ]
    for (const key of spike) expect(STYLE_KEYS).toContain(key)
    for (const key of [`insetInlineStart`, `insetInlineEnd`, `paddingInlineStart`, `paddingInlineEnd`, `marginInlineStart`, `marginInlineEnd`])
      expect(STYLE_KEYS).toContain(key)
    // Still out in round 1: zIndex (paint order = tree order), order, gridAutoFlow, justifyItems.
    for (const key of [`zIndex`, `order`, `gridAutoFlow`, `justifyItems`]) expect(STYLE_KEYS).not.toContain(key)
    expect(styleJson.visual.borderWidth.layoutEffect).toBe(true)
  })

  test(`values: px, percent, auto, tokens, colours`, () => {
    expect(validateStyle({ width: 12, height: `50%`, minWidth: `auto`, gap: `$spacing.md`, backgroundColor: `#ff0000`, color: `$color.primary`, borderRadius: `$radius.lg`, boxShadow: `$shadow.sm`, opacity: `$opacity.secondary`, aspectRatio: `16/9`, gridTemplateAreas: [`a b`], fontWeight: 600 })).toEqual([])
    expect(validateStyle({ width: `12em` })).toEqual([{ path: `style.width`, message: `width: expected px, "N%", "auto" or a numeric token` }])
    expect(validateStyle({ color: `$spacing.md` })[0].message).toContain(`#hex or $color`)
    expect(validateStyle({ gap: `$spacing.huge` })).toHaveLength(1)
    expect(validateStyle({ display: `inline` })[0].message).toContain(`expected one of`)
    expect(validateStyle({ zIndex: 2 })).toEqual([{ path: `style.zIndex`, message: `not in the Box style whitelist` }])
  })

  test(`conditions: surface media and :pressed, one level deep, direction root-only`, () => {
    expect(validateStyle({ "@media (min-width: 600px)": { gap: 8 }, ":pressed": { opacity: 0.6 } })).toEqual([])
    expect(validateStyle({ "@media (min-width: 600px)": { ":pressed": { opacity: 0.6 } } })[0].message).toBe(`conditions do not nest`)
    expect(validateStyle({ ":active": { opacity: 1 } })[0].message).toBe(`not a supported condition`)
    expect(validateStyle({ direction: `rtl` }, { root: false })[0].message).toBe(`allowed on the root only`)
    expect(validateStyle({ direction: `rtl` }, { root: true })).toEqual([])
  })

  test(`create validates and freezes, props merges in order keeping conditions nested`, () => {
    const styles = create({ card: { display: `flex`, padding: 12, "@media (min-width: 600px)": { padding: 16 } }, pressed: { ":pressed": { opacity: 0.6 } } })
    expect(Object.isFrozen(styles)).toBe(true)
    expect(() => create({ bad: { color: `red` } as never })).toThrow()
    expect(props(styles.card, false, [styles.pressed, { padding: 8, "@media (min-width: 600px)": { gap: 4 } }])).toEqual({
      display: `flex`,
      padding: 8,
      "@media (min-width: 600px)": { padding: 16, gap: 4 },
      ":pressed": { opacity: 0.6 },
    })
  })
})

describe(`round 1: the whitelist additions`, () => {
  test(`the new keys are in, the per-side widths move layout`, () => {
    for (const key of [`overflowX`, `overflowY`, `insetBlockStart`, `insetBlockEnd`, `backgroundGradient`, `borderTopWidth`, `borderStyle`, `borderTopLeftRadius`, `letterSpacing`, `textDecoration`, `textTransform`, `fontStyle`, `transition`, `transitionEasing`, `transform`, `visibility`, `pointerEvents`, `userSelect`, `cursor`])
      expect(STYLE_KEYS).toContain(key)
    expect(STYLE_LAYOUT_EFFECT_KEYS).toEqual([`borderWidth`, `borderTopWidth`, `borderRightWidth`, `borderBottomWidth`, `borderLeftWidth`])
  })

  test(`values: enums, token-only motion, paint-only transforms, gradients`, () => {
    expect(validateStyle({ fontWeight: 300, textAlign: `justify`, textDecoration: `line-through`, borderStyle: `dashed`, fontStyle: `italic`, cursor: `pointer`, pointerEvents: `none`, visibility: `hidden`, letterSpacing: -0.5, borderTopLeftRadius: `$radius.lg` })).toEqual([])
    expect(validateStyle({ transition: `$motion.fast`, transitionEasing: `$ease.standard` })).toEqual([])
    expect(validateStyle({ transition: 200 })[0].message).toBe(`transition: expected $motion.<name>`)
    expect(validateStyle({ transitionEasing: `$motion.fast` })[0].message).toBe(`transitionEasing: expected $ease.<name>`)
    expect(validateStyle({ transform: `translate(4px, -2px) scale(1.05) rotate(90deg)` })).toEqual([])
    for (const bad of [`translateX(4px)`, `scale(1.1) `, `rotate(1rad)`, `skew(10deg)`, `matrix(1,0,0,1,0,0)`]) expect(validateStyle({ transform: bad }), bad).toHaveLength(1)
    expect(validateStyle({ backgroundGradient: { angle: 90, stops: [{ color: `$color.primary`, offset: 0 }, { color: `#00000000`, offset: 1 }] } })).toEqual([])
    expect(validateStyle({ backgroundGradient: { angle: 90, stops: [{ color: `red`, offset: 0 }] } })).toHaveLength(1)
  })

  test(`conditions: the media grammar, the states, breakpoint tokens must exist`, () => {
    const ok = [`@media (min-width: 600px)`, `@media (max-width: $breakpoint.md)`, `@media (min-height: 480.5px)`, `@media (max-height: $breakpoint.sm)`, `@media (orientation: portrait)`, `@media (hover: none)`, `@media (prefers-reduced-motion: reduce)`]
    for (const key of ok) expect(validateStyle({ [key]: { gap: 4 } }), key).toEqual([])
    for (const key of [`@media (min-width: 40em)`, `@media (width >= 600px)`, `@media (prefers-color-scheme: dark)`, `@media (min-width: $spacing.md)`, `@media screen`])
      expect(validateStyle({ [key]: { gap: 4 } })[0].message, key).toBe(`not a supported condition`)
    expect(validateStyle({ "@media (min-width: $breakpoint.huge)": { gap: 4 } })[0].message).toContain(`unknown breakpoint`)
    expect(STYLE_STATES).toEqual([`:hover`, `:focus-visible`, `:pressed`])
    for (const state of STYLE_STATES) expect(validateStyle({ [state]: { opacity: 0.5 } }), state).toEqual([])
  })

  test(`media conditions parse and match against the surface`, () => {
    expect(parseMediaCondition(`@media (max-width: $breakpoint.md)`)).toEqual({ feature: `max-width`, value: `$breakpoint.md` })
    expect(parseMediaCondition(`@media (min-width: 40em)`)).toBeNull()
    const bp = { sm: 640, md: 768, lg: 1024, xl: 1280 }
    expect(mediaMatches({ feature: `min-width`, value: `$breakpoint.md` }, { width: 768, breakpoints: bp })).toBe(true)
    expect(mediaMatches({ feature: `max-width`, value: `$breakpoint.md` }, { width: 768, breakpoints: bp })).toBe(false)
    expect(mediaMatches({ feature: `min-height`, value: `100px` }, { width: 768 })).toBe(false)
    expect(mediaMatches({ feature: `orientation`, value: `landscape` }, { width: 768 })).toBe(true)
  })

  test(`the active breakpoint is the last one the width reaches`, () => {
    const bp = { sm: 640, md: 768, lg: 1024, xl: 1280 }
    expect(activeBreakpoint(320, bp)).toBeNull()
    expect(activeBreakpoint(640, bp)).toBe(`sm`)
    expect(activeBreakpoint(1023, bp)).toBe(`md`)
    expect(activeBreakpoint(5000, bp)).toBe(`xl`)
  })

  test(`fixtures/style-conditions.json replays (source order, states last, pressed wins)`, () => {
    const file = conditionsFixture as unknown as { breakpoints: Record<string, number>; cases: { name: string; style: Record<string, unknown>; contexts: ConditionContext[]; expected: unknown[] }[] }
    expect(file.cases.length).toBeGreaterThan(5)
    for (const c of file.cases) {
      expect(validateStyle(c.style).filter((i) => !i.message.includes(`unknown breakpoint`)), c.name).toEqual([])
      c.contexts.forEach((ctx, i) => expect(resolveConditions(c.style, { ...ctx, breakpoints: ctx.breakpoints ?? file.breakpoints }), `${c.name} #${i}`).toEqual(c.expected[i] as Record<string, unknown>))
    }
    const order = file.cases.find((c) => c.name.startsWith(`blocks apply in source order`))!
    expect(order.expected[0]).toEqual({ padding: 4 })
    const states = file.cases.find((c) => c.name.startsWith(`states apply`))!
    expect((states.expected[2] as Record<string, unknown>).opacity).toBe(0.6)
  })
})
