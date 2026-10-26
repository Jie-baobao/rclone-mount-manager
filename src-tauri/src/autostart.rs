//! 开机自启动：不依赖第三方插件，直接写各平台自己的启动项。
//! - Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//! - macOS:   ~/Library/LaunchAgents/<bundle-id>.plist
//! - Linux:   ~/.config/autostart/<name>.desktop

use std::path::PathBuf;

const ENTRY_NAME: &str = "RcloneMountManager";
const BUNDLE_ID: &str = "io.github.rclonemount.manager";

fn exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("无法获取程序路径：{e}"))
}

#[cfg(windows)]
pub fn set_enabled(enable: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let (key, _) = hkcu
        .create_subkey(path)
        .map_err(|e| format!("打开注册表启动项失败：{e}"))?;

    if enable {
        let exe = exe_path()?;
        // 带 --silent 启动：由设置里的「静默启动」决定要不要弹主界面
        let value = format!("\"{}\" --silent", exe.display());
        key.set_value(ENTRY_NAME, &value)
            .map_err(|e| format!("写入启动项失败：{e}"))?;
    } else {
        let _ = key.delete_value(ENTRY_NAME);
    }
    Ok(())
}

#[cfg(windows)]
pub fn is_enabled() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
        Ok(key) => key.get_value::<String, _>(ENTRY_NAME).is_ok(),
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{BUNDLE_ID}.plist")))
}

#[cfg(target_os = "macos")]
pub fn set_enabled(enable: bool) -> Result<(), String> {
    let path = plist_path().ok_or_else(|| "无法定位用户目录".to_string())?;
    if enable {
        let exe = exe_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败：{e}"))?;
        }
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{BUNDLE_ID}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>--silent</string>
  </array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
            exe.display()
        );
        std::fs::write(&path, plist).map_err(|e| format!("写入启动项失败：{e}"))?;
    } else {
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn desktop_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".config/autostart")
            .join(format!("{ENTRY_NAME}.desktop")),
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn set_enabled(enable: bool) -> Result<(), String> {
    let path = desktop_path().ok_or_else(|| "无法定位用户目录".to_string())?;
    if enable {
        let exe = exe_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败：{e}"))?;
        }
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Rclone Mount Manager\nExec=\"{}\" --silent\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        std::fs::write(&path, entry).map_err(|e| format!("写入启动项失败：{e}"))?;
    } else {
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn is_enabled() -> bool {
    desktop_path().map(|p| p.exists()).unwrap_or(false)
}
