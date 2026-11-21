//! #EDE-146, #EDE-147, #EDE-145: the connect program. Attached to a space's slice (for
//! `connect()` and for a datagram's `sendto()`) it decides every connect of
//! a unix socket CREATED in the space (stream, seqpacket, datagram) by the
//! path string the caller passed, before the kernel looks the path up. It cannot follow links or `..`; it keeps
//! ordinary programs, whatever their environment, off the person's system
//! bus, sound servers, user manager, session bus, Wayland socket and eDE's own sockets.

use crate::ebpf::{Asm, Cond, Size, PROBE_READ_KERNEL, R0, R1, R2, R3, R6, R7, R8, R10};

/// The program's name in the kernel (`bpftool prog`).
pub const NAME: &str = "ede_connect";
/// The longest pathname a unix address holds.
pub const SUN_PATH: usize = 108;
/// The family of a unix address.
const AF_UNIX: i32 = 1;

/// Where `struct bpf_sock_addr_kern` keeps the caller's address and its
/// length (bytes; from the kernel's BTF: [`crate::btf`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub uaddr: u32,
    pub uaddrlen: u32,
}

/// One byte of a pattern: itself, or any digit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Byte {
    Is(u8),
    Digit,
}

fn pattern(text: &str) -> Vec<Byte> {
    text.bytes().map(|b| if b == b'#' { Byte::Digit } else { Byte::Is(b) }).collect()
}

fn matches(bytes: &[u8], pattern: &[Byte]) -> bool {
    bytes.len() == pattern.len()
        && bytes.iter().zip(pattern).all(|(b, p)| match p {
            Byte::Is(want) => b == want,
            Byte::Digit => b.is_ascii_digit(),
        })
}

/// What the program decides for one space: the person's user `uid`, and
/// `spaces`, the compositor's directory of spaces (its sockets stay
/// reachable; its name is the person's Wayland socket's,
/// [`crate::protocol::display_of`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub uid: u32,
    pub spaces: String,
}

/// A path with these at both `/run` and `/var/run` (a link to it).
fn both_runs(paths: &[String]) -> Vec<String> {
    paths.iter().flat_map(|p| [p.clone(), p.replacen("/run/", "/var/run/", 1)]).collect()
}

impl Policy {
    /// Exactly these paths are refused. [#146: the system bus; the
    /// person's session bus (their portals hand out camera streams);
    /// PID 1's private socket (what `sudo reboot` talks to)]
    pub fn exact(&self) -> Vec<String> {
        let u = self.uid;
        both_runs(&["/run/dbus/system_bus_socket".into(), format!("/run/user/{u}/bus"), "/run/systemd/private".into()])
    }

    /// Paths beginning with these are refused. [the person's user manager
    /// (no escape out of the slice, also not by `flatpak run`'s hard-coded
    /// socket); the proxies of the person's Flatpaks; eDE's control and
    /// services sockets (lock, log out, power); systemd's Varlink services
    /// (`sudo -i` would get a fresh login session); the person's tmux and
    /// screen servers (they run what they are sent outside the space);
    /// VPN daemons, wpa_supplicant and hostapd (`wpa_cli disconnect` for
    /// a member of `netdev`); the daemons whose group is root (docker,
    /// libvirt, lxd, incus) and a rootless docker of the person's; JACK2's
    /// servers, which a client tries when it finds no PulseAudio]
    pub fn prefixes(&self) -> Vec<String> {
        let u = self.uid;
        let mut list = both_runs(&[
            format!("/run/user/{u}/systemd/"),
            format!("/run/user/{u}/.dbus-proxy/"),
            format!("/run/user/{u}/exponential-desktop/"),
            format!("/run/user/{u}/docker.sock"),
            "/run/systemd/io.systemd.".into(),
            "/run/screen/".into(),
            "/run/tailscale/".into(),
            "/run/mullvad-vpn".into(),
            "/run/cloudflare-warp/".into(),
            "/run/wpa_supplicant/".into(),
            "/run/hostapd/".into(),
            "/run/docker.sock".into(),
            "/run/libvirt/".into(),
            "/run/incus/".into(),
        ]);
        list.extend([format!("/tmp/tmux-{u}/"), "/var/snap/lxd/".into(), "/var/lib/lxd/".into(), "/var/lib/incus/".into(), "/dev/shm/jack".into()]);
        list
    }

