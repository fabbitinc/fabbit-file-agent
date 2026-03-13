#[cfg(target_os = "windows")]
mod platform {
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::RegKey;

    const CLSID: &str = "{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}";
    const DISPLAY_NAME: &str = "Fabbit";
    const FOLDER_SHORTCUT_CLSID: &str = "{0E5AAE11-A475-4c5b-AB00-C66DE400274E}";

    pub fn target_folder() -> PathBuf {
        dirs::home_dir()
            .expect("Failed to get home directory")
            .join("Fabbit")
    }

    pub fn shell_uri() -> String {
        format!("shell:::{}", CLSID)
    }

    pub fn register(icon_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let target = target_folder();
        std::fs::create_dir_all(&target)?;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        // 1. CLSID 등록
        let clsid_path = format!(r"Software\Classes\CLSID\{}", CLSID);
        let (clsid_key, _) = hkcu.create_subkey(&clsid_path)?;
        clsid_key.set_value("", &DISPLAY_NAME)?;
        clsid_key.set_value("SortOrderIndex", &0x42u32)?;
        clsid_key.set_value("System.IsPinnedToNameSpaceTree", &1u32)?;

        // DefaultIcon
        let (icon_key, _) = clsid_key.create_subkey("DefaultIcon")?;
        icon_key.set_value("", &format!("{},0", icon_path))?;

        // InProcServer32 (빈 문자열 - 셸 폴더 바로가기)
        let (inproc_key, _) = clsid_key.create_subkey("InProcServer32")?;
        inproc_key.set_value("", &"")?;

        // Instance - 셸 폴더 바로가기 CLSID 참조
        let (instance_key, _) = clsid_key.create_subkey("Instance")?;
        instance_key.set_value("CLSID", &FOLDER_SHORTCUT_CLSID)?;

        // Instance\InitPropertyBag - 실제 폴더 경로 지정
        let (bag_key, _) = instance_key.create_subkey("InitPropertyBag")?;
        bag_key.set_value("Attributes", &0x11u32)?;
        bag_key.set_value("TargetFolderPath", &target.to_string_lossy().as_ref())?;

        // ShellFolder
        let (sf_key, _) = clsid_key.create_subkey("ShellFolder")?;
        sf_key.set_value("FolderValueFlags", &0x28u32)?;
        sf_key.set_value("Attributes", &0xF080004Du32)?;

        // 2. "내 PC" 하위에 등록
        let ns_path = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace\{}",
            CLSID
        );
        let (ns_key, _) = hkcu.create_subkey(&ns_path)?;
        ns_key.set_value("", &DISPLAY_NAME)?;

        // 3. 바탕화면 아이콘 숨김
        let hide_path =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel";
        let (hide_key, _) = hkcu.create_subkey(hide_path)?;
        hide_key.set_value(CLSID, &1u32)?;

        // 탐색기에 변경 알림
        notify_shell_change();

        Ok(())
    }

    pub fn unregister() -> Result<(), Box<dyn std::error::Error>> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        let _ = hkcu.delete_subkey_all(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace\{}",
            CLSID
        ));
        let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\CLSID\{}", CLSID));

        // HideDesktopIcons 엔트리 제거
        if let Ok(hide_key) = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel",
            KEY_WRITE,
        ) {
            let _ = hide_key.delete_value(CLSID);
        }

        notify_shell_change();

        Ok(())
    }

    pub fn is_registered() -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        hkcu.open_subkey(format!(r"Software\Classes\CLSID\{}", CLSID))
            .is_ok()
    }

    fn notify_shell_change() {
        unsafe {
            windows_sys::Win32::UI::Shell::SHChangeNotify(
                0x08000000, // SHCNE_ASSOCCHANGED
                0x0000,     // SHCNF_IDLIST
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }
}

#[cfg(target_os = "windows")]
pub use platform::*;

#[cfg(not(target_os = "windows"))]
pub fn target_folder() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("Failed to get home directory")
        .join("Fabbit")
}

#[cfg(not(target_os = "windows"))]
pub fn register(_icon_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(target_folder())?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn unregister() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn is_registered() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn shell_uri() -> String {
    target_folder().to_string_lossy().to_string()
}
