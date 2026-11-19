//! Which space slice a process is in, read from its `/proc/<pid>/cgroup`.
//! The names are the compositor's (`ede_protocol::process`), under the
//! user manager of the process's own uid.

/// A compositor's slice of all its spaces.
pub fn spaces_slice(compositor: u32) -> String {
    format!("app-exponential_p{compositor}.slice")
}

/// One space's slice.
pub fn space_slice(compositor: u32, space: u32) -> String {
    format!("app-exponential_p{compositor}-s{space}.slice")
}

/// A space's slice, as path components below `/sys/fs/cgroup`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceSlice {
    pub compositor: u32,
    pub space: u32,
    pub components: Vec<String>,
}

impl SpaceSlice {
    pub fn name(&self) -> &str {
        self.components.last().map(String::as_str).unwrap_or_default()
    }
}

/// A number as the compositor writes one: digits, no leading zero.
fn number(text: &str) -> Option<u32> {
    let n: u32 = text.parse().ok()?;
    (n.to_string() == text).then_some(n)
}

/// The space slice the process whose `/proc/<pid>/cgroup` is `text` is in
/// (itself or one of its cgroups below it), under user `uid`'s manager.
pub fn space_slice_of(text: &str, uid: u32) -> Result<SpaceSlice, String> {
    let path = text.lines().find_map(|line| line.strip_prefix("0::")).ok_or("no unified cgroup")?;
    let mut parts = path.split('/');
    let not_space = || format!("{path} is not in an agent space");
    if parts.next() != Some("") {
        return Err(not_space());
    }
    let user = ["user.slice".to_string(), format!("user-{uid}.slice"), format!("user@{uid}.service"), "app.slice".to_string()];
    let mut components = Vec::new();
    for want in user {
        if parts.next() != Some(want.as_str()) {
            return Err(not_space());
        }
        components.push(want);
    }
    let all = parts.next().ok_or_else(not_space)?;
    let compositor = all.strip_prefix("app-exponential_p").and_then(|rest| rest.strip_suffix(".slice")).and_then(number).ok_or_else(not_space)?;
    let one = parts.next().ok_or_else(not_space)?;
    let space = one.strip_prefix(&format!("app-exponential_p{compositor}-s")).and_then(|rest| rest.strip_suffix(".slice")).and_then(number).ok_or_else(not_space)?;
    // What lies below (the launch's scope, the space's user manager)
    // only has to be a path.
    if parts.any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(not_space());
    }
    components.push(spaces_slice(compositor));
    components.push(space_slice(compositor, space));
    Ok(SpaceSlice { compositor, space, components })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_the_compositors() {
        assert_eq!(spaces_slice(41), ede_protocol::process::spaces_slice(41));
        assert_eq!(space_slice(41, 3), ede_protocol::process::space_slice(41, 3));
    }

    #[test]
    fn a_process_of_a_space_names_its_space_slice() {
        let launch = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p41-s3.slice/exponential-p41-s3-7.scope\n";
        let slice = space_slice_of(launch, 1000).unwrap();
        assert_eq!((slice.compositor, slice.space, slice.name()), (41, 3, "app-exponential_p41-s3.slice"));
        assert_eq!(slice.components.join("/"), "user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p41-s3.slice");
        // The space's own user manager and what it starts.
        let nested = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p41-s3.slice/exponential-p41-s3-session.scope/app.slice/snap.firefox.firefox-1.scope\n";
        assert_eq!(space_slice_of(nested, 1000).unwrap().space, 3);
    }

    #[test]
    fn nothing_else_is_a_space() {
        for (text, uid) in [
            // Another user's space.
            ("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p41-s3.slice/x.scope", 1001),
            // The person's session, their apps, the slice of all spaces.
            ("0::/user.slice/user-1000.slice/session-3.scope", 1000),
            ("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-firefox-1.scope", 1000),
            ("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/exponential-p41-probe0.scope", 1000),
            // Another compositor's space under this one's slice, made-up numbers.
            ("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p42-s3.slice/x.scope", 1000),
            ("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p041.slice/app-exponential_p041-s3.slice/x.scope", 1000),
            ("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p41-s.slice/x.scope", 1000),
            ("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p41-s3.slice/../../x", 1000),
            ("0::/system.slice/app-exponential_p41.slice/app-exponential_p41-s3.slice", 1000),
            ("", 1000),
            ("1:name=systemd:/user.slice", 1000),
        ] {
            assert!(space_slice_of(text, uid).is_err(), "{text} as {uid}");
        }
    }
}
