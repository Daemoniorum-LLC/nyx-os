//! User information utilities

use anyhow::{Result, anyhow};
use std::ffi::CStr;

/// User information
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub gecos: String,
    pub home: String,
    pub shell: String,
    pub groups: Vec<u32>,
}

/// Get user information by username
pub fn get_user_info(username: &str) -> Result<UserInfo> {
    let username_c = std::ffi::CString::new(username)?;

    unsafe {
        let pwd = libc::getpwnam(username_c.as_ptr());
        if pwd.is_null() {
            return Err(anyhow!("User not found: {}", username));
        }

        let pwd = &*pwd;

        let home = CStr::from_ptr(pwd.pw_dir).to_string_lossy().to_string();
        let shell = CStr::from_ptr(pwd.pw_shell).to_string_lossy().to_string();
        let gecos = if pwd.pw_gecos.is_null() {
            String::new()
        } else {
            CStr::from_ptr(pwd.pw_gecos).to_string_lossy().to_string()
        };

        let groups = get_user_groups(pwd.pw_name, pwd.pw_gid)?;

        Ok(UserInfo {
            username: username.to_string(),
            uid: pwd.pw_uid,
            gid: pwd.pw_gid,
            gecos,
            home,
            shell,
            groups,
        })
    }
}

/// Get user information by UID
pub fn get_user_by_uid(uid: u32) -> Result<UserInfo> {
    unsafe {
        let pwd = libc::getpwuid(uid);
        if pwd.is_null() {
            return Err(anyhow!("User not found for UID: {}", uid));
        }

        let pwd = &*pwd;
        let username = CStr::from_ptr(pwd.pw_name).to_string_lossy().to_string();

        get_user_info(&username)
    }
}

/// Get supplementary groups for a user
fn get_user_groups(username: *const libc::c_char, primary_gid: u32) -> Result<Vec<u32>> {
    let mut ngroups: libc::c_int = 32;
    let mut groups = vec![0 as libc::gid_t; ngroups as usize];

    unsafe {
        let result = libc::getgrouplist(
            username,
            primary_gid,
            groups.as_mut_ptr(),
            &mut ngroups,
        );

        if result < 0 {
            // Need more space
            groups.resize(ngroups as usize, 0);
            libc::getgrouplist(
                username,
                primary_gid,
                groups.as_mut_ptr(),
                &mut ngroups,
            );
        }
    }

    groups.truncate(ngroups as usize);
    Ok(groups)
}

/// Get group name by GID
pub fn get_group_name(gid: u32) -> Option<String> {
    unsafe {
        let grp = libc::getgrgid(gid);
        if grp.is_null() {
            return None;
        }

        Some(CStr::from_ptr((*grp).gr_name).to_string_lossy().to_string())
    }
}

/// List all users eligible for login
pub fn list_login_users(min_uid: u32, max_uid: u32, hidden: &[String]) -> Vec<UserInfo> {
    let mut users = Vec::new();

    // Read /etc/passwd
    unsafe {
        libc::setpwent();

        loop {
            let pwd = libc::getpwent();
            if pwd.is_null() {
                break;
            }

            let pwd = &*pwd;
            let uid = pwd.pw_uid;

            // Skip users outside UID range
            if uid < min_uid || uid > max_uid {
                continue;
            }

            let username = CStr::from_ptr(pwd.pw_name).to_string_lossy().to_string();

            // Skip hidden users
            if hidden.contains(&username) {
                continue;
            }

            // Skip users with nologin shell
            let shell = CStr::from_ptr(pwd.pw_shell).to_string_lossy().to_string();
            if shell.contains("nologin") || shell.contains("false") {
                continue;
            }

            if let Ok(info) = get_user_info(&username) {
                users.push(info);
            }
        }

        libc::endpwent();
    }

    users.sort_by(|a, b| a.username.cmp(&b.username));
    users
}

/// Check if user account is valid (not expired, not locked)
pub fn is_account_valid(username: &str) -> Result<bool> {
    // Would check /etc/shadow for account expiration
    // and password status. For now, return true if user exists.

    get_user_info(username)?;
    Ok(true)
}

/// Get user avatar path
pub fn get_user_avatar(username: &str) -> Option<String> {
    let paths = [
        format!("/var/lib/AccountsService/icons/{}", username),
        format!("/home/{}/.face", username),
        format!("/home/{}/.face.icon", username),
    ];

    for path in paths {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    None
}

/// User display info for greeter
#[derive(Debug, Clone)]
pub struct UserDisplay {
    pub username: String,
    pub display_name: String,
    pub avatar: Option<String>,
    pub uid: u32,
}

impl From<UserInfo> for UserDisplay {
    fn from(info: UserInfo) -> Self {
        let display_name = if info.gecos.is_empty() {
            info.username.clone()
        } else {
            // GECOS first field is full name
            info.gecos.split(',').next()
                .unwrap_or(&info.username)
                .to_string()
        };

        Self {
            avatar: get_user_avatar(&info.username),
            username: info.username,
            display_name,
            uid: info.uid,
        }
    }
}

/// Set up user environment
pub fn setup_user_environment(user: &UserInfo) -> std::collections::HashMap<String, String> {
    let mut env = std::collections::HashMap::new();

    env.insert("USER".into(), user.username.clone());
    env.insert("LOGNAME".into(), user.username.clone());
    env.insert("HOME".into(), user.home.clone());
    env.insert("SHELL".into(), user.shell.clone());
    env.insert("PATH".into(), "/usr/local/bin:/usr/bin:/bin".into());

    // XDG base directories
    env.insert("XDG_CACHE_HOME".into(), format!("{}/.cache", user.home));
    env.insert("XDG_CONFIG_HOME".into(), format!("{}/.config", user.home));
    env.insert("XDG_DATA_HOME".into(), format!("{}/.local/share", user.home));
    env.insert("XDG_STATE_HOME".into(), format!("{}/.local/state", user.home));

    env
}

/// Switch to user credentials
pub fn switch_user(user: &UserInfo) -> Result<()> {
    use nix::unistd::{setgid, setgroups, setuid, Gid, Uid};

    // Must set groups before dropping privileges
    let gids: Vec<Gid> = user.groups.iter()
        .map(|&g| Gid::from_raw(g))
        .collect();

    setgroups(&gids)?;
    setgid(Gid::from_raw(user.gid))?;
    setuid(Uid::from_raw(user.uid))?;

    // Change to home directory
    std::env::set_current_dir(&user.home)?;

    Ok(())
}
