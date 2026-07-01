# Running and targeting the Windows + Linux UTM VMs

Companion to `docs/macos-setup-vm.md` — that doc covers initial VM setup;
this one covers the state we actually landed on after working through it,
and the exact commands to reach each VM from this Mac.

## Why Emulated VLAN instead of Shared Network

`docs/macos-setup-vm.md` assumes UTM's default **Shared Network** mode
(vmnet) gives the Mac direct host→guest reachability. On this machine
(macOS 27) it doesn't: host→guest SSH/ping time out completely while
guest→host and guest→internet work fine, with firewall/permissions ruled
out — this exactly matches a confirmed, currently-open upstream bug
([utmapp/UTM#6692](https://github.com/utmapp/UTM/issues/6692)) where macOS
silently rejects host-initiated connections into vmnet-backed guests,
regardless of backend (QEMU or Apple Virtualization) or network sub-mode
(Shared or Bridged — Bridged additionally doesn't work at all over Wi-Fi).

**Both VMs are configured with Network Mode → Emulated VLAN** instead (UTM
VM settings → Network). This uses QEMU's own userspace SLIRP networking,
a completely different code path from vmnet, which sidesteps the bug
entirely. The guest always gets `10.0.2.15` on this network; the tradeoff
is you must add an explicit **Port Forward** rule (same Network settings
page) per port you want to reach from the host:

| Protocol | Guest Address | Guest Port | Host Address | Host Port |
|---|---|---|---|---|
| TCP | 10.0.2.15 | 22 | 0.0.0.0 | 2222 (Windows) / 22222 (Linux) |

Network mode / port-forward changes require a full VM **stop + start**
(not just a guest-side reboot) to take effect.

## Windows VM

- UTM VM name: **Windows** (Windows 11 LTSC ARM64, QEMU/Emulate backend)
- SSH: `ssh -i ~/.ssh/exp_vm_windows -p 2222 niach@127.0.0.1`
- Windows account `niach` is an **Administrator**, so the public key lives in
  `C:\ProgramData\ssh\administrators_authorized_keys` (restricted ACLs via
  `icacls`), not the per-user `.ssh\authorized_keys`.

### Gotcha: Windows Firewall blocks it even with the right rule

The built-in `OpenSSH-Server-In-TCP` firewall rule that `Add-WindowsCapability`
creates is scoped to the **Private** profile only (`Profiles: 2`, i.e.
Domain=1/Private=2/Public=4 bitmask). The Emulated VLAN virtual adapter
comes up classified as **Public** by default, so the rule silently doesn't
apply and connections hang at the SSH banner exchange. Fix once per VM
(inside the guest):

```powershell
Get-NetConnectionProfile | Set-NetConnectionProfile -NetworkCategory Private
```

### Cross-compiling + shipping a Windows binary

Zig bundles mingw headers/import libs for `aarch64-windows-gnu`, so no
sysroot or Windows SDK is needed on the Mac:

```bash
zig build-exe hello.zig -target aarch64-windows-gnu --subsystem windows -O ReleaseSmall -femit-bin=hello.exe
scp -i ~/.ssh/exp_vm_windows -P 2222 hello.exe niach@127.0.0.1:C:/Users/niach/hello.exe
```

### Gotcha: GUI apps launched over SSH are invisible (Session 0)

`sshd.exe` runs as a Windows service in Session 0, so anything launched
directly via `ssh ... hello.exe` runs with no visible desktop — confirmed
via `tasklist` showing the process under session name `Services`. To get
it into your actual interactive desktop session, register a Scheduled Task
bound to interactive logon and trigger it remotely instead of running the
exe directly:

```bash
ssh -i ~/.ssh/exp_vm_windows -p 2222 niach@127.0.0.1 \
  "schtasks /create /tn HelloTest /tr C:\\Users\\niach\\hello.exe /sc onlogon /it /rl LIMITED /f"
ssh -i ~/.ssh/exp_vm_windows -p 2222 niach@127.0.0.1 "schtasks /run /tn HelloTest"
```

`tasklist /FI "IMAGENAME eq hello.exe"` afterward should show session name
`Console` (session 1) instead of `Services` — that's the signal it's
actually visible on screen.

## Linux VM

- UTM VM name: **Linux** (Ubuntu 26.04 LTS, arm64, GNOME/Wayland desktop,
  autologin as `niach`)
- SSH: `ssh -i ~/.ssh/exp_vm_linux -p 22222 niach@127.0.0.1`
- Passwordless sudo is enabled for `niach` via `/etc/sudoers.d/niach-nopasswd`
  (`echo "niach ALL=(ALL) NOPASSWD:ALL" | sudo tee /etc/sudoers.d/niach-nopasswd`)
  so build commands can run non-interactively over SSH.

### Building the real `apps/linux` app: natively in-guest, not cross-compiled

`apps/linux/build.zig` links GTK4/libadwaita/cairo/gdk-pixbuf and a
prebuilt `libghostty.so`, all resolved via the host's pkg-config/library
search. None of that is present on macOS, so **cross-compiling the real
app from the Mac isn't practical** without building a full aarch64-linux
sysroot. Building natively inside the guest sidesteps this entirely and
is fast since the guest is arm64-on-arm64 (no emulation). (The build used
to also pull in a Rust `agent-core` crate — that was removed in the v2
architecture cut; no Rust/cargo toolchain is needed anymore.)

One-time setup in the guest:

```bash
sudo apt-get update
sudo apt-get install -y build-essential git curl patchelf ncurses-bin pkg-config \
  libgtk-4-dev libadwaita-1-dev libcairo2-dev libgdk-pixbuf-2.0-dev \
  libsqlite3-dev libcurl4-openssl-dev libgl1-mesa-dev
curl -fsSL https://ziglang.org/download/0.16.0/zig-aarch64-linux-0.16.0.tar.xz -o /tmp/zig.tar.xz
sudo mkdir -p /opt/zig && sudo tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1
sudo ln -sf /opt/zig/zig /usr/local/bin/zig
```

Sync the source in (preserve the `apps/linux`, `packages/` directory
structure relative to repo root — `build.zig` uses `../../` relative
paths, so flattening breaks it):

```bash
rsync -az --exclude='.zig-cache' --exclude='.git' --exclude='node_modules' --exclude='vendor' \
  -e "ssh -i ~/.ssh/exp_vm_linux -p 22222" \
  apps/linux/ niach@127.0.0.1:~/exponential/apps/linux/
rsync -az -e "ssh -i ~/.ssh/exp_vm_linux -p 22222" packages/electric-protocol/ niach@127.0.0.1:~/exponential/packages/electric-protocol/
```

Build (webkit/X11 preview backends are optional — skip installing their
dev packages by disabling both):

```bash
ssh -i ~/.ssh/exp_vm_linux -p 22222 niach@127.0.0.1 \
  "cd ~/exponential/apps/linux && bash scripts/build-libghostty.sh && zig build -Dwebkit=false -Dx11=false"
```

### Running it with a visible window

Unlike the Windows session-0 problem, the Linux guest boots to a real
GNOME/Wayland desktop (autologin), and UTM's own console renders it
directly — no extra tooling needed to *see* it. The only wrinkle: an SSH
session isn't part of that graphical login session, so the display env
vars need to be picked up explicitly. Grab them once from the running
session (`gnome-shell`'s PID) and reuse:

```bash
ssh -i ~/.ssh/exp_vm_linux -p 22222 niach@127.0.0.1 \
  "sudo cat /proc/\$(pgrep -f 'gnome-shell --mode')/environ | tr '\0' '\n' | grep XDG_RUNTIME_DIR"
# XDG_RUNTIME_DIR=/run/user/1000 → wayland socket is /run/user/1000/wayland-0

ssh -i ~/.ssh/exp_vm_linux -p 22222 niach@127.0.0.1 \
  "cd ~/exponential/apps/linux && XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 \
   DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus DISPLAY=:0 \
   nohup ./zig-out/bin/exponential > /tmp/exp-app.log 2>&1 & disown"
```

Confirmed working end-to-end: real GTK4 window rendering the actual
Exponential sign-in screen, visible live in UTM's console.

## Inspecting either VM visually from the Mac side

No screen-sharing tool needed — plain macOS `screencapture` works against
the UTM window directly:

```bash
osascript -e 'tell application "UTM" to activate'
screencapture -x /tmp/vm-check.png
```

We tried adding [Peekaboo](https://github.com/openclaw/Peekaboo) for
programmatic clicking into the VM displays too, but confirmed it doesn't
actually work: UTM's guest display is a GPU-composited view, and synthetic
clicks report success at the macOS event-posting layer without ever
reaching the guest OS (no cursor movement, no state change, across many
varied attempts). Peekaboo was fully uninstalled. **Screenshots for visual
inspection: yes. Driving clicks into a VM: no — use SSH for that.**
