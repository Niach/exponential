// VAPP-85: the Box style whitelist, the VAPP-4 spike's rules as tests.

import { describe, expect, test } from "bun:test"
import styleJson from "../catalog/style.json" with { type: "json" }
import { STYLE_KEYS, create, props, validateStyle } from "./style"

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
    for (const key of [`zIndex`, `transition`, `transform`, `order`, `gridAutoFlow`, `justifyItems`]) expect(STYLE_KEYS).not.toContain(key)
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
    expect(validateStyle({ ":hover": { opacity: 1 } })[0].message).toBe(`not in the Box style whitelist`)
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