    /// Under `/run` (or `/var/run`), paths ending in these are refused:
    /// the person's sound servers by every default name, also as a snap's
    /// launch helper spells it (`/run/user/U/snap.x/../pulse/native`)
    /// [#147]; the person's Wayland socket by the name of the directory of
    /// spaces, `/run/user/U/wayland-1` for `wayland-1.spaces`, also as
    /// a snap's link to it (`/run/user/U/snap.x/wayland-1`, its desktop
    /// launcher's) and a Flatpak's bind of it (`/run/flatpak/wayland-1`)
    /// spell it [#145]. The space's own socket is in its directory
    /// (`…/space-n/wayland-0`) and passes; eDE's socket is never
    /// `wayland-0`, the name a space's Flatpak gives the space's own.
    pub fn ends(&self) -> Vec<String> {
        let mut ends: Vec<String> = ["/pulse/native", "/pipewire-#", "/pipewire-#-manager"].map(String::from).into();
        ends.extend(crate::protocol::display_of(&self.spaces).map(|name| format!("/{name}")));
        ends
    }

    /// The reference: does the program allow a connect to `path` (the
    /// bytes the kernel's address holds up to its length or first NUL)?
    pub fn allows(&self, path: &[u8]) -> bool {
        if path.is_empty() {
            return true; // unnamed, or abstract
        }
        if path.windows(4).any(|w| w == b"/../") {
            return false;
        }
        if path.starts_with(format!("{}/", self.spaces).as_bytes()) {
            return true;
        }
        if self.exact().iter().any(|e| path == e.as_bytes()) || self.prefixes().iter().any(|p| path.starts_with(p.as_bytes())) {
            return false;
        }
        let under_run = path.starts_with(b"/run/") || path.starts_with(b"/var/run/");
        !(under_run && self.ends().iter().any(|s| { let p = pattern(s); path.len() >= p.len() && matches(&path[path.len() - p.len()..], &p) }))
    }

