// EXP-1635 (composer-menu.json `addFile`): one hidden input per choice. Any
// file and many at once by default, images for Photos, the rear camera and
// one shot for Camera.
import { createRef } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  ComposerInlineTools,
  type ComposerInlineToolsHandle,
} from "@/components/composer-inline-tools"

const input = (testId: string) => screen.getByTestId(testId) as HTMLInputElement

describe(`ComposerInlineTools`, () => {
  it(`the file input takes any file, many at once`, () => {
    render(<ComposerInlineTools onFiles={vi.fn()} />)
    expect(input(`composer-file-input`).multiple).toBe(true)
    expect(input(`composer-file-input`).hasAttribute(`accept`)).toBe(false)
    expect(input(`composer-file-input`).hasAttribute(`capture`)).toBe(false)
  })

  it(`the photo input takes images, many at once, never the camera directly`, () => {
    render(<ComposerInlineTools onFiles={vi.fn()} />)
    expect(input(`composer-photo-input`).accept).toBe(`image/*`)
    expect(input(`composer-photo-input`).multiple).toBe(true)
    expect(input(`composer-photo-input`).hasAttribute(`capture`)).toBe(false)
  })

  it(`the camera input is one shot on the rear camera`, () => {
    render(<ComposerInlineTools onFiles={vi.fn()} />)
    expect(input(`composer-camera-input`).accept).toBe(`image/*`)
    expect(input(`composer-camera-input`).getAttribute(`capture`)).toBe(`environment`)
    expect(input(`composer-camera-input`).multiple).toBe(false)
  })

  it.each([
    [undefined, `composer-file-input`],
    [`file`, `composer-file-input`],
    [`photos`, `composer-photo-input`],
    [`camera`, `composer-camera-input`],
  ] as const)(`attach(%s) clicks %s`, (choice, testId) => {
    const ref = createRef<ComposerInlineToolsHandle>()
    render(<ComposerInlineTools ref={ref} onFiles={vi.fn()} />)
    const click = vi.spyOn(input(testId), `click`)
    ref.current?.attach(choice)
    expect(click).toHaveBeenCalledOnce()
  })

  it(`a pick from any input reaches the composer`, () => {
    const onFiles = vi.fn()
    render(<ComposerInlineTools onFiles={onFiles} />)
    const file = new File([`x`], `a.png`, { type: `image/png` })
    fireEvent.change(input(`composer-camera-input`), { target: { files: [file] } })
    expect(onFiles).toHaveBeenCalledWith([file])
  })
})
