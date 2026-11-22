//! #EXP-1967: a test process killed by SIGILL / SIGSEGV / SIGBUS / SIGFPE
//! leaves no panic text (libtest never sees it), and the one SIGILL we have
//! (`render`, release-suite on ios-v0.14.138) faulted at an address without
//! unwind info. So the handler prints what names the fault afterwards: the
//! signal, the faulting `rip` and `si_addr`, the stack words around `rsp`
//! (return-address candidates for a manual backtrace) and `/proc/self/maps`
//! (the binary's load base). With the test binary the failed CI step keeps
//! as an artifact, `addr2line -e <binary> <rip - base>` names the frame.
//! Then the handler that was there before takes the fault again (Rust's
//! stack-overflow report for SIGSEGV, else the default: the core dump, the
//! same exit code).
//!
//! Only raw `write(2)` / `open(2)` / `read(2)` in the handler: nothing
//! that allocates or locks (async-signal-safe). Its buffers are statics:
//! it runs on the small alternate signal stack (a 4 KiB buffer there
//! overflowed it).

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod imp {
    use std::sync::Once;

    const SIGNALS: [libc::c_int; 4] = [libc::SIGILL, libc::SIGSEGV, libc::SIGBUS, libc::SIGFPE];
    static INSTALL: Once = Once::new();
    /// The actions before ours, per `SIGNALS` index (written once, before
    /// any handler can run).
    static mut PREVIOUS: [std::mem::MaybeUninit<libc::sigaction>; 4] = [std::mem::MaybeUninit::zeroed(); 4];
    /// The handler's buffers (one fault reports at a time; a second one
    /// re-enters only after the first restored the previous action).
    static mut MAPS: [u8; 4096] = [0; 4096];
    static mut WORDS: [u64; 64] = [0; 64];

    pub fn install() {
        INSTALL.call_once(|| unsafe {
            for (i, sig) in SIGNALS.into_iter().enumerate() {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = handler as *const () as usize;
                // ONSTACK: a stack overflow's SIGSEGV still gets a stack
                // (std installs the alternate stack per thread).
                action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(sig, &action, (*std::ptr::addr_of_mut!(PREVIOUS))[i].as_mut_ptr());
            }
        });
    }

    fn out(bytes: &[u8]) {
        let mut rest = bytes;
        while !rest.is_empty() {
            let n = unsafe { libc::write(2, rest.as_ptr().cast(), rest.len()) };
            if n <= 0 {
                return;
            }
            rest = &rest[n as usize..];
        }
    }

    fn hex(label: &[u8], value: u64) {
        hex_word(label, value);
        out(b"\n");
    }

    fn hex_word(label: &[u8], value: u64) {
        let mut buf = [0u8; 18];
        buf[0] = b'0';
        buf[1] = b'x';
        for i in 0..16 {
            let nibble = ((value >> ((15 - i) * 4)) & 0xf) as u8;
            buf[2 + i] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
        }
        out(label);
        out(&buf);
    }

    extern "C" fn handler(sig: libc::c_int, info: *mut libc::siginfo_t, context: *mut libc::c_void) {
        unsafe {
            out(b"\n=== #EXP-1967 crash report ===\n");
            hex(b"signal ", sig as u64);
            if !info.is_null() {
                hex(b"si_code ", (*info).si_code as u64);
                hex(b"si_addr ", (*info).si_addr() as u64);
            }
            if !context.is_null() {
                let uc = &*(context as *const libc::ucontext_t);
                let regs = &uc.uc_mcontext.gregs;
                let rip = regs[libc::REG_RIP as usize] as u64;
                let rsp = regs[libc::REG_RSP as usize] as u64;
                hex(b"rip ", rip);
                hex(b"rsp ", rsp);
                hex(b"rbp ", regs[libc::REG_RBP as usize] as u64);
                // The words at rip (an intentional `ud2` = 0x0b0f) and the
                // stack's first 64 words (the return-address candidates).
                // Only read memory /proc/self/maps would show readable: the
                // fault may be a wild jump, so read through `process_vm_readv`
                // on ourselves (EFAULT instead of a nested fault).
                peek(b"code@rip ", rip, 2);
                peek(b"stack ", rsp, 64);
            }
            out(b"--- /proc/self/maps ---\n");
            let fd = libc::open(b"/proc/self/maps\0".as_ptr().cast(), libc::O_RDONLY);
            if fd >= 0 {
                let buf = &mut *std::ptr::addr_of_mut!(MAPS);
                loop {
                    let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
                    if n <= 0 {
                        break;
                    }
                    out(&buf[..n as usize]);
                }
                libc::close(fd);
            }
            out(b"=== end of crash report ===\n");
            // Back to the previous action; returning re-runs the faulting
            // instruction, which now reaches it.
            if let Some(i) = SIGNALS.iter().position(|s| *s == sig) {
                libc::sigaction(sig, (*std::ptr::addr_of!(PREVIOUS))[i].as_ptr(), std::ptr::null_mut());
            }
            if !matches!(sig, libc::SIGILL | libc::SIGSEGV | libc::SIGBUS | libc::SIGFPE) || info.is_null() || (*info).si_code <= 0 {
                // Sent, not a fault (kill/raise): nothing re-runs, so re-send it.
                libc::raise(sig);
            }
        }
    }

    unsafe fn peek(label: &[u8], at: u64, words: usize) {
        let buf = &mut *std::ptr::addr_of_mut!(WORDS);
        let words = words.min(buf.len());
        let local = libc::iovec { iov_base: buf.as_mut_ptr().cast(), iov_len: words * 8 };
        let remote = libc::iovec { iov_base: at as *mut libc::c_void, iov_len: words * 8 };
        let n = libc::process_vm_readv(libc::getpid(), &local, 1, &remote, 1, 0);
        if n <= 0 {
            out(label);
            out(b"unreadable\n");
            return;
        }
        for (i, word) in buf[..(n as usize / 8)].iter().enumerate() {
            out(label);
            hex_word(b"+", (i * 8) as u64);
            hex(b" = ", *word);
        }
    }
}

/// Installs the handler once per process (a no-op off Linux x86_64).
pub fn install() {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    imp::install();
}
