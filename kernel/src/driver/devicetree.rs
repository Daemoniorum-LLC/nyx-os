//! Device Tree (DTB/FDT) support
//!
//! Provides access to Flattened Device Tree for hardware discovery,
//! primarily used on ARM and Apple Silicon platforms.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::{Lazy, RwLock};

/// Cached device tree
static DEVICE_TREE: Lazy<RwLock<Option<DeviceTree>>> = Lazy::new(|| RwLock::new(None));

/// Device Tree header magic
const FDT_MAGIC: u32 = 0xD00DFEED;

/// Device Tree structure
#[derive(Clone)]
pub struct DeviceTree {
    /// Root node
    root: DeviceTreeNode,
    /// Memory reservations
    memory_reservations: Vec<MemoryReservation>,
    /// Strings block
    strings: Vec<u8>,
}

impl DeviceTree {
    /// Find a node by compatible string
    pub fn find_compatible(&self, compatible: &str) -> Option<DeviceTreeNode> {
        self.root.find_compatible(compatible)
    }

    /// Find all nodes matching a compatible string
    pub fn find_all_compatible(&self, compatible: &str) -> Vec<DeviceTreeNode> {
        let mut results = Vec::new();
        self.root.find_all_compatible(compatible, &mut results);
        results
    }

    /// Get root node
    pub fn root(&self) -> &DeviceTreeNode {
        &self.root
    }

    /// Find node by path (e.g., "/soc/gpu")
    pub fn find_by_path(&self, path: &str) -> Option<DeviceTreeNode> {
        if path == "/" {
            return Some(self.root.clone());
        }

        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        self.root.find_by_path_parts(&parts)
    }
}

/// Device tree node
#[derive(Clone, Debug)]
pub struct DeviceTreeNode {
    /// Node name
    pub name: String,
    /// Properties
    properties: Vec<DeviceTreeProperty>,
    /// Child nodes
    children: Vec<DeviceTreeNode>,
}

