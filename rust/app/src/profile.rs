pub fn default_profile_dir() -> String {
    expand_env(r"%LOCALAPPDATA%\Larian Studios\Baldur's Gate 3\PlayerProfiles\Public\")
}

/// Expand `%NAME%` tokens using process environment variables. Unknown or
/// unterminated tokens are left verbatim (matching
/// `Environment.ExpandEnvironmentVariables`).
pub fn expand_env(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        match after_start.find('%') {
            Some(end) => {
                let name = &after_start[..end];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        // Unknown variable: keep the literal text.
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after_start[end + 1..];
            }
            None => {
                // Unterminated: keep the rest verbatim.
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // The expansion reads process-global environment variables, so tests that
    // touch them must not run concurrently.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn default_dir_uses_localappdata() {
        let _g = lock();
        set_var("LOCALAPPDATA", r"C:\Users\king\AppData\Local");
        assert_eq!(
            default_profile_dir(),
            r"C:\Users\king\AppData\Local\Larian Studios\Baldur's Gate 3\PlayerProfiles\Public\"
        );
    }

    #[test]
    fn expand_replaces_known_var() {
        let _g = lock();
        set_var("LOCALAPPDATA", r"C:\Apps");
        assert_eq!(
            expand_env(r"%LOCALAPPDATA%\Larian Studios"),
            r"C:\Apps\Larian Studios"
        );
    }

    #[test]
    fn expand_leaves_unknown_var_alone() {
        let _g = lock();
        assert_eq!(expand_env(r"%NOPE%\foo"), r"%NOPE%\foo");
    }

    #[test]
    fn expand_passthrough_literal() {
        let _g = lock();
        assert_eq!(expand_env(r"C:\plain\path"), r"C:\plain\path");
    }

    #[test]
    fn expand_handles_multiple_vars() {
        let _g = lock();
        set_var("A", "1");
        set_var("B", "2");
        assert_eq!(expand_env(r"%A%-%B%"), "1-2");
    }

    #[test]
    fn expand_handles_adjacent_vars() {
        let _g = lock();
        set_var("A", "x");
        set_var("B", "y");
        assert_eq!(expand_env("%A%%B%"), "xy");
    }

    fn set_var(name: &str, val: &str) {
        std::env::set_var(name, val);
    }
}