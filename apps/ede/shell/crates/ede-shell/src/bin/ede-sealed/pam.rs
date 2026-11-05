//! PAM through the system's `libpam.so.0`, opened at run time (no
//! libpam headers or a `-dev` package to build against): `pam_start`,
//! `pam_authenticate`, `pam_acct_mgmt`, `pam_end` with a conversation that
//! answers every prompt with the password. The service is `ede` when
//! `/etc/pam.d/ede` exists, else `login`.

use std::ffi::{c_char, c_int, c_void, CStr, CString};

const PAM_SUCCESS: c_int = 0;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

#[repr(C)]
struct PamConv {
    conv: Option<unsafe extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int>,
    appdata_ptr: *mut c_void,
}

type PamStart = unsafe extern "C" fn(*const c_char, *const c_char, *const PamConv, *mut *mut c_void) -> c_int;
type PamAuthenticate = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type PamAcctMgmt = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type PamEnd = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type PamStrerror = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;

/// Answers prompts with the password `appdata` points at (a `CString`).
unsafe extern "C" fn conversation(n: c_int, messages: *mut *const PamMessage, responses: *mut *mut PamResponse, appdata: *mut c_void) -> c_int {
    if n <= 0 || messages.is_null() || responses.is_null() || appdata.is_null() {
        return PAM_CONV_ERR;
    }
    let password = &*(appdata as *const CString);
    // SAFETY: libpam frees each `resp` and the array with `free`.
    let out = libc::calloc(n as usize, std::mem::size_of::<PamResponse>()) as *mut PamResponse;
    if out.is_null() {
        return PAM_BUF_ERR;
    }
    for i in 0..n as usize {
        // Linux-PAM: an array of pointers to messages.
        let message: *const PamMessage = *messages.add(i);
        let style = (*message).msg_style;
        let reply = &mut *out.add(i);
        if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
            reply.resp = libc::strdup(password.as_ptr());
        }
    }
    *responses = out;
    PAM_SUCCESS
}

/// `Ok(())` when `user` authenticates with `password`; `Err(message)` else.
pub fn authenticate(user: &str, password: &str) -> Result<(), String> {
    let lib = unsafe { libloading::Library::new("libpam.so.0") }.map_err(|e| format!("libpam: {e}"))?;
    let (start, auth, acct, end, strerror) = unsafe {
        (
            *lib.get::<PamStart>(b"pam_start\0").map_err(|e| e.to_string())?,
            *lib.get::<PamAuthenticate>(b"pam_authenticate\0").map_err(|e| e.to_string())?,
            *lib.get::<PamAcctMgmt>(b"pam_acct_mgmt\0").map_err(|e| e.to_string())?,
            *lib.get::<PamEnd>(b"pam_end\0").map_err(|e| e.to_string())?,
            *lib.get::<PamStrerror>(b"pam_strerror\0").map_err(|e| e.to_string())?,
        )
    };
    let service = if std::path::Path::new("/etc/pam.d/ede").exists() { "ede" } else { "login" };
    let service = CString::new(service).unwrap();
    let user_c = CString::new(user).map_err(|_| "bad user name")?;
    let password = Box::new(CString::new(password).map_err(|_| "a password cannot contain NUL")?);
    let conv = PamConv { conv: Some(conversation), appdata_ptr: &*password as *const CString as *mut c_void };
    let mut handle: *mut c_void = std::ptr::null_mut();
    let message = |handle: *mut c_void, code: c_int| -> String {
        let p = unsafe { strerror(handle, code) };
        if p.is_null() { format!("PAM error {code}") } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
    };
    let rc = unsafe { start(service.as_ptr(), user_c.as_ptr(), &conv, &mut handle) };
    if rc != PAM_SUCCESS {
        return Err(message(std::ptr::null_mut(), rc));
    }
    let rc = unsafe { auth(handle, 0) };
    let rc = if rc == PAM_SUCCESS { unsafe { acct(handle, 0) } } else { rc };
    let result = if rc == PAM_SUCCESS { Ok(()) } else { Err(message(handle, rc)) };
    unsafe { end(handle, rc) };
    drop(password);
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_wrong_password_is_refused_with_a_reason() {
        let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
        let result = super::authenticate(&user, "definitely-not-the-password");
        assert!(result.is_err(), "{result:?}");
    }
}
