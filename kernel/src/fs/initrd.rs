//! Initial Ramdisk (initrd) Filesystem
//!
//! Supports both CPIO (newc format) and USTAR (TAR) archives.
//! This is a read-only filesystem for booting.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use super::{FsError, FileStat, FileType, DirEntry};

/// Initrd filesystem
pub struct InitrdFs {
    /// File entries indexed by path
    entries: BTreeMap<String, InitrdEntry>,
    /// Format detected
    format: InitrdFormat,
}

/// Initrd archive format
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitrdFormat {
    /// CPIO newc format
    CpioNewc,
    /// USTAR TAR format
    Ustar,
    /// Unknown format
    Unknown,
}

/// Entry in the initrd
#[derive(Clone)]
struct InitrdEntry {
    /// File type
    file_type: FileType,
    /// File mode
    mode: u32,
    /// File size
    size: u64,
    /// Data offset in archive
    data_offset: usize,
    /// Data slice (reference to archive data)
    data: Vec<u8>,
    /// User ID
    uid: u32,
    /// Group ID
    gid: u32,
    /// Modification time
    mtime: u64,
}

/// Initrd parse error
#[derive(Debug, Clone)]
pub enum InitrdError {
    /// Invalid magic number
    InvalidMagic,
    /// Truncated data
    Truncated,
    /// Invalid header
    InvalidHeader,
    /// Unsupported format
    UnsupportedFormat,
}

impl InitrdFs {
    /// Create a new initrd filesystem from raw data
    pub fn new(data: &[u8]) -> Result<Self, InitrdError> {
        let format = Self::detect_format(data)?;

        let entries = match format {
            InitrdFormat::CpioNewc => Self::parse_cpio(data)?,
            InitrdFormat::Ustar => Self::parse_tar(data)?,
            InitrdFormat::Unknown => return Err(InitrdError::UnsupportedFormat),
        };

        log::debug!("Parsed initrd: {} entries, format: {:?}", entries.len(), format);

        Ok(Self { entries, format })
    }

    /// Detect archive format from magic bytes
    fn detect_format(data: &[u8]) -> Result<InitrdFormat, InitrdError> {
        if data.len() < 6 {
            return Err(InitrdError::Truncated);
        }

        // Check for CPIO newc magic: "070701" or "070702"
        if &data[0..6] == b"070701" || &data[0..6] == b"070702" {
            return Ok(InitrdFormat::CpioNewc);
        }

        // Check for USTAR magic at offset 257
        if data.len() >= 263 && &data[257..262] == b"ustar" {
            return Ok(InitrdFormat::Ustar);
        }

        // Try to detect plain TAR (no magic, but first byte is usually 0 or filename)
        if data.len() >= 512 && data[0] != 0 {
            // Check if it looks like a TAR header
            let checksum_field = &data[148..156];
            if checksum_field.iter().all(|&b| b == 0 || b == b' ' || b.is_ascii_digit()) {
                return Ok(InitrdFormat::Ustar);
            }
        }

        Err(InitrdError::InvalidMagic)
    }

