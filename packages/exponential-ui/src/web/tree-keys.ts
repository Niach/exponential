// #VAPP-285: the Tree keys in the DOM, shared by the React and Solid Box:
// `treeKey` (src/tree-key.ts) over the visible `[role=treeitem]` rows of the
// nearest `[role=tree]`, read off their aria-level / aria-expanded.

import { treeKey, type TreeKeyRow } from "../tree-key"

/** What a key on a treeitem does: `focus` = the row to focus, `toggle` =
 *  fold it (the Box's `toggle`, the fold keys), `none` = handled, nothing;
 *  `undefined` = not a treeitem (or not a tree key). */
export function treeKeyTarget(item: HTMLElement, key: string, rtl: boolean): { focus: HTMLElement } | `toggle` | `none` | undefined {
  if (item.getAttribute(`role`) !== `treeitem`) return undefined
  if (![`ArrowDown`, `ArrowUp`, `Home`, `End`, `ArrowLeft`, `ArrowRight`].includes(key)) return undefined
  const tree = item.closest(`[role="tree"]`)
  if (!tree) return undefined
  const items = [...tree.querySelectorAll<HTMLElement>(`[role="treeitem"]`)].filter((el) => el.closest(`[role="tree"]`) === tree)
  const rows: TreeKeyRow[] = items.map((el) => {
    const expanded = el.getAttribute(`aria-expanded`)
    return { level: Number(el.getAttribute(`aria-level`) ?? 1) || 1, ...(expanded === `true` || expanded === `false` ? { expanded: expanded === `true` } : {}) }
  })
  const result = treeKey(rows, items.indexOf(item), key, rtl)
  if (!result) return `none`
  if (`focus` in result) return { focus: items[result.focus]! }
  return `toggle`
}
