//! 密码的宿主：优先系统凭据库（Windows 凭据管理器 / macOS 钥匙串），
//! 不可用时（Linux 没有统一凭据服务、或凭据库读写失败）回退到 rclone.conf。
//!
//! 存进凭据库的是 **rclone obscure 过的串**，不是明文——rclone.conf 里的
//! `pass =` 就是这个格式，我们原样搬运；需要明文时（导出配置）走 `rclone reveal`。
//! 这样即使是拿到凭据库内容的攻击者，也还要再过一层 obscure（可逆混淆），
//! 而真正的系统凭据库本身是按用户加密的。

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod imp {
    use keyring::Entry;

    /// 凭据库里的服务名；凭据管理器里显示为「rclone-mount-manager: <连接id>」
    const SERVICE: &str = "rclone-mount-manager";

    pub fn set(id: &str, obscured: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE, id).map_err(|e| format!("打开系统凭据库失败：{e}"))?;
        entry
            .set_password(obscured)
            .map_err(|e| format!("写入系统凭据库失败：{e}"))
    }

    pub fn get(id: &str) -> Option<String> {
        let entry = Entry::new(SERVICE, id).ok()?;
        entry.get_password().ok()
    }

    pub fn remove(id: &str) {
        if let Ok(entry) = Entry::new(SERVICE, id) {
            let _ = entry.delete_credential();
        }
    }

    pub fn available() -> bool {
        true
    }
}

/// Linux 上没有跨发行版统一的凭据服务：GNOME 有 gnome-keyring、KDE 有 kwallet、
/// 服务器/容器常常什么都没有。keyring crate 的 secret-service 后端要走 dbus，
/// 在无桌面的环境里直接挂掉。为了「装完就能用」，Linux 保持密码落 conf 的老行为。
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod imp {
    pub fn set(_id: &str, _obscured: &str) -> Result<(), String> {
        Err("此平台暂不支持系统凭据库，密码仍保存在 rclone.conf".into())
    }

    pub fn get(_id: &str) -> Option<String> {
        None
    }

    pub fn remove(_id: &str) {}

    pub fn available() -> bool {
        false
    }
}

pub fn set_password(app: &tauri::AppHandle, id: &str, obscured: &str) -> Result<(), String> {
    let _ = app;
    imp::set(id, obscured)
}

pub fn get_password(app: &tauri::AppHandle, id: &str) -> Option<String> {
    let _ = app;
    imp::get(id)
}

pub fn delete_password(app: &tauri::AppHandle, id: &str) {
    let _ = app;
    imp::remove(id)
}

pub fn available() -> bool {
    imp::available()
}
