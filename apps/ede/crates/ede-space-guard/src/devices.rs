//! #EDE-148: the device program. Attached to a space's slice it decides
//! every `open()` and `mknod()` of a device node by a process of the space
//! (the kernel asks it for the OPENING task's cgroup, whoever owns the node
//! and whatever the node's ACL says: uaccess, the audio and video groups,
//! a snap's or a Flatpak's own device rules, root).

use crate::ebpf::{Asm, Cond, Size, R0, R1, R2, R3, R4, R5};

/// `BPF_DEVCG_DEV_CHAR`.
pub const CHAR: u32 = 2;
/// `BPF_DEVCG_DEV_BLOCK`.
pub const BLOCK: u32 = 1;
/// `BPF_DEVCG_ACC_*`.
pub const MKNOD: u32 = 1;
pub const READ: u32 = 2;
pub const WRITE: u32 = 4;

/// What a rule refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refuse {
    /// Every access: open for reading or writing, mknod.
    All,
    /// Opening for writing (reading stays: `rfkill list`, `lsusb -v`).
    Write,
}

/// One character device class a space may not use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    pub major: u32,
    /// `None` = every minor.
    pub minor: Option<u32>,
    pub refuse: Refuse,
    /// What it is, for the README and the doctor.
    pub what: &'static str,
}

/// The table of fixed majors. Everything else (the GPU, ttys, fuse, kvm,
/// /dev/null) is as before, but for [`DYNAMIC`].
pub const RULES: &[Rule] = &[
    Rule { major: 116, minor: None, refuse: Refuse::All, what: "sound (ALSA: every node in /dev/snd)" },
    Rule { major: 14, minor: None, refuse: Refuse::All, what: "sound (OSS: /dev/dsp, /dev/audio)" },
    Rule { major: 81, minor: None, refuse: Refuse::All, what: "cameras and video capture (V4L2: /dev/video*, /dev/v4l-subdev*; libcamera)" },
    Rule { major: 212, minor: None, refuse: Refuse::All, what: "TV capture (DVB)" },
    Rule { major: 13, minor: None, refuse: Refuse::All, what: "input devices (/dev/input)" },
    Rule { major: 10, minor: Some(223), refuse: Refuse::All, what: "virtual input (/dev/uinput: ydotool types into the person's session)" },
    Rule { major: 10, minor: Some(242), refuse: Refuse::Write, what: "radio switches (/dev/rfkill: rfkill block turns off Wi-Fi and Bluetooth)" },
    Rule { major: 89, minor: None, refuse: Refuse::All, what: "I2C (/dev/i2c-*: ddcutil switches monitors off)" },
    Rule { major: 189, minor: None, refuse: Refuse::Write, what: "raw USB (/dev/bus/usb: PTP cameras, libusb capture)" },
];

/// Devices whose major the kernel hands out at boot or module load, by
/// their name in the char section of `/proc/devices`: refused in full.
/// (WebAuthn keys and game controllers do not work in a space.)
pub const DYNAMIC: &[(&str, &str)] = &[
    ("hidraw", "HID raw reports (/dev/hidraw*: keyboards' and mice's input, security keys)"),
    ("usbmon", "USB traffic (/dev/usbmon*: USB keyboards, microphones, cameras)"),
];

/// [`RULES`] and the [`DYNAMIC`] majors this kernel has now (`proc_devices`
/// = the text of `/proc/devices`; a device whose module loads later is
/// covered from the guard's next start).
pub fn table(proc_devices: &str) -> Vec<Rule> {
    let mut rules = RULES.to_vec();
    let chars = proc_devices.split("Block devices:").next().unwrap_or_default();
    for line in chars.lines() {
        let mut words = line.split_whitespace();
        let (Some(major), Some(name)) = (words.next().and_then(|m| m.parse::<u32>().ok()), words.next()) else { continue };
        if let Some((_, what)) = DYNAMIC.iter().find(|(n, _)| *n == name) {
            if !rules.iter().any(|r| r.major == major) {
                rules.push(Rule { major, minor: None, refuse: Refuse::All, what });
            }
        }
    }
    rules
}

/// What the program of `table` answers: `true` = allowed. The reference
/// the program's bytes are tested against.
pub fn allowed(table: &[Rule], kind: u32, major: u32, minor: u32, access: u32) -> bool {
    if kind != CHAR {
        return true;
    }
    !table.iter().any(|rule| {
        rule.major == major
            && rule.minor.is_none_or(|m| m == minor)
            && match rule.refuse {
                Refuse::All => true,
                Refuse::Write => access & WRITE != 0,
            }
    })
}

/// The program's name in the kernel (`bpftool prog`).
pub const NAME: &str = "ede_devices";

