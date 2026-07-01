# macOS host: testing Linux + Windows desktop builds via VMs

Goal: develop/test the Linux (Zig+GTK4) and future Windows (Zig+Win32+ConPTY,
riding the `ghostty-windows` fork) desktop apps entirely from a Mac, without
dual-booting or context-switching machines. On Apple Silicon, arm64 guest VMs
run at near-native speed via Apple's `Virtualization.framework` — the trick is
cross-compiling to arm64 for the fast local loop, and leaning on x86_64 CI as
the pre-release safety net (most real users are on x86_64).

## Why not Wine

Tried first, abandoned: `ghostty-windows` under Wine got far enough to load
OpenGL and spawn `cmd.exe` via ConPTY, but hit `error during resize
err=error.ResizeFailed` and exited cleanly right after — likely a Wine
window/display-geometry quirk (a known-weak area of Wine's Win32 coverage),
possibly also a genuine early-fork bug. Not worth chasing further; a real
Windows VM removes the ambiguity entirely.

## 1. Linux guest: Tart

[Tart](https://github.com/cirruslabs/tart) is a CLI-first VM manager built on
`Virtualization.framework`, designed to be scripted (used heavily in CI).

```bash
brew install cirruslabs/cli/tart

# Clone a base Linux image (arm64) from Tart's OCI registry
tart clone ghcr.io/cirruslabs/ubuntu:latest exp-linux-dev

# Boot it (headless, or drop --no-graphics to see the console)
tart run exp-linux-dev --no-graphics &

# Get its IP once booted
tart ip exp-linux-dev
```

Then install GTK4 dev deps + Zig inside the guest once, and from then on just
`scp`/`rsync` your cross-compiled binary in and `ssh` to run it:

```bash
zig build -Dtarget=aarch64-linux-gnu
scp zig-out/bin/exponential-linux admin@$(tart ip exp-linux-dev):~/app
ssh admin@$(tart ip exp-linux-dev) DISPLAY=:0 ./app
```

(For a real GUI window rather than headless, run Tart with graphics enabled,
or forward X11/Wayland — Tart's default Ubuntu image ships a desktop session
you can view via its own window when not passing `--no-graphics`.)

## 2. Windows guest: UTM (free — no yearly cost)

UTM is free/open-source and, for an arm64 guest on Apple Silicon, uses the
same `Virtualization.framework` backend Parallels does — comparable
performance, no subscription.

Difference vs. Parallels: no bundled Windows-on-Arm installer/license.
Download Microsoft's free **"Windows 11 on Arm Insider Preview VHDX"**
(built for exactly this VM use case) and import it as a new UTM VM. Catch:
it's an Insider build, so it needs reinstalling/updating roughly every ~90
days to stay activated — fine for a test rig, mildly annoying long-term.
UTM also needs TPM 2.0 + Secure Boot enabled in the VM config to satisfy
Win11's install requirements (toggle in UTM's VM settings before first boot).

```bash
brew install --cask utm   # or download from utmapp.org

utmctl start "Windows 11"
utmctl ip-address "Windows 11"
```

`utmctl` covers start/stop/list/ip scripting but — unlike Parallels'
`prlctl exec` — has no built-in guest-command-exec RPC. That's not actually a
problem: the SSH-based flow below is a guest-OS feature, not a
hypervisor feature, so it works identically under UTM, Parallels, or
anything else.

Inside the guest, once:

```powershell
# Enable OpenSSH Server (built into Win10/11) so the Mac can scp/ssh in
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Start-Service sshd
Set-Service -Name sshd -StartupType Automatic
```

Then from the Mac:

```bash
zig build -Dtarget=aarch64-windows-gnu
scp zig-out/bin/exponential.exe user@<vm-ip>:C:/apps/exponential.exe
ssh user@<vm-ip> "C:/apps/exponential.exe"
```

x86_64 Windows binaries also run under the guest's built-in x86 emulation for
quick functional checks, but prefer the native `aarch64-windows` build for
day-to-day iteration — no emulation-of-emulation.

(Parallels remains an option if the yearly cost is acceptable — it trades the
Insider-VHDX renewal hassle for a turnkey licensed installer and
`prlctl exec`, but isn't required for anything in this flow.)

## 3. Wiring into the existing run-config / play-button system

This should become one more run-target kind in `.exponential/config.json`
(alongside the existing preview/`command` targets), e.g. `vm-linux` /
`vm-windows`, pointing at a VM name + remote path. Selecting it from the play
button would:

1. Cross-compile the Zig binary for the guest arch.
2. `scp`/shared-folder it into the VM.
3. `ssh <vm> <binary>` to launch it.
4. Tee stdout/stderr back into a terminal-dock tab, same as local run configs.

Not built yet — this doc just covers the manual VM setup needed before that
integration is worth writing.

## Caveats

- OpenGL acceleration inside the Windows guest is the shakiest link in this
  chain — good enough to confirm ConPTY spawning + basic rendering
  correctness, not a reliable proxy for real GPU-driver behavior or
  performance. Do a final pass on real Windows hardware (or an x86_64 CI run)
  before shipping.
- Always validate against x86_64 in CI (`ubuntu-latest`, `windows-latest`)
  before release — arm64-in-VM is a dev-loop accelerant, not a substitute for
  testing the architecture your users actually run.
- `ghostty-windows` maturity is still unverified beyond "ConPTY spawn works,
  resize is flaky under Wine" — re-test the resize path on the real Windows
  VM before relying on the fork further.
