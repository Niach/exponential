//! VAPP-124: how much stack the current thread has left, for [`crate::deep`]
//! and [`crate::roomy`].
//!
//! `stacker` knows the host thread's stack limit on Linux, macOS and
//! Windows but has no Android or iOS backend: there `remaining_stack()` is
//! `None` and `maybe_grow` ALWAYS switches to a heap segment, so every
//! pass ran off the host stack and ART threw StackOverflowError at the
//! first JNA upcall from it (the formatter, the measurer). On those
//! targets the host thread's limit comes from pthread instead (bionic's
//! `pthread_getattr_np`, Darwin's `pthread_get_stackaddr_np`), probed once
//! per thread; inside a segment `stacker` knows the limit itself.

/// Run `f` on the current stack when at least `red_zone` bytes are left,
/// else on a fresh `stack_size` segment.
#[inline]
pub(crate) fn maybe_grow<R>(red_zone: usize, stack_size: usize, f: impl FnOnce() -> R) -> R {
    match remaining() {
        Some(left) if left >= red_zone => f(),
        _ => stacker::grow(stack_size, f),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios", test)))]
#[inline]
fn remaining() -> Option<usize> {
    stacker::remaining_stack()
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
#[inline]
fn remaining() -> Option<usize> {
    stacker::remaining_stack().or_else(|| host_limit().map(|limit| stack_pointer().saturating_sub(limit)))
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
#[inline(always)]
fn stack_pointer() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// The lowest usable address of the host thread's stack, probed once.
#[cfg(any(target_os = "android", target_os = "ios", test))]
fn host_limit() -> Option<usize> {
    use std::cell::Cell;
    thread_local! {
        static LIMIT: Cell<Option<Option<usize>>> = const { Cell::new(None) };
    }
    LIMIT.with(|cell| {
        if let Some(known) = cell.get() {
            return known;
        }
        let probed = probe();
        cell.set(Some(probed));
        probed
    })
}

#[cfg(target_os = "android")]
fn probe() -> Option<usize> {
    // SAFETY: an initialised attr filled for the calling thread, destroyed
    // before return; bionic resolves the main thread's stack from
    // /proc/self/maps and a pthread's from its own mapping.
    unsafe {
        let mut attr = std::mem::MaybeUninit::<libc::pthread_attr_t>::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), attr.as_mut_ptr()) != 0 {
            return None;
        }
        let mut addr = std::ptr::null_mut();
        let mut size = 0;
        let mut guard = 0;
        let ok = libc::pthread_attr_getstack(attr.as_ptr(), &mut addr, &mut size) == 0
            && libc::pthread_attr_getguardsize(attr.as_ptr(), &mut guard) == 0;
        libc::pthread_attr_destroy(attr.as_mut_ptr());
        ok.then(|| addr as usize + guard)
    }
}

#[cfg(any(target_os = "ios", all(test, target_vendor = "apple")))]
fn probe() -> Option<usize> {
    // SAFETY: plain queries about the calling thread.
    unsafe {
        let me = libc::pthread_self();
        let top = libc::pthread_get_stackaddr_np(me) as usize;
        Some(top - libc::pthread_get_stacksize_np(me))
    }
}

#[cfg(all(test, not(any(target_os = "android", target_vendor = "apple"))))]
fn probe() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_probe_brackets_the_current_frame() {
        let limit = host_limit().expect("the host thread's stack limit");
        let sp = stack_pointer();
        assert!(sp > limit, "the frame lies above the limit");
        if let Some(stacker_says) = stacker::remaining_stack() {
            // Both see the same stack within a page or two.
            assert!(stacker_says.abs_diff(sp - limit) < 64 * 1024);
        }
    }

    #[test]
    fn a_roomy_thread_runs_on_its_own_stack() {
        let sp = stack_pointer();
        let inner = maybe_grow(64 * 1024, 1024 * 1024, stack_pointer);
        assert!(sp.abs_diff(inner) < 64 * 1024, "no segment switch");
    }

    #[test]
    fn a_tight_thread_grows_and_the_segment_reports_itself() {
        let sp = stack_pointer();
        let (inner, left) = maybe_grow(usize::MAX / 2, 1024 * 1024, || (stack_pointer(), remaining()));
        assert!(sp.abs_diff(inner) > 64 * 1024, "ran on a segment");
        let left = left.expect("stacker knows its own segment");
        assert!(left > 512 * 1024 && left <= 1024 * 1024 + 64 * 1024);
    }
}