    /// Parse CPIO newc format
    fn parse_cpio(data: &[u8]) -> Result<BTreeMap<String, InitrdEntry>, InitrdError> {
        let mut entries = BTreeMap::new();
        let mut offset = 0;

        while offset + 110 <= data.len() {
            // Check magic
            if &data[offset..offset + 6] != b"070701" && &data[offset..offset + 6] != b"070702" {
                break;
            }

            // Parse header fields (all hex strings)
            let ino = parse_hex_str(&data[offset + 6..offset + 14])?;
            let mode = parse_hex_str(&data[offset + 14..offset + 22])? as u32;
            let uid = parse_hex_str(&data[offset + 22..offset + 30])? as u32;
            let gid = parse_hex_str(&data[offset + 30..offset + 38])? as u32;
            let _nlink = parse_hex_str(&data[offset + 38..offset + 46])?;
            let mtime = parse_hex_str(&data[offset + 46..offset + 54])?;
            let filesize = parse_hex_str(&data[offset + 54..offset + 62])? as usize;
            let _devmajor = parse_hex_str(&data[offset + 62..offset + 70])?;
            let _devminor = parse_hex_str(&data[offset + 70..offset + 78])?;
            let _rdevmajor = parse_hex_str(&data[offset + 78..offset + 86])?;
            let _rdevminor = parse_hex_str(&data[offset + 86..offset + 94])?;
            let namesize = parse_hex_str(&data[offset + 94..offset + 102])? as usize;
            let _check = parse_hex_str(&data[offset + 102..offset + 110])?;

            // Read filename
            let name_start = offset + 110;
            let name_end = name_start + namesize - 1; // -1 for null terminator
            if name_end > data.len() {
                return Err(InitrdError::Truncated);
            }

            let name = String::from_utf8_lossy(&data[name_start..name_end]).to_string();

            // CPIO trailer
            if name == "TRAILER!!!" {
                break;
            }

            // Align to 4 bytes after name
            let data_start = align_up(name_start + namesize, 4);
            let data_end = data_start + filesize;

            if data_end > data.len() {
                return Err(InitrdError::Truncated);
            }

            // Determine file type from mode
            let file_type = match mode & 0o170000 {
                0o040000 => FileType::Directory,
                0o100000 => FileType::Regular,
                0o120000 => FileType::Symlink,
                0o020000 => FileType::CharDevice,
                0o060000 => FileType::BlockDevice,
                0o010000 => FileType::Fifo,
                0o140000 => FileType::Socket,
                _ => FileType::Regular,
            };

            // Normalize path
            let path = if name.starts_with("./") {
                format!("/{}", &name[2..])
            } else if name.starts_with('/') {
                name.clone()
            } else {
                format!("/{}", name)
            };

            // Store entry
            entries.insert(path, InitrdEntry {
                file_type,
                mode: mode & 0o7777,
                size: filesize as u64,
                data_offset: data_start,
                data: data[data_start..data_end].to_vec(),
                uid,
                gid,
                mtime,
            });

            // Align to 4 bytes after data
            offset = align_up(data_end, 4);
        }

        // Add root directory if not present
        if !entries.contains_key("/") {
            entries.insert(String::from("/"), InitrdEntry {
                file_type: FileType::Directory,
                mode: 0o755,
                size: 0,
                data_offset: 0,
                data: Vec::new(),
                uid: 0,
                gid: 0,
                mtime: 0,
            });
        }

        Ok(entries)
    }

    /// Parse USTAR/TAR format
    fn parse_tar(data: &[u8]) -> Result<BTreeMap<String, InitrdEntry>, InitrdError> {
        let mut entries = BTreeMap::new();
        let mut offset = 0;

        while offset + 512 <= data.len() {
            let header = &data[offset..offset + 512];

            // Check for empty block (end of archive)
            if header.iter().all(|&b| b == 0) {
                break;
            }

            // Parse TAR header
            let name = parse_tar_string(&header[0..100]);
            if name.is_empty() {
                break;
            }

            let mode = parse_octal_str(&header[100..108]).unwrap_or(0o644) as u32;
            let uid = parse_octal_str(&header[108..116]).unwrap_or(0) as u32;
            let gid = parse_octal_str(&header[116..124]).unwrap_or(0) as u32;
            let size = parse_octal_str(&header[124..136]).unwrap_or(0) as usize;
            let mtime = parse_octal_str(&header[136..148]).unwrap_or(0);
            let typeflag = header[156];

            // Determine file type
            let file_type = match typeflag {
                b'0' | 0 => FileType::Regular,
                b'1' => FileType::Regular, // Hard link (treat as regular)
                b'2' => FileType::Symlink,
                b'3' => FileType::CharDevice,
                b'4' => FileType::BlockDevice,
                b'5' => FileType::Directory,
                b'6' => FileType::Fifo,
                _ => FileType::Regular,
            };

            // Handle long names (GNU tar extension)
            let prefix = parse_tar_string(&header[345..500]);
            let full_name = if !prefix.is_empty() {
                format!("{}/{}", prefix, name)
            } else {
                name
            };

            // Normalize path
            let path = if full_name.starts_with("./") {
                format!("/{}", &full_name[2..])
            } else if full_name.starts_with('/') {
                full_name.clone()
            } else {
                format!("/{}", full_name)
            };

            // Remove trailing slash for directories
            let path = path.trim_end_matches('/').to_string();
            let path = if path.is_empty() { String::from("/") } else { path };

            // Data follows header
            let data_start = offset + 512;
            let data_end = data_start + size;

            if data_end > data.len() {
                return Err(InitrdError::Truncated);
            }

            entries.insert(path, InitrdEntry {
                file_type,
                mode,
                size: size as u64,
                data_offset: data_start,
                data: data[data_start..data_end].to_vec(),
                uid,
                gid,
                mtime,
            });

            // Move to next header (512-byte aligned)
            offset = align_up(data_end, 512);
        }

        // Add root directory if not present
        if !entries.contains_key("/") {
            entries.insert(String::from("/"), InitrdEntry {
                file_type: FileType::Directory,
                mode: 0o755,
                size: 0,
                data_offset: 0,
                data: Vec::new(),
                uid: 0,
                gid: 0,
                mtime: 0,
            });
        }

        Ok(entries)
    }

