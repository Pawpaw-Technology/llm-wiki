use std::path::PathBuf;

/// Resolve install prefix in priority order:
/// 1. `$LW_INSTALL_PREFIX` (explicit override)
/// 2. Two directories above the running binary (so `$PREFIX/bin/lw` → `$PREFIX`)
///    when `$PREFIX/installer/install.sh` exists (confirms it's a real install)
/// 3. `$HOME/.llm-wiki` (default)
pub fn install_prefix() -> PathBuf {
    if let Ok(p) = std::env::var("LW_INSTALL_PREFIX") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(p) = exe.ancestors().nth(2)
        && p.join("installer").join("install.sh").exists()
    {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .map(|h| h.join(".llm-wiki"))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn explicit_env_wins() {
        // SAFETY: tests serialized via serial_test
        unsafe { std::env::set_var("LW_INSTALL_PREFIX", "/custom/path") };
        assert_eq!(install_prefix(), PathBuf::from("/custom/path"));
        unsafe { std::env::remove_var("LW_INSTALL_PREFIX") };
    }

    #[test]
    #[serial_test::serial]
    fn falls_back_to_home_when_no_install_detected() {
        // SAFETY: tests serialized via serial_test
        unsafe { std::env::remove_var("LW_INSTALL_PREFIX") };
        // Test binary doesn't live under a real install prefix, so this exercises
        // the current_exe check falling through to the home fallback.
        let got = install_prefix();
        // We can't assert the exact path because it depends on $HOME, but we can
        // assert it's not the current_exe ancestor (which would lack installer/install.sh).
        if let Some(home) = dirs::home_dir() {
            assert_eq!(got, home.join(".llm-wiki"));
        }
    }
}
