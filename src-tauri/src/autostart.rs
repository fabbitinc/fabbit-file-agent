#[cfg(target_os = "windows")]
mod platform {
    use winreg::enums::*;
    use winreg::RegKey;

    const REG_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "FabbitFileAgent";

    pub fn is_enabled() -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey(REG_PATH) {
            key.get_value::<String, _>(APP_NAME).is_ok()
        } else {
            false
        }
    }

    pub fn enable() -> Result<(), Box<dyn std::error::Error>> {
        let exe_path = std::env::current_exe()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(REG_PATH)?;
        key.set_value(APP_NAME, &exe_path.to_string_lossy().as_ref())?;
        Ok(())
    }

    pub fn disable() -> Result<(), Box<dyn std::error::Error>> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey_with_flags(REG_PATH, KEY_WRITE) {
            let _ = key.delete_value(APP_NAME);
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub use platform::*;

#[cfg(not(target_os = "windows"))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn enable() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn disable() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

pub fn toggle() -> Result<bool, Box<dyn std::error::Error>> {
    if is_enabled() {
        disable()?;
        Ok(false)
    } else {
        enable()?;
        Ok(true)
    }
}