/// The program (`BPF_PROG_TYPE_CGROUP_DEVICE`): reads `struct
/// bpf_cgroup_dev_ctx { u32 access_type; u32 major; u32 minor; }`, where
/// `access_type = access << 16 | type`; returns 1 = allow, 0 = deny.
pub fn program(table: &[Rule]) -> Vec<u8> {
    let mut asm = Asm::new();
    asm.load(Size::W, R2, R1, 0) // access_type
        .mov_reg(R3, R2)
        .and(R3, 0xffff) // the device type
        .rsh(R2, 16) // the access bits
        .jump_if(Cond::Ne, R3, CHAR as i32, "allow")
        .load(Size::W, R4, R1, 4) // major
        .load(Size::W, R5, R1, 8); // minor
    for (i, rule) in table.iter().enumerate() {
        let next = format!("next{i}");
        asm.jump_if(Cond::Ne, R4, rule.major as i32, &next);
        if let Some(minor) = rule.minor {
            asm.jump_if(Cond::Ne, R5, minor as i32, &next);
        }
        match rule.refuse {
            Refuse::All => asm.goto("deny"),
            Refuse::Write => asm.jump_if(Cond::Set, R2, WRITE as i32, "deny"),
        };
        asm.label(&next);
    }
    asm.label("allow").ret(1).label("deny").mov(R0, 0).exit();
    asm.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ebpf::Vm;

    /// A `/proc/devices` with hidraw and usbmon at this machine's majors.
    const PROC_DEVICES: &str = "Character devices:\n  1 mem\n  4 /dev/vc/0\n 10 misc\n 13 input\n 81 video4linux\n116 alsa\n189 usb_device\n226 drm\n237 usbmon\n238 hidraw\n239 media\n\nBlock devices:\n  8 sd\n237 hidraw\n";

    fn run(kind: u32, major: u32, minor: u32, access: u32) -> bool {
        run_with(&table(PROC_DEVICES), kind, major, minor, access)
    }

    fn run_with(table: &[Rule], kind: u32, major: u32, minor: u32, access: u32) -> bool {
        let mut ctx = Vec::new();
        ctx.extend((access << 16 | kind).to_le_bytes());
        ctx.extend(major.to_le_bytes());
        ctx.extend(minor.to_le_bytes());
        match Vm::new(ctx, vec![]).run(&program(table)) {
            Ok(1) => true,
            Ok(0) => false,
            other => panic!("the program answered {other:?}"),
        }
    }

    #[test]
    fn the_program_answers_as_the_table_for_every_device_and_access() {
        let accesses = [READ, WRITE, READ | WRITE, MKNOD];
        let mut checked = 0;
        for table in [table(PROC_DEVICES), table(""), RULES.to_vec()] {
            for kind in [CHAR, BLOCK] {
                for major in 0..300 {
                    for minor in [0, 1, 7, 223, 242, 255, 1024] {
                        for access in accesses {
                            assert_eq!(run_with(&table, kind, major, minor, access), allowed(&table, kind, major, minor, access), "type {kind} {major}:{minor} access {access}");
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 3 * 2 * 300 * 7 * 4);
    }

    /// The dynamic majors are read from the char section only, by name.
    #[test]
    fn hidraw_and_usbmon_take_their_majors_from_proc_devices() {
        let rules = table(PROC_DEVICES);
        let dynamic: Vec<(u32, &str)> = rules[RULES.len()..].iter().map(|r| (r.major, r.what.split(' ').next().unwrap())).collect();
        assert_eq!(dynamic, [(237, "USB"), (238, "HID")]);
        assert_eq!(table("Character devices:\n116 alsa\n").len(), RULES.len(), "none on a kernel without them");
        // This machine's, where it has the file.
        if let Ok(text) = std::fs::read_to_string("/proc/devices") {
            assert!(table(&text).len() >= RULES.len());
        }
    }

    #[test]
    fn a_space_has_no_microphone_camera_rfkill_or_virtual_keyboard_and_keeps_the_rest() {
        // The person's microphone and camera (#EDE-148), and what turns
        // off the network or types into the person's session (#EDE-146).
        for (major, minor, access) in [(116, 0, READ), (116, 3, READ | WRITE), (116, 33, READ), (14, 3, READ), (81, 0, READ | WRITE), (81, 7, READ), (10, 223, WRITE), (10, 242, WRITE), (189, 4, READ | WRITE), (89, 1, READ | WRITE), (13, 64, READ), (116, 0, MKNOD), (238, 0, READ), (237, 1, READ)] {
            assert!(!run(CHAR, major, minor, access), "{major}:{minor} access {access} is open to a space");
        }
        // The GPU, /dev/null, a tty, fuse, kvm, `rfkill list`, `lsusb -v`,
        // other misc devices, block devices.
        for (kind, major, minor, access) in [(CHAR, 226, 128, READ | WRITE), (CHAR, 1, 3, READ | WRITE), (CHAR, 5, 0, READ | WRITE), (CHAR, 136, 2, READ | WRITE), (CHAR, 10, 229, READ | WRITE), (CHAR, 10, 232, READ | WRITE), (CHAR, 10, 242, READ), (CHAR, 189, 4, READ), (BLOCK, 8, 0, READ)] {
            assert!(run(kind, major, minor, access), "{major}:{minor} access {access} is closed to a space");
        }
    }
}
