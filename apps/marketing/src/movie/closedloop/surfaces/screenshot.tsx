// closedloop/surfaces/screenshot.tsx — the reporter's annotated screenshot
// as an IMAGE (the widget uploads it and the issue description embeds it,
// SLOP-4): the acme.shop checkout page at its captured 1568×936 with the red
// rectangle the reporter dragged around "Pay now" (widgetmock's MARK,
// stroke strokeWidthFor(1568, 936) = 5). Scaled to the box it is drawn in.

import React from "react"
import { WIN } from "../../ships/theme"
import { CheckoutPage, PAGE_H, SITE_ANCHORS } from "./sitemock"

const PAY = SITE_ANCHORS.payRectPage
const MARK = { x: PAY.x - 10, y: PAY.y - 10, w: PAY.w + 20, h: PAY.h + 20 }
const STROKE = Math.max(3, Math.round(WIN.w * 0.0035))

export const SCREENSHOT_ASPECT = WIN.w / PAGE_H

export const AnnotatedScreenshot: React.FC<{ width: number }> = ({ width }) => {
  const s = width / WIN.w
  return (
    <div style={{ position: "relative", width, height: PAGE_H * s, overflow: "hidden" }}>
      <div
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          width: WIN.w,
          height: PAGE_H,
          transform: `scale(${s})`,
          transformOrigin: "0 0",
        }}
      >
        <CheckoutPage frame={0} />
        <div
          style={{
            position: "absolute",
            left: MARK.x,
            top: MARK.y,
            width: MARK.w,
            height: MARK.h,
            boxSizing: "border-box",
            border: `${STROKE}px solid #ef4444`,
          }}
        />
      </div>
    </div>
  )
}
