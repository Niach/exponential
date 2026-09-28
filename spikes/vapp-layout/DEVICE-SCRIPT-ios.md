# VAPP-4 · iOS device script

About 15 minutes on your iPhone. The simulator numbers are in `FINDINGS-ios.md`.
The screen needs no backend: with nobody signed in it opens at the root, and
when you are signed in it is pushed on top of the app.

## 1. Build and run

1. `cd apps/ios && tuist install && tuist generate --no-open`
2. `open Exponential.xcworkspace`, then scheme **Exponential**, destination = your iPhone.
3. Product > Scheme > Edit Scheme > Run > Arguments > "Arguments Passed On Launch", add these three rows:
   - `-uiTesting`
   - `-uiTestingScreen`
   - `kitchen-sink`
4. Build and run with Cmd-R, in **Release** (Edit Scheme > Run > Info > Build Configuration = Release) so the timings are real. In the simulator, Debug ran about 1.3 to 1.6 times slower than Release (the Rust core is a release build either way).

For the bench tree, change the third row to `kitchen-sink-bench`. For RTL, add a row `-uiTestingRTL`.

## 2. Bench readings (5 each)

The caption at the top reads `{n} nodes · {calls} calls · taffy {µs} µs · wall {µs} µs`.

1. With `kitchen-sink`: write down the caption once (this is the first pass).
2. Tap **Re-layout** 5 times and write down the caption after each tap. Each tap is a COLD pass on a fresh Surface, so every leaf gets measured again.
3. Tap **Warm** 5 times and write down the caption after each tap. Each tap is a forced pass on the same Surface, where taffy's cache answers.
4. Repeat steps 1 to 3 with `kitchen-sink-bench` (203 nodes).

The Xcode console also prints one `VappSpike pass=…` line per pass (`measureCalls`, `layoutNs`, `wallNs`, `buildNs`).

## 3. VoiceOver order

1. Settings > Accessibility > VoiceOver > On (or triple-click the side button if you have that shortcut set).
2. Launch the kitchen sink and touch the "Reddit radar" title once.
3. Swipe right one element at a time down to the bottom of the screen. VoiceOver scrolls for you. Write down every spoken element.
4. Expected order (the fixture's pre-order; the avatar and the divider are hidden on purpose):
   1. Reddit radar
   2. Kitchen sink · one taffy layout on every client
   3. 3
   4. Scan now (button)
   5. Sources
   6. r/selfhosted
   7. r/opensource
   8. r/webdev
   9. Auto-scan (toggle, on)
   10. Drafts
   11. Cover (image)
   12. LIVE
   13. Looking for a Linear alternative, self-hostable, with agents. Draft reply ready. 2 sources cited
   14. Progress
   15. All, Drafts, Sent, Archived (buttons)
   16. More (button)
   17. Draft reply
   18. Title (echoes after 150 ms) (text field)
   19. host:
   20. Body (text field)
   21. r/selfhosted (the select)
   22. Cancel, Send (buttons)
   23. basis 30% · grow 1
   24. 160 · shrink 0
   25. grow 2 · max 50%
   26. 25%
5. The bench bar (caption, A11y, Re-layout, Warm) comes before item 1. That is the screen chrome, not the surface.
6. To get the app's own walk for comparison: tap **A11y** and copy the `VappSpike a11y [...]` line from the Xcode console.

## 4. Typing test by hand

1. Tap the field "Title (echoes after 150 ms)".
2. Software keyboard: type `abcdefghijklmnopqrstuvwxyz0123456789ABCD` as fast as you can. Hardware keyboard (Magic Keyboard, or the Mac keyboard in the simulator): the same string, typed at full speed.
3. Stop typing. After about half a second, check two things: the field shows exactly the 40 characters (none lost, none doubled, cursor at the end), and the muted line below it reads `host: ` followed by the same 40 characters.
4. Also try deleting in the middle of the text and typing there. The cursor must not jump to the end when the echo lands.
5. Note pass or fail for each keyboard.