    /// Read a file's contents
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let entry = self.entries.get(path).ok_or(FsError::NotFound)?;

        if entry.file_type == FileType::Directory {
            return Err(FsError::IsDirectory);
        }

        Ok(entry.data.clone())
    }

    /// Read file at specific offset into buffer
    ///
    /// Used for memory-mapped file access.
    pub fn read_at(&self, path: &str, offset: u64, buffer: &mut [u8]) -> Result<usize, FsError> {
        let entry = self.entries.get(path).ok_or(FsError::NotFound)?;

        if entry.file_type == FileType::Directory {
            return Err(FsError::IsDirectory);
        }

        // Calculate how much we can read
        let offset = offset as usize;
        if offset >= entry.data.len() {
            // Reading past EOF
            return Ok(0);
        }

        let available = entry.data.len() - offset;
        let to_read = buffer.len().min(available);

        buffer[..to_read].copy_from_slice(&entry.data[offset..offset + to_read]);

        Ok(to_read)
    }

    /// Get file metadata
    pub fn stat(&self, path: &str) -> Result<FileStat, FsError> {
        let entry = self.entries.get(path).ok_or(FsError::NotFound)?;

        Ok(FileStat {
            file_type: entry.file_type,
            size: entry.size,
            nlink: 1,
            uid: entry.uid,
            gid: entry.gid,
            mode: entry.mode,
            atime: entry.mtime,
            mtime: entry.mtime,
            ctime: entry.mtime,
            dev: 0,
            ino: 0,
        })
    }

    /// List directory contents
    pub fn readdir(&self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let path = if path == "/" { "" } else { path };

        // Check if directory exists
        if !path.is_empty() {
            let entry = self.entries.get(path).ok_or(FsError::NotFound)?;
            if entry.file_type != FileType::Directory {
                return Err(FsError::NotDirectory);
            }
        }

        let mut results = Vec::new();
        let prefix = if path.is_empty() {
            String::from("/")
        } else {
            format!("{}/", path)
        };

        for (entry_path, entry) in &self.entries {
            // Skip the directory itself
            if entry_path == path || entry_path == "/" && path.is_empty() {
                continue;
            }

            // Check if this is a direct child
            if entry_path.starts_with(&prefix) || (path.is_empty() && entry_path.starts_with('/')) {
                let relative = if path.is_empty() {
                    &entry_path[1..] // Remove leading /
                } else {
                    &entry_path[prefix.len()..]
                };

                // Only include direct children (no more slashes)
                if !relative.contains('/') && !relative.is_empty() {
                    results.push(DirEntry {
                        name: relative.to_string(),
                        file_type: entry.file_type,
                        ino: 0,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Get archive format
    pub fn format(&self) -> InitrdFormat {
        self.format
    }

    /// Get total number of entries
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse hex string to u64
fn parse_hex_str(data: &[u8]) -> Result<u64, InitrdError> {
    let s = core::str::from_utf8(data).map_err(|_| InitrdError::InvalidHeader)?;
    u64::from_str_radix(s, 16).map_err(|_| InitrdError::InvalidHeader)
}

/// Parse octal string to u64
fn parse_octal_str(data: &[u8]) -> Result<u64, InitrdError> {
    let s = core::str::from_utf8(data).map_err(|_| InitrdError::InvalidHeader)?;
    let s = s.trim_matches(|c: char| c == ' ' || c == '\0');
    if s.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(s, 8).map_err(|_| InitrdError::InvalidHeader)
}

/// Parse null-terminated string from TAR header
fn parse_tar_string(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

/// Align value up to boundary
fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
