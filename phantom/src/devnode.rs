//! Device node creation and management

use crate::device::Device;
use anyhow::{Result, anyhow};
use std::path::Path;
use tracing::{info, debug};

/// Create a device node
pub async fn create_devnode(device: &Device, name: Option<&str>) -> Result<()> {
    let devname = name.unwrap_or_else(|| {
        device.devnode.as_deref()
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or(&device.sysname)
    });

    let devpath = format!("/dev/{}", devname);

    // Need major/minor to create device
    let major = device.major
        .ok_or_else(|| anyhow!("No major number for device"))?;
    let minor = device.minor
        .ok_or_else(|| anyhow!("No minor number for device"))?;

    // Determine device type
    let dev_type = if device.subsystem.as_deref() == Some("block") {
        libc::S_IFBLK
    } else {
        libc::S_IFCHR
    };

    // Create device node
    let devpath_c = std::ffi::CString::new(devpath.as_str())?;
    let dev = makedev(major, minor);

    let result = unsafe {
        // Remove existing node if present
        libc::unlink(devpath_c.as_ptr());

        // Create new node
        libc::mknod(devpath_c.as_ptr(), dev_type | 0o660, dev)
    };

    if result < 0 {
        let err = std::io::Error::last_os_error();
        // EEXIST is okay
        if err.raw_os_error() != Some(libc::EEXIST) {
            return Err(anyhow!("mknod failed: {}", err));
        }
    }

    info!("Created device node: {}", devpath);
    Ok(())
}

/// Create a symlink to a device
pub async fn create_symlink(device: &Device, link_name: &str) -> Result<()> {
    let link_path = if link_name.starts_with('/') {
        link_name.to_string()
    } else {
        format!("/dev/{}", link_name)
    };

    let target = device.devnode.as_ref()
        .ok_or_else(|| anyhow!("Device has no devnode"))?;

    // Ensure parent directory exists
    if let Some(parent) = Path::new(&link_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove existing symlink
    let _ = std::fs::remove_file(&link_path);

    // Create symlink
    std::os::unix::fs::symlink(target, &link_path)?;

    debug!("Created symlink: {} -> {}", link_path, target);
    Ok(())
}

/// Remove a symlink
pub async fn remove_symlink(link_path: &str) -> Result<()> {
    let path = if link_path.starts_with('/') {
        link_path.to_string()
    } else {
        format!("/dev/{}", link_path)
    };

    if Path::new(&path).is_symlink() {
        std::fs::remove_file(&path)?;
        debug!("Removed symlink: {}", path);
    }

    Ok(())
}

/// Set permissions on a device node
pub async fn set_permissions(devpath: &str, mode: u32) -> Result<()> {
    let path = std::ffi::CString::new(devpath)?;

    let result = unsafe {
        libc::chmod(path.as_ptr(), mode)
    };

    if result < 0 {
        return Err(anyhow!("chmod failed: {}", std::io::Error::last_os_error()));
    }

    debug!("Set permissions on {}: {:o}", devpath, mode);
    Ok(())
}

/// Set owner on a device node
pub async fn set_owner(devpath: &str, owner: &str) -> Result<()> {
    let uid = lookup_user(owner)?;

    let path = std::ffi::CString::new(devpath)?;

    let result = unsafe {
        libc::chown(path.as_ptr(), uid, u32::MAX)
    };

    if result < 0 {
        return Err(anyhow!("chown failed: {}", std::io::Error::last_os_error()));
    }

    debug!("Set owner on {}: {}", devpath, owner);
    Ok(())
}

/// Set group on a device node
pub async fn set_group(devpath: &str, group: &str) -> Result<()> {
    let gid = lookup_group(group)?;

    let path = std::ffi::CString::new(devpath)?;

    let result = unsafe {
        libc::chown(path.as_ptr(), u32::MAX, gid)
    };

    if result < 0 {
        return Err(anyhow!("chown failed: {}", std::io::Error::last_os_error()));
    }

    debug!("Set group on {}: {}", devpath, group);
    Ok(())
}

/// Lookup user by name or UID
fn lookup_user(name: &str) -> Result<u32> {
    // Try as numeric UID first
    if let Ok(uid) = name.parse::<u32>() {
        return Ok(uid);
    }

    // Lookup by name
    let name_c = std::ffi::CString::new(name)?;
    let pwd = unsafe { libc::getpwnam(name_c.as_ptr()) };

    if pwd.is_null() {
        return Err(anyhow!("User not found: {}", name));
    }

    Ok(unsafe { (*pwd).pw_uid })
}

/// Lookup group by name or GID
fn lookup_group(name: &str) -> Result<u32> {
    // Try as numeric GID first
    if let Ok(gid) = name.parse::<u32>() {
        return Ok(gid);
    }

    // Lookup by name
    let name_c = std::ffi::CString::new(name)?;
    let grp = unsafe { libc::getgrnam(name_c.as_ptr()) };

    if grp.is_null() {
        return Err(anyhow!("Group not found: {}", name));
    }

    Ok(unsafe { (*grp).gr_gid })
}

/// Create device number from major/minor
fn makedev(major: u32, minor: u32) -> libc::dev_t {
    ((major as libc::dev_t) << 8) | (minor as libc::dev_t & 0xff) | ((minor as libc::dev_t & !0xff) << 12)
}

/// Initialize static device nodes
pub async fn create_static_nodes() -> Result<()> {
    // Create essential device nodes
    let static_nodes = [
        ("null", 1, 3, libc::S_IFCHR, 0o666),
        ("zero", 1, 5, libc::S_IFCHR, 0o666),
        ("full", 1, 7, libc::S_IFCHR, 0o666),
        ("random", 1, 8, libc::S_IFCHR, 0o666),
        ("urandom", 1, 9, libc::S_IFCHR, 0o666),
        ("tty", 5, 0, libc::S_IFCHR, 0o666),
        ("console", 5, 1, libc::S_IFCHR, 0o620),
        ("ptmx", 5, 2, libc::S_IFCHR, 0o666),
    ];

    for (name, major, minor, dev_type, mode) in static_nodes {
        let path = format!("/dev/{}", name);
        let path_c = std::ffi::CString::new(path.as_str())?;
        let dev = makedev(major, minor);

        let _ = unsafe { libc::unlink(path_c.as_ptr()) };

        let result = unsafe {
            libc::mknod(path_c.as_ptr(), dev_type | mode, dev)
        };

        if result < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EEXIST) {
                tracing::warn!("Failed to create {}: {}", path, err);
            }
        }
    }

    // Create common directories
    for dir in ["/dev/pts", "/dev/shm", "/dev/mqueue", "/dev/input", "/dev/dri"] {
        let _ = std::fs::create_dir_all(dir);
    }

    // Create symlinks
    let _ = std::os::unix::fs::symlink("/proc/self/fd", "/dev/fd");
    let _ = std::os::unix::fs::symlink("/proc/self/fd/0", "/dev/stdin");
    let _ = std::os::unix::fs::symlink("/proc/self/fd/1", "/dev/stdout");
    let _ = std::os::unix::fs::symlink("/proc/self/fd/2", "/dev/stderr");

    info!("Created static device nodes");
    Ok(())
}
