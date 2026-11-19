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

/// The table. Everything else (the GPU, ttys, fuse, kvm, /dev/null) is
/// as before.
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

/// What the program answers: `true` = allowed. The reference the
/// program's bytes are tested against.
pub fn allowed(kind: u32, major: u32, minor: u32, access: u32) -> bool {
    if kind != CHAR {
        return true;
    }
    !RULES.iter().any(|rule| {
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
pub fn program() -> Vec<u8> {
    let mut asm = Asm::new();
    asm.load(Size::W, R2, R1, 0) // access_type
        .mov_reg(R3, R2)
        .and(R3, 0xffff) // the device type
        .rsh(R2, 16) // the access bits
        .jump_if(Cond::Ne, R3, CHAR as i32, "allow")
        .load(Size::W, R4, R1, 4) // major
        .load(Size::W, R5, R1, 8); // minor
    for (i, rule) in RULES.iter().enumerate() {
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

    fn run(kind: u32, major: u32, minor: u32, access: u32) -> bool {
        let mut ctx = Vec::new();
        ctx.extend((access << 16 | kind).to_le_bytes());
        ctx.extend(major.to_le_bytes());
        ctx.extend(minor.to_le_bytes());
        match Vm::new(ctx, vec![]).run(&program()) {
            Ok(1) => true,
            Ok(0) => false,
            other => panic!("the program answered {other:?}"),
        }
    }

    #[test]
    fn the_program_answers_as_the_table_for_every_device_and_access() {
        let accesses = [READ, WRITE, READ | WRITE, MKNOD];
        let mut checked = 0;
        for kind in [CHAR, BLOCK] {
            for major in 0..300 {
                for minor in [0, 1, 7, 223, 242, 255, 1024] {
                    for access in accesses {
                        assert_eq!(run(kind, major, minor, access), allowed(kind, major, minor, access), "type {kind} {major}:{minor} access {access}");
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 2 * 300 * 7 * 4);
    }

    #[test]
    fn a_space_has_no_microphone_camera_rfkill_or_virtual_keyboard_and_keeps_the_rest() {
        // The person's microphone and camera (#EDE-148), and what turns
        // off the network or types into the person's session (#EDE-146).
        for (major, minor, access) in [(116, 0, READ), (116, 3, READ | WRITE), (116, 33, READ), (14, 3, READ), (81, 0, READ | WRITE), (81, 7, READ), (10, 223, WRITE), (10, 242, WRITE), (189, 4, READ | WRITE), (89, 1, READ | WRITE), (13, 64, READ), (116, 0, MKNOD)] {
            assert!(!run(CHAR, major, minor, access), "{major}:{minor} access {access} is open to a space");
        }
        // The GPU, /dev/null, a tty, fuse, kvm, `rfkill list`, `lsusb -v`,
        // other misc devices, block devices.
        for (kind, major, minor, access) in [(CHAR, 226, 128, READ | WRITE), (CHAR, 1, 3, READ | WRITE), (CHAR, 5, 0, READ | WRITE), (CHAR, 136, 2, READ | WRITE), (CHAR, 10, 229, READ | WRITE), (CHAR, 10, 232, READ | WRITE), (CHAR, 10, 242, READ), (CHAR, 189, 4, READ), (BLOCK, 8, 0, READ)] {
            assert!(run(kind, major, minor, access), "{major}:{minor} access {access} is closed to a space");
        }
    }
}
