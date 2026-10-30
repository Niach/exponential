import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { voiceWave, voiceWavePath } from "@exponential_app/eui"
import { VoiceWave } from "./voice-wave"

// NEW-77: the VoiceWave primitive paints the core's rings (fixtures/
// voice-wave.json) around its centre child.

describe(`VoiceWave`, () => {
  it(`draws the three rings of the core at the size's edge, the child at its own size`, () => {
    const { container } = render(
      <VoiceWave activity="listening" level={0.6} bands={[0.9, 0.6, 1, 0.5]} size="lg" label="Voice">
        <b>mark</b>
      </VoiceWave>
    )
    const root = container.querySelector(`[data-slot=voice-wave]`)!
    expect(root.getAttribute(`role`)).toBe(`img`)
    expect(root.getAttribute(`aria-label`)).toBe(`Voice, listening`)
    const want = voiceWave({ size: 160, activity: `listening`, level: 0.6, bands: [0.9, 0.6, 1, 0.5] })
    const rings = container.querySelectorAll(`[data-slot=voice-wave-ring]`)
    expect(rings).toHaveLength(3)
    rings.forEach((ring, k) => {
      expect(ring.getAttribute(`d`)).toBe(voiceWavePath(want.rings[k]!))
      expect(ring.getAttribute(`fill-opacity`)).toBe(String(want.rings[k]!.fillAlpha))
    })
    const child = container.querySelector<HTMLElement>(`[data-slot=voice-wave-child]`)!
    // Laid out at its own size (no forced box); the scale is the painter's
    // (browser/voice-wave-child.test.ts measures it).
    expect(child.style.width).toBe(``)
    expect(child.getAttribute(`aria-hidden`)).toBeNull()
    expect(child.textContent).toBe(`mark`)
  })

  it(`is decorative without a label, quiet when muted, and keeps but hides the child at sm`, () => {
    const { container, rerender } = render(
      <VoiceWave activity="muted" size="sm">
        <b>mark</b>
      </VoiceWave>
    )
    const root = container.querySelector(`[data-slot=voice-wave]`)!
    expect(root.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(root.getAttribute(`role`)).toBeNull()
    expect(root.getAttribute(`class`)).toContain(`opacity-35`)
    const hidden = container.querySelector<HTMLElement>(`[data-slot=voice-wave-child]`)!
    expect(hidden.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(hidden.getAttribute(`class`)).toContain(`invisible`)
    rerender(
      <VoiceWave activity="speaking" level={1} size="md">
        <b>mark</b>
      </VoiceWave>
    )
    expect(root.getAttribute(`class`)).not.toContain(`opacity-35`)
    expect(container.querySelector(`[data-slot=voice-wave-child]`)!.getAttribute(`aria-hidden`)).toBeNull()
    const first = container.querySelector(`[data-slot=voice-wave-ring]`)!
    expect(first.getAttribute(`d`)).toBe(voiceWavePath(voiceWave({ size: 56, activity: `speaking`, level: 1 }).rings[0]!))
  })
})
