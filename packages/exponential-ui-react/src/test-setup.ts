// jsdom lacks the layout APIs Radix reads; stub them for the DOM suites.
class RO {
  observe() {}
  unobserve() {}
  disconnect() {}
}
if (typeof globalThis.ResizeObserver === `undefined`) (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = RO
if (typeof Element !== `undefined`) {
  if (!Element.prototype.scrollTo) Element.prototype.scrollTo = () => {}
  if (!Element.prototype.scrollIntoView) Element.prototype.scrollIntoView = () => {}
  if (!(Element.prototype as { hasPointerCapture?: unknown }).hasPointerCapture) (Element.prototype as unknown as { hasPointerCapture: () => boolean }).hasPointerCapture = () => false
  if (!(Element.prototype as { setPointerCapture?: unknown }).setPointerCapture) (Element.prototype as unknown as { setPointerCapture: () => void }).setPointerCapture = () => {}
  if (!(Element.prototype as { releasePointerCapture?: unknown }).releasePointerCapture) (Element.prototype as unknown as { releasePointerCapture: () => void }).releasePointerCapture = () => {}
}
