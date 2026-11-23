//! #EDE-157: the signal program. A space's processes are the person's
//! user, and the kernel lets a user signal all of its own processes: from
//! a space `pkill firefox` ends the person's Firefox, `killall Xwayland`
//! their X11 apps, `pkill -f exponential` their session. The guard
//! attaches an LSM program to the space's slice (`BPF_LSM_CGROUP` on
//! `bpf_lsm_task_kill`, Linux 6.5 or newer with `bpf` among the kernel's
//! LSMs): it runs for every signal a process of the slice SENDS (kill,
//! tkill, tgkill, pidfd_send_signal, rt_sigqueueinfo all ask
//! `security_task_kill`) and lets through an existence probe (signal 0)
//! and a signal to a process inside the same slice; every other one is
//! refused (`EPERM`). Signals the kernel sends (SIGCHLD, SIGPIPE) never ask.
//!
//! The slice is the program's: its cgroup id is in the program's bytes,
//! so each slice gets a program of its own.
//!
//! Not covered (on purpose): `fcntl(F_SETOWN)` + SIGIO (another hook), a
//! process that left its cgroup, signals from the person to a space.

use crate::ebpf::{Asm, Cond, Size, R0, R1, R2, R6, R7, R8};

/// The program's name (the kernel keeps 15 bytes of it).
pub const NAME: &str = "ede_signals";
/// The hook and the kernel functions, by their names in the kernel's BTF.
pub const FUNCS: [&str; 4] = ["bpf_lsm_task_kill", "bpf_cgroup_from_id", "bpf_task_under_cgroup", "bpf_cgroup_release"];

/// The BTF ids of [`FUNCS`] on this kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ids {
    pub hook: u32,
    pub cgroup_from_id: u32,
    pub task_under_cgroup: u32,
    pub cgroup_release: u32,
}

impl Ids {
    pub fn from_btf(btf: &[u8]) -> Result<Ids, String> {
        let ids = crate::btf::func_ids(btf, &FUNCS)?;
        Ok(Ids { hook: ids[0], cgroup_from_id: ids[1], task_under_cgroup: ids[2], cgroup_release: ids[3] })
    }
}

/// Does the kernel run BPF LSM programs (`/sys/kernel/security/lsm`'s
/// text, a comma-separated list)? Without `bpf` there, an attached
/// program is never asked.
pub fn bpf_lsm_active(lsm: &str) -> bool {
    lsm.trim().split(',').any(|name| name == "bpf")
}

/// The program for the slice of cgroup id `slice`. Its context is the
/// hook's arguments, 8 bytes each: `task_kill(struct task_struct *p,
/// struct kernel_siginfo *info, int sig, const struct cred *cred)`. It
/// answers 1 (let through) or 0 (refused: the kernel's `EPERM`).
pub fn program(ids: &Ids, slice: u64) -> Vec<u8> {
    let mut asm = Asm::new();
    asm.load(Size::Dw, R6, R1, 0) // the target task
        .load(Size::Dw, R7, R1, 16)
        .mov32_reg(R7, R7) // the signal (an int)
        .jump_if(Cond::Eq, R7, 0, "allow")
        .mov64(R1, slice)
        .call_kfunc(ids.cgroup_from_id)
        // The slice gone: nothing to be inside of.
        .jump_if(Cond::Eq, R0, 0, "deny")
        .mov_reg(R8, R0)
        .mov_reg(R1, R6)
        .mov_reg(R2, R8)
        .call_kfunc(ids.task_under_cgroup)
        .mov_reg(R7, R0)
        .mov_reg(R1, R8)
        .call_kfunc(ids.cgroup_release)
        .jump_if(Cond::Ne, R7, 0, "allow")
        .label("deny")
        .ret(0)
        .label("allow")
        .ret(1);
    asm.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ebpf::Vm;
    use std::{cell::RefCell, rc::Rc};

    const IDS: Ids = Ids { hook: 10, cgroup_from_id: 11, task_under_cgroup: 12, cgroup_release: 13 };
    /// The slice's cgroup id, and the kernel's pointer to it.
    const SLICE: u64 = 4242;
    const SLICE_PTR: u64 = 0xffff_8000_0000_1000;

    /// The hook's context: a task pointer, a siginfo, the signal (with
    /// junk above its 32 bits, as a register may carry it), a cred.
    fn ctx(task: u64, sig: i32) -> Vec<u8> {
        let mut ctx = Vec::new();
        for v in [task, 0xffff_8000_0000_2000, 0xdead_0000_0000_0000 | sig as u32 as u64, 0xffff_8000_0000_3000] {
            ctx.extend(v.to_le_bytes());
        }
        ctx
    }

    /// Runs the program on a model: `inside` = the tasks under the slice,
    /// `alive` = whether `bpf_cgroup_from_id` finds the slice. Its answer
    /// and the references left unreleased.
    fn run(task: u64, sig: i32, inside: &'static [u64], alive: bool) -> (u64, i32) {
        let held = Rc::new(RefCell::new(0));
        let h = held.clone();
        let vm = Vm::new(ctx(task, sig), vec![]).with_kfuncs(move |id, args| match id {
            11 => {
                assert_eq!(args[0], SLICE, "the slice's id is baked in");
                if alive {
                    *h.borrow_mut() += 1;
                    Ok(SLICE_PTR)
                } else {
                    Ok(0)
                }
            }
            12 => {
                assert_eq!(args[1], SLICE_PTR);
                Ok(inside.contains(&args[0]) as u64)
            }
            13 => {
                assert_eq!(args[0], SLICE_PTR);
                *h.borrow_mut() -= 1;
                Ok(0)
            }
            id => Err(format!("kfunc {id}")),
        });
        let mut vm = vm;
        let answer = vm.run(&program(&IDS, SLICE)).unwrap();
        let left = *held.borrow();
        (answer, left)
    }

    #[test]
    fn a_signal_to_a_process_of_the_space_passes_one_to_the_persons_is_refused() {
        const SPACE_TASK: u64 = 0xffff_8000_0001_0000;
        const PERSONS: u64 = 0xffff_8000_0002_0000;
        for sig in [libc::SIGTERM, libc::SIGKILL, libc::SIGHUP, libc::SIGSTOP, 64] {
            assert_eq!(run(SPACE_TASK, sig, &[SPACE_TASK], true), (1, 0), "signal {sig} inside the slice");
            assert_eq!(run(PERSONS, sig, &[SPACE_TASK], true), (0, 0), "signal {sig} to the person's process");
        }
        // `kill -0`: whether a process is there, never refused.
        assert_eq!(run(PERSONS, 0, &[], true), (1, 0));
        // The slice gone (the program outlives it only for a moment): refused.
        assert_eq!(run(PERSONS, libc::SIGTERM, &[], false), (0, 0));
    }

    #[test]
    fn the_kernels_lsm_list_names_bpf() {
        assert!(bpf_lsm_active("lockdown,capability,landlock,yama,apparmor,bpf,ima,evm\n"));
        assert!(bpf_lsm_active("bpf"));
        assert!(!bpf_lsm_active("lockdown,capability,landlock,yama,apparmor,ima,evm\n"));
        assert!(!bpf_lsm_active("bpffs,notbpf"));
        assert!(!bpf_lsm_active(""));
    }
}
