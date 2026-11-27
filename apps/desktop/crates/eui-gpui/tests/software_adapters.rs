//! #EXP-2211: lavapipe (Mesa's software Vulkan) before Mesa 24 presents only
//! black to gpui's 32-bit-visual X11 windows (Ubuntu 22.04's Mesa 23.2 under
//! Xvfb, the AppImage catalog's test), so the vendored `gpui-pre-wgpu` tries
//! every other adapter (llvmpipe over GL) first. Only that adapter yields.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

use gpui_wgpu::wgpu::{AdapterInfo, Backend, DeviceType};
use gpui_wgpu::{is_black_presenting_lavapipe, mesa_version};

const MESA_23: &str = "Mesa 23.2.1-1ubuntu3.1~22.04.4 (LLVM 15.0.7)";
const MESA_24: &str = "Mesa 24.0.5-1ubuntu1 (LLVM 17.0.6)";

fn adapter(backend: Backend, device_type: DeviceType, driver: &str, driver_info: &str) -> AdapterInfo {
    AdapterInfo {
        name: "llvmpipe (LLVM 15.0.7, 256 bits)".into(),
        vendor: 0x10005,
        device: 0,
        device_type,
        device_pci_bus_id: String::new(),
        driver: driver.into(),
        driver_info: driver_info.into(),
        backend,
        subgroup_min_size: 4,
        subgroup_max_size: 8,
        transient_saves_memory: None,
        limit_bucket: None,
    }
}

#[test]
fn mesa_versions_parse() {
    assert_eq!(mesa_version(MESA_23), Some((23, 2)));
    assert_eq!(mesa_version(MESA_24), Some((24, 0)));
    assert_eq!(mesa_version("Mesa 25.2.8-0ubuntu0.24.04.5 (LLVM 20.1.2)"), Some((25, 2)));
    assert_eq!(mesa_version("NVIDIA 580.95.05"), None);
    assert_eq!(mesa_version(""), None);
}

#[test]
fn only_lavapipe_before_mesa_24_yields() {
    assert!(is_black_presenting_lavapipe(&adapter(Backend::Vulkan, DeviceType::Cpu, "llvmpipe", MESA_23)));
    assert!(!is_black_presenting_lavapipe(&adapter(Backend::Vulkan, DeviceType::Cpu, "llvmpipe", MESA_24)));
    // Its GL twin is the one that paints.
    assert!(!is_black_presenting_lavapipe(&adapter(Backend::Gl, DeviceType::Cpu, "llvmpipe", MESA_23)));
    // Hardware Mesa drivers present over DRI3, untouched.
    assert!(!is_black_presenting_lavapipe(&adapter(Backend::Vulkan, DeviceType::IntegratedGpu, "radv", MESA_23)));
}