impl DeviceTreeNode {
    /// Create a new node
    pub fn new(name: String) -> Self {
        Self {
            name,
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Get a property by name
    pub fn property(&self, name: &str) -> Option<&DeviceTreeProperty> {
        self.properties.iter().find(|p| p.name == name)
    }

    /// Get property as u32
    pub fn property_u32(&self, name: &str) -> Option<u32> {
        self.property(name).and_then(|p| {
            if p.value.len() >= 4 {
                Some(u32::from_be_bytes([p.value[0], p.value[1], p.value[2], p.value[3]]))
            } else {
                None
            }
        })
    }

    /// Get property as u64
    pub fn property_u64(&self, name: &str) -> Option<u64> {
        self.property(name).and_then(|p| {
            if p.value.len() >= 8 {
                Some(u64::from_be_bytes([
                    p.value[0], p.value[1], p.value[2], p.value[3],
                    p.value[4], p.value[5], p.value[6], p.value[7],
                ]))
            } else if p.value.len() >= 4 {
                Some(u32::from_be_bytes([p.value[0], p.value[1], p.value[2], p.value[3]]) as u64)
            } else {
                None
            }
        })
    }

    /// Get property as string
    pub fn property_string(&self, name: &str) -> Option<&str> {
        self.property(name).and_then(|p| {
            // Find null terminator
            let len = p.value.iter().position(|&b| b == 0).unwrap_or(p.value.len());
            core::str::from_utf8(&p.value[..len]).ok()
        })
    }

    /// Get property as string list
    pub fn property_strings(&self, name: &str) -> Vec<&str> {
        self.property(name)
            .map(|p| {
                p.value
                    .split(|&b| b == 0)
                    .filter_map(|s| core::str::from_utf8(s).ok())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get "compatible" property strings
    pub fn compatible(&self) -> Vec<&str> {
        self.property_strings("compatible")
    }

    /// Check if node is compatible with given string
    pub fn is_compatible(&self, compat: &str) -> bool {
        self.compatible().iter().any(|&c| c == compat)
    }

    /// Get "reg" property as (address, size) pairs
    pub fn reg(&self) -> Vec<(u64, u64)> {
        let addr_cells = self.property_u32("#address-cells").unwrap_or(2) as usize;
        let size_cells = self.property_u32("#size-cells").unwrap_or(1) as usize;

        self.property("reg")
            .map(|p| {
                let cell_size = (addr_cells + size_cells) * 4;
                let mut result = Vec::new();

                for chunk in p.value.chunks_exact(cell_size) {
                    let addr = read_cells(&chunk[..addr_cells * 4], addr_cells);
                    let size = read_cells(&chunk[addr_cells * 4..], size_cells);
                    result.push((addr, size));
                }

                result
            })
            .unwrap_or_default()
    }

    /// Get "interrupts" property
    pub fn interrupts(&self) -> Vec<u32> {
        self.property("interrupts")
            .map(|p| {
                p.value
                    .chunks_exact(4)
                    .map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get child nodes
    pub fn children(&self) -> &[DeviceTreeNode] {
        &self.children
    }

    /// Find child by name
    pub fn find_child(&self, name: &str) -> Option<&DeviceTreeNode> {
        self.children.iter().find(|c| c.name == name || c.name.starts_with(&format!("{}@", name)))
    }

    /// Find node with compatible string (recursive)
    fn find_compatible(&self, compatible: &str) -> Option<DeviceTreeNode> {
        if self.is_compatible(compatible) {
            return Some(self.clone());
        }

        for child in &self.children {
            if let Some(node) = child.find_compatible(compatible) {
                return Some(node);
            }
        }

        None
    }

    /// Find all nodes with compatible string (recursive)
    fn find_all_compatible(&self, compatible: &str, results: &mut Vec<DeviceTreeNode>) {
        if self.is_compatible(compatible) {
            results.push(self.clone());
        }

        for child in &self.children {
            child.find_all_compatible(compatible, results);
        }
    }

    /// Find node by path parts
    fn find_by_path_parts(&self, parts: &[&str]) -> Option<DeviceTreeNode> {
        if parts.is_empty() {
            return Some(self.clone());
        }

        let name = parts[0];
        for child in &self.children {
            if child.name == name || child.name.starts_with(&format!("{}@", name)) {
                if parts.len() == 1 {
                    return Some(child.clone());
                } else {
                    return child.find_by_path_parts(&parts[1..]);
                }
            }
        }

        None
    }
}

/// Device tree property
#[derive(Clone, Debug)]
pub struct DeviceTreeProperty {
    /// Property name
    pub name: String,
    /// Property value (raw bytes)
    pub value: Vec<u8>,
}

/// Memory reservation
#[derive(Clone, Debug)]
pub struct MemoryReservation {
    /// Start address
    pub address: u64,
    /// Size
    pub size: u64,
}

/// Read cells from big-endian bytes
fn read_cells(data: &[u8], cells: usize) -> u64 {
    match cells {
        1 => u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as u64,
        2 => u64::from_be_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]),
        _ => 0,
    }
}

/// Initialize device tree subsystem
///
/// Takes the physical address of the DTB passed by the bootloader
pub fn init(dtb_addr: Option<u64>) {
    log::info!("Initializing Device Tree subsystem");

    if let Some(addr) = dtb_addr {
        if let Some(tree) = parse_dtb(addr) {
            log::info!("Device Tree: Parsed successfully");
            *DEVICE_TREE.write() = Some(tree);
        }
    } else {
        log::debug!("Device Tree: No DTB address provided");
    }
}

/// Get the device tree
pub fn get_device_tree() -> Option<DeviceTree> {
    DEVICE_TREE.read().clone()
}

/// Parse a Flattened Device Tree from memory
fn parse_dtb(addr: u64) -> Option<DeviceTree> {
    let virt = crate::arch::x86_64::paging::phys_to_virt(
        crate::mem::PhysAddr::new(addr)
    ).as_u64() as *const u8;

    // Read header
    let header = unsafe {
        let magic = u32::from_be(*(virt as *const u32));
        if magic != FDT_MAGIC {
            log::warn!("Device Tree: Invalid magic {:#x}", magic);
            return None;
        }

        FdtHeader {
            magic,
            totalsize: u32::from_be(*(virt.add(4) as *const u32)),
            off_dt_struct: u32::from_be(*(virt.add(8) as *const u32)),
            off_dt_strings: u32::from_be(*(virt.add(12) as *const u32)),
            off_mem_rsvmap: u32::from_be(*(virt.add(16) as *const u32)),
            version: u32::from_be(*(virt.add(20) as *const u32)),
            last_comp_version: u32::from_be(*(virt.add(24) as *const u32)),
            boot_cpuid_phys: u32::from_be(*(virt.add(28) as *const u32)),
            size_dt_strings: u32::from_be(*(virt.add(32) as *const u32)),
            size_dt_struct: u32::from_be(*(virt.add(36) as *const u32)),
        }
    };

    log::trace!("Device Tree: version {}, size {}", header.version, header.totalsize);

    // Read strings block
    let strings = unsafe {
        let strings_ptr = virt.add(header.off_dt_strings as usize);
        core::slice::from_raw_parts(strings_ptr, header.size_dt_strings as usize).to_vec()
    };

    // Parse structure block
    let struct_ptr = unsafe { virt.add(header.off_dt_struct as usize) };
    let (root, _) = parse_node(struct_ptr, &strings)?;

    // Parse memory reservations
    let mut memory_reservations = Vec::new();
    let mut rsvmap_ptr = unsafe { virt.add(header.off_mem_rsvmap as usize) };
    loop {
        let addr = unsafe { u64::from_be(*(rsvmap_ptr as *const u64)) };
        let size = unsafe { u64::from_be(*(rsvmap_ptr.add(8) as *const u64)) };
        if addr == 0 && size == 0 {
            break;
        }
        memory_reservations.push(MemoryReservation { address: addr, size });
        rsvmap_ptr = unsafe { rsvmap_ptr.add(16) };
    }

    Some(DeviceTree {
        root,
        memory_reservations,
        strings,
    })
}

/// FDT header
struct FdtHeader {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

/// FDT tokens
const FDT_BEGIN_NODE: u32 = 0x00000001;
const FDT_END_NODE: u32 = 0x00000002;
const FDT_PROP: u32 = 0x00000003;
const FDT_NOP: u32 = 0x00000004;
const FDT_END: u32 = 0x00000009;

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // DeviceTreeNode Tests
    // =========================================================================

    #[test]
    fn test_node_creation() {
        let node = DeviceTreeNode::new("test-node".to_string());
        assert_eq!(node.name, "test-node");
        assert!(node.properties.is_empty());
        assert!(node.children.is_empty());
    }

    #[test]
    fn test_node_property_lookup() {
        let mut node = DeviceTreeNode::new("test".to_string());
        node.properties.push(DeviceTreeProperty {
            name: "compatible".to_string(),
            value: b"arm,cortex-a72\0".to_vec(),
        });
        node.properties.push(DeviceTreeProperty {
            name: "reg".to_string(),
            value: vec![0, 0, 0, 0, 0, 0, 0x10, 0], // 4096 in big-endian
        });

        assert!(node.property("compatible").is_some());
        assert!(node.property("reg").is_some());
        assert!(node.property("nonexistent").is_none());
    }

    #[test]
    fn test_property_u32() {
        let mut node = DeviceTreeNode::new("test".to_string());
        // 0x12345678 in big-endian
        node.properties.push(DeviceTreeProperty {
            name: "test-value".to_string(),
            value: vec![0x12, 0x34, 0x56, 0x78],
        });

        let value = node.property_u32("test-value");
        assert_eq!(value, Some(0x12345678));
    }

    #[test]
    fn test_property_u32_too_short() {
        let mut node = DeviceTreeNode::new("test".to_string());
        node.properties.push(DeviceTreeProperty {
            name: "short".to_string(),
            value: vec![0x12, 0x34], // Only 2 bytes
        });

        assert_eq!(node.property_u32("short"), None);
    }

    #[test]
    fn test_property_u64() {
        let mut node = DeviceTreeNode::new("test".to_string());
        // 0x123456789ABCDEF0 in big-endian
        node.properties.push(DeviceTreeProperty {
            name: "test-addr".to_string(),
            value: vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0],
        });

        let value = node.property_u64("test-addr");
        assert_eq!(value, Some(0x123456789ABCDEF0));
    }

    #[test]
    fn test_property_u64_from_u32() {
        let mut node = DeviceTreeNode::new("test".to_string());
        // 0x12345678 in big-endian (only 4 bytes, should work as u64)
        node.properties.push(DeviceTreeProperty {
            name: "test-addr".to_string(),
            value: vec![0x12, 0x34, 0x56, 0x78],
        });

        let value = node.property_u64("test-addr");
        assert_eq!(value, Some(0x12345678));
    }

    #[test]
    fn test_property_string() {
        let mut node = DeviceTreeNode::new("test".to_string());
        node.properties.push(DeviceTreeProperty {
            name: "model".to_string(),
            value: b"Raspberry Pi 4\0".to_vec(),
        });

        let model = node.property_string("model");
        assert_eq!(model, Some("Raspberry Pi 4"));
    }

    #[test]
    fn test_property_strings() {
        let mut node = DeviceTreeNode::new("test".to_string());
        // Multiple null-terminated strings
        node.properties.push(DeviceTreeProperty {
            name: "compatible".to_string(),
            value: b"apple,m1-gpu\0apple,agx-g13g\0".to_vec(),
        });

        let strings = node.property_strings("compatible");
        assert_eq!(strings.len(), 2);
        assert_eq!(strings[0], "apple,m1-gpu");
        assert_eq!(strings[1], "apple,agx-g13g");
    }

    #[test]
    fn test_compatible() {
        let mut node = DeviceTreeNode::new("test".to_string());
        node.properties.push(DeviceTreeProperty {
            name: "compatible".to_string(),
            value: b"arm,gic-400\0arm,cortex-a15-gic\0".to_vec(),
        });

        let compat = node.compatible();
        assert_eq!(compat.len(), 2);
        assert!(compat.contains(&"arm,gic-400"));
        assert!(compat.contains(&"arm,cortex-a15-gic"));
    }

    #[test]
    fn test_is_compatible() {
        let mut node = DeviceTreeNode::new("test".to_string());
        node.properties.push(DeviceTreeProperty {
            name: "compatible".to_string(),
            value: b"nvidia,tegra210-gpu\0nvidia,gm20b\0".to_vec(),
        });

        assert!(node.is_compatible("nvidia,tegra210-gpu"));
        assert!(node.is_compatible("nvidia,gm20b"));
        assert!(!node.is_compatible("nvidia,xavier"));
    }

    #[test]
    fn test_interrupts() {
        let mut node = DeviceTreeNode::new("test".to_string());
        // Three 32-bit interrupt values
        node.properties.push(DeviceTreeProperty {
            name: "interrupts".to_string(),
            value: vec![
                0x00, 0x00, 0x00, 0x20, // 32
                0x00, 0x00, 0x00, 0x21, // 33
                0x00, 0x00, 0x00, 0x22, // 34
            ],
        });

        let irqs = node.interrupts();
        assert_eq!(irqs.len(), 3);
        assert_eq!(irqs[0], 32);
        assert_eq!(irqs[1], 33);
        assert_eq!(irqs[2], 34);
    }

    #[test]
    fn test_find_child() {
        let mut node = DeviceTreeNode::new("soc".to_string());
        node.children.push(DeviceTreeNode::new("uart0".to_string()));
        node.children.push(DeviceTreeNode::new("uart1@fe201000".to_string()));
        node.children.push(DeviceTreeNode::new("gpio".to_string()));

        // Exact match
        assert!(node.find_child("uart0").is_some());
        assert_eq!(node.find_child("uart0").unwrap().name, "uart0");

        // Match with @address suffix
        assert!(node.find_child("uart1").is_some());
        assert_eq!(node.find_child("uart1").unwrap().name, "uart1@fe201000");

        // No match
        assert!(node.find_child("spi").is_none());
    }

    // =========================================================================
    // DeviceTree Tests
    // =========================================================================

    #[test]
    fn test_find_compatible_recursive() {
        let mut gpu = DeviceTreeNode::new("gpu".to_string());
        gpu.properties.push(DeviceTreeProperty {
            name: "compatible".to_string(),
            value: b"arm,mali-g76\0".to_vec(),
        });

        let mut soc = DeviceTreeNode::new("soc".to_string());
        soc.children.push(gpu);

        let tree = DeviceTree {
            root: soc,
            memory_reservations: Vec::new(),
            strings: Vec::new(),
        };

        let found = tree.find_compatible("arm,mali-g76");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "gpu");
    }

    #[test]
    fn test_find_all_compatible() {
        let mut uart0 = DeviceTreeNode::new("uart0".to_string());
        uart0.properties.push(DeviceTreeProperty {
            name: "compatible".to_string(),
            value: b"ns16550a\0".to_vec(),
        });

        let mut uart1 = DeviceTreeNode::new("uart1".to_string());
        uart1.properties.push(DeviceTreeProperty {
            name: "compatible".to_string(),
            value: b"ns16550a\0".to_vec(),
        });

        let mut soc = DeviceTreeNode::new("soc".to_string());
        soc.children.push(uart0);
        soc.children.push(uart1);

        let tree = DeviceTree {
            root: soc,
            memory_reservations: Vec::new(),
            strings: Vec::new(),
        };

        let found = tree.find_all_compatible("ns16550a");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_find_by_path_root() {
        let root = DeviceTreeNode::new("".to_string());
        let tree = DeviceTree {
            root,
            memory_reservations: Vec::new(),
            strings: Vec::new(),
        };

        let found = tree.find_by_path("/");
        assert!(found.is_some());
    }

    #[test]
    fn test_find_by_path_nested() {
        let gpu = DeviceTreeNode::new("gpu".to_string());

        let mut soc = DeviceTreeNode::new("soc".to_string());
        soc.children.push(gpu);

        let mut root = DeviceTreeNode::new("".to_string());
        root.children.push(soc);

        let tree = DeviceTree {
            root,
            memory_reservations: Vec::new(),
            strings: Vec::new(),
        };

        let found = tree.find_by_path("/soc/gpu");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "gpu");
    }

    // =========================================================================
    // Helper Function Tests
    // =========================================================================

    #[test]
    fn test_read_cells_1() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let value = read_cells(&data, 1);
        assert_eq!(value, 0x12345678);
    }

    #[test]
    fn test_read_cells_2() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let value = read_cells(&data, 2);
        assert_eq!(value, 0x123456789ABCDEF0);
    }

    #[test]
    fn test_read_cells_unsupported() {
        let data = [0; 16];
        let value = read_cells(&data, 3); // Unsupported
        assert_eq!(value, 0);
    }

    // =========================================================================
    // MemoryReservation Tests
    // =========================================================================

    #[test]
    fn test_memory_reservation() {
        let reservation = MemoryReservation {
            address: 0x8000_0000,
            size: 0x1000_0000,
        };

        assert_eq!(reservation.address, 0x8000_0000);
        assert_eq!(reservation.size, 0x1000_0000);
    }

    // =========================================================================
    // FDT Constants Tests
    // =========================================================================

    #[test]
    fn test_fdt_magic() {
        assert_eq!(FDT_MAGIC, 0xD00DFEED);
    }

    #[test]
    fn test_fdt_tokens() {
        assert_eq!(FDT_BEGIN_NODE, 0x00000001);
        assert_eq!(FDT_END_NODE, 0x00000002);
        assert_eq!(FDT_PROP, 0x00000003);
        assert_eq!(FDT_NOP, 0x00000004);
        assert_eq!(FDT_END, 0x00000009);
    }
}

/// Parse a device tree node
fn parse_node(ptr: *const u8, strings: &[u8]) -> Option<(DeviceTreeNode, *const u8)> {
    let mut current = ptr;

    // Read token
    let token = unsafe { u32::from_be(*(current as *const u32)) };
    if token != FDT_BEGIN_NODE {
        return None;
    }
    current = unsafe { current.add(4) };

    // Read name
    let name_start = current;
    let mut name_len = 0;
    while unsafe { *current.add(name_len) } != 0 {
        name_len += 1;
    }
    let name = unsafe {
        core::str::from_utf8(core::slice::from_raw_parts(name_start, name_len))
            .ok()?
            .to_string()
    };
    current = unsafe { current.add(name_len + 1) };
    // Align to 4 bytes
    let offset = current as usize;
    current = (((offset + 3) / 4) * 4) as *const u8;

    let mut node = DeviceTreeNode::new(name);

    // Parse properties and children
    loop {
        let token = unsafe { u32::from_be(*(current as *const u32)) };
        current = unsafe { current.add(4) };

        match token {
            FDT_PROP => {
                let len = unsafe { u32::from_be(*(current as *const u32)) } as usize;
                let nameoff = unsafe { u32::from_be(*(current.add(4) as *const u32)) } as usize;
                current = unsafe { current.add(8) };

                // Get property name from strings block
                let prop_name = {
                    let name_end = strings[nameoff..].iter().position(|&b| b == 0).unwrap_or(0);
                    core::str::from_utf8(&strings[nameoff..nameoff + name_end])
                        .ok()?
                        .to_string()
                };

                let value = unsafe { core::slice::from_raw_parts(current, len).to_vec() };
                current = unsafe { current.add(len) };
                // Align to 4 bytes
                let offset = current as usize;
                current = (((offset + 3) / 4) * 4) as *const u8;

                node.properties.push(DeviceTreeProperty {
                    name: prop_name,
                    value,
                });
            }
            FDT_BEGIN_NODE => {
                current = unsafe { current.sub(4) }; // Back up to include token
                let (child, next) = parse_node(current, strings)?;
                node.children.push(child);
                current = next;
            }
            FDT_END_NODE => {
                return Some((node, current));
            }
            FDT_NOP => {
                // Skip
            }
            FDT_END => {
                return Some((node, current));
            }
            _ => {
                log::warn!("Device Tree: Unknown token {:#x}", token);
                return None;
            }
        }
    }
}
