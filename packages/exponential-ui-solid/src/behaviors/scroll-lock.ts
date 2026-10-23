// FEED-131: react-remove-scroll-bar's body lock, as Radix's modal parts use it.

import { onCleanup } from "solid-js"

export const LOCK_ATTRIBUTE = `data-scroll-locked`
let lockSheets = 0
let lockSheet: HTMLStyleElement | null = null
/** react-remove-scroll-bar's body lock (the same sheet, the same counter
 *  attribute) while a modal overlay is up. */
export function useScrollLock() {
  const body = document.body
  const count = () => {
    const n = parseInt(body.getAttribute(LOCK_ATTRIBUTE) || `0`, 10)
    return isFinite(n) ? n : 0
  }
  body.setAttribute(LOCK_ATTRIBUTE, String(count() + 1))
  if (lockSheets === 0) {
    const cs = getComputedStyle(body)
    const parse = (x: string) => parseInt(x || ``, 10) || 0
    const [left, top, right] = [parse(cs.marginLeft), parse(cs.marginTop), parse(cs.marginRight)]
    const gap = Math.max(0, window.innerWidth - document.documentElement.clientWidth + right - left)
    const important = `!important`
    const css = `\n  .with-scroll-bars-hidden {\n   overflow: hidden ${important};\n   padding-right: ${gap}px ${important};\n  }\n  body[${LOCK_ATTRIBUTE}] {\n    overflow: hidden ${important};\n    overscroll-behavior: contain;\n    ${[`position: relative ${important};`, `\n    padding-left: ${left}px;\n    padding-top: ${top}px;\n    padding-right: ${right}px;\n    margin-left:0;\n    margin-top:0;\n    margin-right: ${gap}px ${important};\n    `].join(``)}\n  }\n  \n  .right-scroll-bar-position {\n    right: ${gap}px ${important};\n  }\n  \n  .width-before-scroll-bar {\n    margin-right: ${gap}px ${important};\n  }\n  \n  .right-scroll-bar-position .right-scroll-bar-position {\n    right: 0 ${important};\n  }\n  \n  .width-before-scroll-bar .width-before-scroll-bar {\n    margin-right: 0 ${important};\n  }\n  \n  body[${LOCK_ATTRIBUTE}] {\n    --removed-body-scroll-bar-size: ${gap}px;\n  }\n`
    lockSheet = document.createElement(`style`)
    lockSheet.type = `text/css`
    lockSheet.appendChild(document.createTextNode(css))
    ;(document.head || document.getElementsByTagName(`head`)[0]).appendChild(lockSheet)
  }
  lockSheets++
  onCleanup(() => {
    const n = count() - 1
    if (n <= 0) body.removeAttribute(LOCK_ATTRIBUTE)
    else body.setAttribute(LOCK_ATTRIBUTE, String(n))
    lockSheets--
    if (!lockSheets && lockSheet) {
      lockSheet.remove()
      lockSheet = null
    }
  })
}