    /// The program (`BPF_PROG_TYPE_CGROUP_SOCK_ADDR`, attach type
    /// `BPF_CGROUP_UNIX_CONNECT` or `_SENDMSG`): 1 = allow, 0 = deny
    /// (EPERM). `Err` = a layout it cannot read.
    pub fn program(&self, layout: Layout) -> Result<Vec<u8>, String> {
        // The stack: the kernel's struct at fp-184 (48 bytes), the address
        // at fp-136 (its family, then the path from fp-134; 120 bytes,
        // zeroed first, so the path ends in NULs).
        const CTX: i16 = -184;
        const ADDR: i16 = -136;
        const PATH: i16 = ADDR + 2;
        let ctx_len = (layout.uaddrlen + 4).max(layout.uaddr + 8) as i32;
        if ctx_len > 48 || layout.uaddr % 8 != 0 || layout.uaddrlen % 4 != 0 {
            return Err(format!("a layout the connect program cannot read: {layout:?}"));
        }
        let mut a = Asm::new();
        a.mov_reg(R6, R1);
        for off in (CTX..0).step_by(8) {
            a.store_imm(Size::Dw, R10, off, 0);
        }
        // The kernel's struct, then the address and its length.
        a.mov_reg(R1, R10).add(R1, CTX as i32).mov(R2, ctx_len).mov_reg(R3, R6).call(PROBE_READ_KERNEL).jump_if(Cond::Ne, R0, 0, "deny");
        a.load(Size::Dw, R7, R10, CTX + layout.uaddr as i16).load(Size::W, R8, R10, CTX + layout.uaddrlen as i16);
        // Two bytes of family; nothing after them = an unnamed address.
        // The address is read with its family: a family that is not
        // AF_UNIX means the layout was misread, and that is a refusal.
        a.jump_if(Cond::Lt, R8, 3, "allow").mov_reg(R2, R8).jump_if(Cond::Le, R2, 2 + SUN_PATH as i32, "sized").mov(R2, 2 + SUN_PATH as i32).label("sized");
        a.mov_reg(R1, R10).add(R1, ADDR as i32).mov_reg(R3, R7).call(PROBE_READ_KERNEL).jump_if(Cond::Ne, R0, 0, "deny");
        a.load(Size::H, R0, R10, ADDR).jump_if(Cond::Ne, R0, AF_UNIX, "deny");
        let byte = |a: &mut Asm, i: usize| {
            a.load(Size::B, R0, R10, PATH + i as i16);
        };
        // An abstract address (a NUL first) or none.
        byte(&mut a, 0);
        a.jump_if(Cond::Eq, R0, 0, "allow");
        // `/../` anywhere.
        for i in 0..=SUN_PATH - 4 {
            let next = format!("dots{i}");
            for (k, want) in b"/../".iter().enumerate() {
                byte(&mut a, i + k);
                a.jump_if(Cond::Ne, R0, *want as i32, &next);
            }
            a.goto("deny").label(&next);
        }
        // `bytes` from position `at`, then a NUL when `exact`.
        let compare = |a: &mut Asm, at: usize, bytes: &[Byte], exact: bool, matched: &str, label: &str| {
            let next = format!("not-{label}");
            for (k, want) in bytes.iter().enumerate() {
                byte(a, at + k);
                match want {
                    Byte::Is(b) => a.jump_if(Cond::Ne, R0, *b as i32, &next),
                    Byte::Digit => a.add(R0, -(b'0' as i32)).jump_if(Cond::Gt, R0, 9, &next),
                };
            }
            if exact && at + bytes.len() < SUN_PATH {
                byte(a, at + bytes.len());
                a.jump_if(Cond::Ne, R0, 0, &next);
            }
            a.goto(matched).label(&next);
        };
        compare(&mut a, 0, &pattern(&format!("{}/", self.spaces)), false, "allow", "spaces");
        for (n, path) in self.exact().iter().enumerate() {
            compare(&mut a, 0, &pattern(path), true, "deny", &format!("exact{n}"));
        }
        for (n, path) in self.prefixes().iter().enumerate() {
            compare(&mut a, 0, &pattern(path), false, "deny", &format!("prefix{n}"));
        }
        // By the path's end: only under /run.
        let ends = self.ends();
        if !ends.is_empty() {
            compare(&mut a, 0, &pattern("/run/"), false, "ends", "run");
            compare(&mut a, 0, &pattern("/var/run/"), false, "ends", "varrun");
            a.goto("allow").label("ends");
            for i in 0..SUN_PATH {
                byte(&mut a, i);
                a.jump_if(Cond::Eq, R0, 0, &format!("end{i}"));
            }
            a.goto(&format!("end{SUN_PATH}"));
            for end in 0..=SUN_PATH {
                a.label(&format!("end{end}"));
                for (n, suffix) in ends.iter().enumerate() {
                    let p = pattern(suffix);
                    if end >= p.len() {
                        compare(&mut a, end - p.len(), &p, false, "deny", &format!("end{end}-{n}"));
                    }
                }
                a.goto("allow");
            }
        }
        a.label("allow").ret(1).label("deny").mov(R0, 0).exit();
        Ok(a.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ebpf::Vm;

    const LAYOUT: Layout = Layout { uaddr: 8, uaddrlen: 32 };
    const ADDR: u64 = 0x2000_0000_0000;

    fn policy() -> Policy {
        Policy { uid: 1000, spaces: "/run/user/1000/exponential-desktop/wayland-1.spaces".into() }
    }

    /// The program's answer for a connect whose address holds `sun_path`
    /// and whose length is `len` (the family's two bytes included); the
    /// kernel's copy has `tail` after it.
    fn run(program: &[u8], sun_path: &[u8], len: u32, tail: u8) -> bool {
        let mut ctx = vec![0u8; 48];
        ctx[8..16].copy_from_slice(&ADDR.to_le_bytes());
        ctx[32..36].copy_from_slice(&len.to_le_bytes());
        let mut addr = vec![1u8, 0]; // AF_UNIX
        addr.extend(sun_path);
        addr.resize(128, tail);
        match Vm::new(ctx, vec![(ADDR, addr)]).run(program) {
            Ok(1) => true,
            Ok(0) => false,
            other => panic!("the program answered {other:?}"),
        }
    }

    /// Every way a client passes a path: with its NUL in the length
    /// (CPython, sd-bus), without it and garbage after (libdbus;
    /// io_uring's copy), the whole struct (libpulse), no NUL in 108 bytes.
    fn every_spelling(program: &[u8], path: &str) -> Vec<bool> {
        let p = path.as_bytes();
        let mut with_nul = p.to_vec();
        with_nul.push(0);
        let mut answers = vec![run(program, &with_nul, 2 + with_nul.len() as u32, 0xff), run(program, p, 2 + p.len() as u32, 0xff), run(program, &with_nul, 110, 0)];
        if p.len() < SUN_PATH {
            answers.push(run(program, p, 2 + p.len() as u32, b'x'));
        }
        answers
    }

    fn table() -> Vec<String> {
        let mut paths: Vec<String> = [
            // The person's: refused.
            "/run/dbus/system_bus_socket",
            "/var/run/dbus/system_bus_socket",
            "/run/user/1000/bus",
            "/run/user/1000/systemd/private",
            "/run/user/1000/systemd/io.systemd.Manager",
            "/run/user/1000/.dbus-proxy/session-bus-proxy-AB12",
            "/run/user/1000/exponential-desktop/wayland-1.sock",
            "/run/user/1000/exponential-desktop/services.sock",
            "/run/systemd/private",
            "/run/systemd/io.systemd.Login",
            "/run/systemd/io.systemd.Manager",
            "/tmp/tmux-1000/default",
            "/run/screen/S-niach/1234.pts-0.host",
            "/run/tailscale/tailscaled.sock",
            "/run/mullvad-vpn",
            "/run/cloudflare-warp/warp_service",
            "/dev/shm/jack_default_1000_0/jack_0",
            "/run/user/1000/pulse/native",
            "/run/user/1000/snap.firefox/../pulse/native",
            "/run/user/1000/snap.firefox/pulse/native",
            "/run/pulse/native",
            "/run/flatpak/pulse/native",
            "/run/user/1000/pipewire-0",
            "/run/user/1000/pipewire-0-manager",
            "/run/user/1000/pipewire-3",
            "/run/pipewire/pipewire-0",
            "/run/user/1000/exponential-desktop/wayland-1.spaces/../wayland-1.sock",
            "/run/user/1000/exponential-desktop/wayland-1.spaces/space-1/../../../bus",
            // #EDE-145: the person's Wayland socket, snaps' links to it,
            // a Flatpak's bind of it, spelt with `.` or `//`.
            "/run/user/1000/wayland-1",
            "/var/run/user/1000/wayland-1",
            "/run/user/1000/snap.firefox/wayland-1",
            "/run/user/1000/snap.firefox_beta/wayland-1",
            "/run/user/1000/snap.firefox/../wayland-1",
            "/run/flatpak/wayland-1",
            "/run/user/1000/./wayland-1",
            "/run/user/1000//wayland-1",
            // The space's own, and the rest: allowed.
            "/run/user/1000/exponential-desktop/wayland-1.spaces/space-1/bus",
            "/run/user/1000/exponential-desktop/wayland-1.spaces/space-1/wayland-0",
            "/run/user/1000/exponential-desktop/wayland-1.spaces/space-1/pulse/native",
            "/run/user/1000/exponential-desktop/wayland-1.spaces/space-2/systemd/private",
            "/run/user/1000/exponential-desktop/wayland-1.spaces/space-2/wayland-1",
            // Other Wayland sockets: a space's Flatpak's link to the
            // space's own socket (`wayland-0`), other names.
            "/run/user/1000/wayland-0",
            "/run/user/1000/wayland-10",
            "/run/user/1000/wayland-1.lock",
            "/run/user/1000/xwayland-1",
            "/run/user/1000/snap.firefox/wayland-0",
            "/run/flatpak/wayland-0",
            "/tmp/wayland-1",
            "/home/niach/wayland-1",
            "/run/user/1000/doc/socket",
            "/run/user/1000/pipewire-x",
            "/run/user/1000/pipewire-0x",
            "/run/user/1000/bus2",
            "/run/wpa_supplicant/wlan0",
            "/var/run/wpa_supplicant/wlan0",
            "/run/hostapd/wlan0",
            "/run/docker.sock",
            "/var/run/docker.sock",
            "/run/user/1000/docker.sock",
            "/run/libvirt/libvirt-sock",
            "/var/snap/lxd/common/lxd/unix.socket",
            "/var/lib/incus/unix.socket",
            "/run/dockerx.sock",
            "/var/snap/firefox/common/x",
            "/run/user/1001/bus",
            "/run/user/1001/systemd/private",
            "/run/systemd/journal/stdout",
            "/run/systemd/journal/socket",
            "/run/systemd/resolve/io.systemd.Resolve",
            "/run/systemd/userdb/io.systemd.DynamicUser",
            "/run/snapd.socket",
            "/run/cups/cups.sock",
            "/run/flatpak/bus",
            "/tmp/.X11-unix/X10",
            "/tmp/tmux-1001/default",
            "/tmp/my-test/pulse/native",
            "/home/niach/rig/pipewire-0",
            "/run/dbus/system_bus_socket2",
            "/run/dbus",
            "relative/pulse/native",
            "native",
            "/",
        ]
        .map(String::from)
        .to_vec();
        // As long as an address holds: the end of the path at every place.
        for len in [96, 100, 104, 105, 106, 107, 108] {
            for end in ["/pulse/native", "/pipewire-7", "/pipewire-0-manager", "/wayland-0", "/wayland-1"] {
                let mut path = String::from("/run/user/1000/snap.app");
                while path.len() + end.len() < len {
                    path.push('a');
                }
                path.push_str(end);
                paths.push(path);
            }
        }
        paths
    }

    #[test]
    fn the_program_answers_as_the_reference_for_every_path_and_spelling() {
        let policy = policy();
        let program = policy.program(LAYOUT).unwrap();
        for path in table() {
            let want = policy.allows(path.as_bytes());
            for (spelling, got) in every_spelling(&program, &path).into_iter().enumerate() {
                assert_eq!(got, want, "{path} (spelling {spelling}): the program says {got}, the reference {want}");
            }
        }
    }

    #[test]
    fn a_space_reaches_its_own_sockets_and_none_of_the_persons() {
        let policy = policy();
        for refused in ["/run/dbus/system_bus_socket", "/run/user/1000/snap.firefox/../bus", "/run/user/1000/wayland-1", "/run/user/1000/snap.firefox/wayland-1", "/run/user/1000/pulse/native", "/run/user/1000/snap.firefox/pulse/native", "/run/user/1000/pipewire-0", "/run/user/1000/pipewire-0-manager", "/run/pulse/native", "/dev/shm/jack_default_1000_0/jack_0", "/run/user/1000/systemd/private", "/run/user/1000/bus", "/run/user/1000/exponential-desktop/services.sock", "/tmp/tmux-1000/default", "/run/systemd/io.systemd.Login", "/run/systemd/private"] {
            assert!(!policy.allows(refused.as_bytes()), "{refused}");
        }
        for refused in ["/run/wpa_supplicant/wlan0", "/run/docker.sock", "/run/libvirt/libvirt-sock", "/var/snap/lxd/common/lxd/unix.socket"] {
            assert!(!policy.allows(refused.as_bytes()), "{refused}");
        }
        for allowed in ["/run/user/1000/exponential-desktop/wayland-1.spaces/space-1/bus", "/run/systemd/journal/stdout", "/run/snapd.socket", "/run/flatpak/bus", "/run/user/1000/wayland-0", "/tmp/.X11-unix/X10", "/tmp/rig/pulse/native"] {
            assert!(policy.allows(allowed.as_bytes()), "{allowed}");
        }
    }

    /// #EDE-145: the person's Wayland socket is the one the directory of
    /// spaces is named after: refused, also through a snap's link and a
    /// Flatpak's bind; the space's own socket, and every other name, pass.
    #[test]
    fn the_persons_wayland_socket_is_the_one_the_spaces_are_named_after() {
        let policy = policy();
        for refused in ["/run/user/1000/wayland-1", "/run/user/1000/snap.firefox/wayland-1", "/run/user/1000/snap.chromium/wayland-1", "/var/run/user/1000/wayland-1", "/run/flatpak/wayland-1"] {
            assert!(!policy.allows(refused.as_bytes()), "{refused}");
        }
        for allowed in ["/run/user/1000/exponential-desktop/wayland-1.spaces/space-1/wayland-0", "/run/user/1000/wayland-0", "/run/user/1000/wayland-2", "/run/user/1000/wayland-10", "/run/user/1000/snap.firefox/wayland-0", "/tmp/.X11-unix/X0"] {
            assert!(policy.allows(allowed.as_bytes()), "{allowed}");
        }
        // Another compositor's spaces: its own socket's name.
        let other = Policy { uid: 1000, spaces: "/run/user/1000/exponential-desktop/wayland-12.spaces".into() };
        let program = other.program(LAYOUT).unwrap();
        for (path, want) in [("/run/user/1000/wayland-12", false), ("/run/user/1000/snap.firefox/wayland-12", false), ("/run/user/1000/wayland-1", true), ("/run/user/1000/wayland-2", true), ("/run/user/1000/exponential-desktop/wayland-12.spaces/space-1/wayland-0", true)] {
            assert_eq!(other.allows(path.as_bytes()), want, "{path}");
            assert!(every_spelling(&program, path).into_iter().all(|got| got == want), "{path}: the program does not answer {want}");
        }
    }

    /// The longest directory of spaces the guard takes (the largest uid, a
    /// name of 64 bytes) still makes a program whose jumps reach (an eBPF
    /// jump spans at most 32767 instructions), and it answers as the
    /// reference.
    #[test]
    fn the_longest_directory_of_spaces_makes_a_program_that_answers_as_the_reference() {
        let name = format!("wayland-{}", "9".repeat(56));
        let policy = Policy { uid: u32::MAX, spaces: format!("/run/user/{}/exponential-desktop/{name}.spaces", u32::MAX) };
        assert_eq!(crate::protocol::spaces_dir_of(&policy.spaces, u32::MAX), Ok(()));
        let program = policy.program(LAYOUT).unwrap();
        let persons = [format!("/run/user/{}/{name}", u32::MAX), format!("/run/user/{}/snap.a/{name}", u32::MAX), format!("/run/user/{}/bus", u32::MAX)];
        assert!(persons.iter().all(|path| !policy.allows(path.as_bytes())), "{persons:?}");
        let mut paths = persons.to_vec();
        paths.push("/run/user/1000/wayland-1".to_string());
        paths.extend(table());
        for path in paths {
            let want = policy.allows(path.as_bytes());
            assert!(every_spelling(&program, &path).into_iter().all(|got| got == want), "{path}: the program does not answer {want}");
        }
    }

    /// A layout the program cannot read is refused when it is made; one
    /// read wrongly (the bytes found are not a unix address) refuses.
    #[test]
    fn a_misread_layout_fails_closed() {
        assert!(policy().program(Layout { uaddr: 12, uaddrlen: 32 }).is_err());
        assert!(policy().program(Layout { uaddr: 8, uaddrlen: 60 }).is_err());
        let program = policy().program(LAYOUT).unwrap();
        let mut ctx = vec![0u8; 48];
        ctx[8..16].copy_from_slice(&ADDR.to_le_bytes());
        ctx[32..36].copy_from_slice(&20u32.to_le_bytes());
        let mut addr = vec![2u8, 0];
        addr.extend(b"/tmp/allowed-path");
        addr.resize(128, 0);
        assert_eq!(Vm::new(ctx, vec![(ADDR, addr)]).run(&program), Ok(0), "a family that is not AF_UNIX was let through");
    }

    #[test]
    fn abstract_unnamed_and_unreadable_addresses() {
        let program = policy().program(LAYOUT).unwrap();
        // Unnamed (the family only) and abstract (a NUL first): allowed.
        assert!(run(&program, b"", 2, 0xff));
        assert!(run(&program, b"\0/run/user/1000/bus", 21, 0xff));
        // An address the program cannot read: refused.
        let mut ctx = vec![0u8; 48];
        ctx[8..16].copy_from_slice(&0x6666u64.to_le_bytes());
        ctx[32..36].copy_from_slice(&20u32.to_le_bytes());
        assert_eq!(Vm::new(ctx, vec![]).run(&program), Ok(0));
    }
}
