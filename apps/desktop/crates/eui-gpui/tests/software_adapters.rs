//! #EXP-2211: lavapipe (Mesa's software Vulkan) before Mesa 24 presents only
//! black to gpui's 32-bit-visual X11 windows (Ubuntu 22.04's Mesa 23.2 under
//! Xvfb, the AppImage catalog's test), so the vendored `gpui-pre-wgpu` tries
//! every other adapter (llvmpipe over GL) first. Only that adapter yields.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

use gpui_wgpu::wgpu::{AdapterInfo, Backend, DeviceType};
use gpui_wgpu::{CompositorGpuHint, WgpuContext, is_black_presenting_lavapipe, mesa_version, outranks_opengl};

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

/// #PERF-17: the first window tries Vulkan alone (no EGL start-up) only with
/// the adapters that sort ahead of every adapter OpenGL can expose; anything
/// else (lavapipe, a virtual GPU) goes through upstream's Vulkan + GL pass, so
/// the adapter picked never changes.
#[test]
fn vulkan_first_takes_exactly_the_adapters_ranked_ahead_of_opengl() {
    let gpu = |device_type: DeviceType, backend: Backend, device: u32| {
        let mut info = adapter(backend, device_type, "", "");
        info.vendor = 0x10de;
        info.device = device;
        info
    };
    assert!(outranks_opengl(&gpu(DeviceType::DiscreteGpu, Backend::Vulkan, 0x2684), None, None));
    assert!(outranks_opengl(&gpu(DeviceType::IntegratedGpu, Backend::Vulkan, 0xa780), None, None));
    assert!(!outranks_opengl(&gpu(DeviceType::Other, Backend::Vulkan, 1), None, None));
    assert!(!outranks_opengl(&gpu(DeviceType::VirtualGpu, Backend::Vulkan, 1), None, None));
    assert!(!outranks_opengl(&adapter(Backend::Vulkan, DeviceType::Cpu, "llvmpipe", MESA_24), None, None));
    // A compositor or ZED_DEVICE_ID match outranks OpenGL whatever its type.
    let hint = CompositorGpuHint { vendor_id: 0x10de, device_id: 7 };
    assert!(outranks_opengl(&gpu(DeviceType::Cpu, Backend::Vulkan, 7), None, Some(&hint)));
    assert!(outranks_opengl(&gpu(DeviceType::VirtualGpu, Backend::Vulkan, 7), Some(7), None));
    // wgpu-hal's gles reports device 0 and Integrated, Other or Cpu: never ahead
    // of the cut, even against a hint or filter naming device 0.
    let zero = CompositorGpuHint { vendor_id: 0, device_id: 0 };
    for device_type in [DeviceType::IntegratedGpu, DeviceType::Other, DeviceType::Cpu] {
        let mut gl = adapter(Backend::Gl, device_type, "", "");
        gl.vendor = 0;
        assert!(!outranks_opengl(&gl, Some(0), Some(&zero)));
        assert!(WgpuContext::adapter_rank(&gl, Some(0), Some(&zero)) >= (1, 1, 1, 1));
    }
}
