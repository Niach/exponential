---
title: Push-to-talk shortcuts
description: "Push-to-talk shortcuts on the voice device: what X11, Wayland, macOS and Windows allow, and what happens when a key can't be grabbed."
---

## What works where

Push to talk runs on the voice device's desktop app only. The shortcut is held: press to listen, release to send.

| | Keys | Mouse buttons |
|---|---|---|
| **Windows** | global | global |
| **Linux, X11** | global | global |
| **macOS** | global | no |
| **Linux, Wayland with the portal** | global, through the portal | no |
| **Linux, Wayland without it** | only while Exponential is focused | no |

The portal is xdg GlobalShortcuts: KDE 5.27+, GNOME 48+ and Hyprland have it. The compositor may bind another key; the shortcut then shows its label. On macOS, Ctrl + Space is also Select previous input source.

## The shortcuts

Record one in Settings → Assistant → Voice on the voice device, under On `{device}` (tray): click the row and press the key or combination. A letter, digit or most other keys need Ctrl, Alt, Shift or Super with them; these may stand alone:

| Binding | Where |
|---|---|
| F1 … F24 | everywhere |
| Insert, Pause, Scroll Lock, Print Screen | everywhere |
| Mouse button 3 … 9 | Windows, X11 |

## When a key can't be grabbed

A key that can't be grabbed always says so. The tray item reads Push to talk · Ctrl + Space unavailable, and Settings → Assistant → Voice says why under the Shortcut row:

| Why | What it says |
|---|---|
| Another app holds it | Another app holds `{shortcut}`. It works while Exponential is focused. |
| No portal | This desktop has no global shortcuts. `{shortcut}` works while Exponential is focused. |
| The portal said no | The desktop didn't allow `{shortcut}`. It works while Exponential is focused. |
| A mouse button | Mouse buttons can't be a shortcut here. Pick a key. |

In every case but the mouse button, the key still works while an Exponential window is focused.

## Holding it

Holding the key stops any reply being spoken, unless the microphone is muted. Mute microphone in the tray keeps it closed; the overlay then reads Microphone muted, Unmute in the tray to talk, with an Unmute pill. A key held for 120 s ends as if you let go.
