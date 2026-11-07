# Doctor fixture: Linux Mint 22.3 with an RTX 4090 (driver 595.99.02, open kernel module)

What a read-only child session saw on Danny's Mint box on 2026-11-07 (#EDE-59),
laid out as the files the doctor reads under `/`: `proc/`, `sys/`, `etc/`, `lib/`,
`usr/`, `run/`, plus the outputs of the commands it runs
(`vulkaninfo-summary.txt`, `systemctl-is-enabled.txt`, `xwayland-version.txt`).
`Nvidia::read(root)` and `read_drm_cards(root/sys/class/drm)` read this tree in
`doctor::tests::the_mint_box_as_the_child_read_it`.

Facts: kernel 6.17, driver 595.99.02 from `nvidia-driver-595-open` (license
"Dual MIT/GPL", GSP firmware), `nvidia_drm modeset=1` in modprobe.d (the sysfs
parameters are root-only, so they are absent here), `fbdev` at its default,
`nvidia-drm_gbm.so` → `libnvidia-allocator.so.1`, `15_nvidia_gbm.json`,
`PreserveVideoMemoryAllocations=1` to `/var` with the sleep units enabled,
Xwayland 23.2.6 from the distro, Vulkan: NVIDIA first, then the Intel iGPU, then
lavapipe. The NVIDIA card is `card2` (pci 0000:01:00.0, `renderD129`) with two
monitors (DP-2 2560x1440, HDMI-A-3 3840x2160); `card1` is the Intel i915 iGPU
(`renderD128`) with nothing connected. The per-GPU file lives at
`proc/driver/nvidia/gpus/0000-01-00.0/information` (dashes: the real path
`gpus/0000:01:00.0` has colons, which Windows checkouts refuse; the doctor never
reads it). `boot_vga` was NOT read on the box: this
tree marks the NVIDIA card as the boot GPU; the rule for the other case is tested
in `the_primary_card_is_the_one_with_the_displays`.
