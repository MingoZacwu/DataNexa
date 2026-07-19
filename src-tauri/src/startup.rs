//! Platform login-start registration for the current user.

use serde::Serialize;

#[cfg(target_os = "windows")]
const WINDOWS_VALUE: &str = "DataNexa";

#[cfg(target_os = "macos")]
const MAC_LOGIN_ITEM_ERROR: &str = "macOS 登录项注册失败，请确认应用已签名并从 .app 中运行。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
pub enum AutoStartStatus {
    Enabled,
    Disabled,
    #[cfg(target_os = "macos")]
    RequiresApproval,
    Unknown,
}

pub fn enable() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
        run_key.set_value(WINDOWS_VALUE, &expected_windows_value()?)?;
    }
    #[cfg(target_os = "macos")]
    call_mac_service(datanexa_register_login_item)?;
    Ok(())
}

pub fn disable() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
        use winreg::RegKey;

        let run_key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_READ | KEY_WRITE,
        ) {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if windows_value_matches(&run_key, &expected_windows_value()?)? {
            run_key.delete_value(WINDOWS_VALUE)?;
        }
    }
    #[cfg(target_os = "macos")]
    call_mac_service(datanexa_unregister_login_item)?;
    Ok(())
}

pub fn status() -> AutoStartStatus {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let expected = match expected_windows_value() {
            Ok(value) => value,
            Err(_) => return AutoStartStatus::Unknown,
        };
        let run_key = match RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return AutoStartStatus::Disabled;
            }
            Err(_) => return AutoStartStatus::Unknown,
        };
        return match windows_value_matches(&run_key, &expected) {
            Ok(true) => AutoStartStatus::Enabled,
            Ok(false) => AutoStartStatus::Disabled,
            Err(_) => AutoStartStatus::Unknown,
        };
    }
    #[cfg(target_os = "macos")]
    {
        return mac_status(unsafe { datanexa_login_item_status() });
    }
    #[allow(unreachable_code)]
    AutoStartStatus::Unknown
}

#[cfg(target_os = "macos")]
pub fn launched_at_login() -> bool {
    unsafe { datanexa_launched_at_login() != 0 }
}

#[cfg(not(target_os = "macos"))]
pub fn launched_at_login() -> bool {
    false
}

pub fn set_activation_policy(regular: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = unsafe { datanexa_set_activation_policy(if regular { 1 } else { 0 }) };
        if status != 0 {
            return Err(anyhow::anyhow!("unable to set macOS activation policy"));
        }
    }
    let _ = regular;
    Ok(())
}

#[cfg(target_os = "windows")]
fn expected_windows_value() -> anyhow::Result<String> {
    Ok(format!(
        "\"{}\" --autostart",
        std::env::current_exe()?.display()
    ))
}

#[cfg(target_os = "windows")]
fn windows_value_matches(run_key: &winreg::RegKey, expected: &str) -> anyhow::Result<bool> {
    match run_key.get_value::<String, _>(WINDOWS_VALUE) {
        Ok(value) => Ok(value.eq_ignore_ascii_case(expected)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn datanexa_register_login_item(error_buffer: *mut std::ffi::c_char, length: usize) -> i32;
    fn datanexa_unregister_login_item(error_buffer: *mut std::ffi::c_char, length: usize) -> i32;
    fn datanexa_login_item_status() -> i32;
    fn datanexa_launched_at_login() -> i32;
    fn datanexa_set_activation_policy(regular: i32) -> i32;
}

#[cfg(target_os = "macos")]
fn call_mac_service(
    operation: unsafe extern "C" fn(*mut std::ffi::c_char, usize) -> i32,
) -> anyhow::Result<()> {
    let mut error_buffer = [0 as std::ffi::c_char; 2048];
    let status = unsafe { operation(error_buffer.as_mut_ptr(), error_buffer.len()) };
    if status == 0 {
        return Ok(());
    }

    let details = unsafe { std::ffi::CStr::from_ptr(error_buffer.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string();
    if details.is_empty() {
        Err(anyhow::anyhow!("{MAC_LOGIN_ITEM_ERROR} (code {status})"))
    } else {
        Err(anyhow::anyhow!("{MAC_LOGIN_ITEM_ERROR} {details}"))
    }
}

#[cfg(target_os = "macos")]
fn mac_status(status: i32) -> AutoStartStatus {
    match status {
        0 => AutoStartStatus::Disabled,
        1 => AutoStartStatus::Enabled,
        2 => AutoStartStatus::RequiresApproval,
        3 => AutoStartStatus::Disabled,
        _ => AutoStartStatus::Unknown,
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{mac_status, AutoStartStatus};

    #[test]
    fn maps_sm_app_service_statuses() {
        assert_eq!(mac_status(0), AutoStartStatus::Disabled);
        assert_eq!(mac_status(1), AutoStartStatus::Enabled);
        assert_eq!(mac_status(2), AutoStartStatus::RequiresApproval);
        assert_eq!(mac_status(3), AutoStartStatus::Disabled);
        assert_eq!(mac_status(99), AutoStartStatus::Unknown);
    }
}
