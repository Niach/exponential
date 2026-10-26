//! The pinned CrispASR runtime per platform: CrispASR v0.8.45's own release
//! assets (sha256 = GitHub's published asset digest), loaded at runtime.
//!
//! - Linux x86_64: the CUDA build, downloaded. It links `libcudart.so.12` and
//!   `libcublas.so.12`, which only a CUDA toolkit install has, so NVIDIA's
//!   own redistributable wheels (PyPI, NVIDIA's EULA; fetched by this
//!   machine, never shipped by us) land in the same directory, where the
//!   library's `$ORIGIN` runpath finds them first. Needs the NVIDIA driver.
//! - macOS arm64: the Metal build, SHIPPED INSIDE the signed bundle
//!   (`Contents/Frameworks`, put there and signed with the app's identity by
//!   `scripts/build-macos-app.sh`, which pins it), so library validation
//!   stays on and notarization covers it. Loaded by absolute path from the
//!   bundle of the running executable only ([`bundled_dir`]); a bare
//!   `cargo run` binary has no bundle and so no GPU voice.
//! - Windows x86_64: the Vulkan build, downloaded. Needs a Vulkan driver.
//! - Anything else: no GPU runtime.

// `Pack` is only used where the runtime is downloaded (Linux, Windows).
#[allow(unused_imports)]
use crate::artifact::{Artifact, Pack};

/// The directory under `{data_dir}/talk/` the runtime unpacks into.
pub const RUNTIME_DIR: &str = "crispasr-0.8.45";

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod imp {
    use super::*;

    pub const GPU_BACKEND: &str = "cuda";
    pub const LIBRARY: &str = "libcrispasr.so.1";
    pub const GGML: &str = "libggml.so.0";

    pub const ARTIFACTS: &[Artifact] = &[
        Artifact {
            id: "libcrispasr-linux-x86_64-cuda",
            url: "https://github.com/CrispStrobe/CrispASR/releases/download/v0.8.45/libcrispasr-linux-x86_64-cuda.tar.gz",
            sha256: "30cd8a533b212d9b5c42321fc6d60aa3230c0d73d9f6d91b0e0a9776ab16ed05",
            size: 196_475_340,
            pack: Pack::TarGz,
            keep: &[
                ("libcrispasr-linux-x86_64-cuda/lib/libcrispasr.so.0.8.45", "libcrispasr.so.1"),
                ("libcrispasr-linux-x86_64-cuda/lib/libggml.so.0.26.0", "libggml.so.0"),
                ("libcrispasr-linux-x86_64-cuda/lib/libggml-base.so.0.26.0", "libggml-base.so.0"),
                ("libcrispasr-linux-x86_64-cuda/lib/libggml-cpu.so.0.26.0", "libggml-cpu.so.0"),
                ("libcrispasr-linux-x86_64-cuda/lib/libggml-cuda.so.0.26.0", "libggml-cuda.so.0"),
                ("libcrispasr-linux-x86_64-cuda/lib/libgomp.so.1", "libgomp.so.1"),
                ("libcrispasr-linux-x86_64-cuda/lib/libblas.so.3", "libblas.so.3"),
                ("libcrispasr-linux-x86_64-cuda/LICENSE", "LICENSE"),
                ("libcrispasr-linux-x86_64-cuda/THIRD_PARTY_NOTICES.txt", "THIRD_PARTY_NOTICES.txt"),
            ],
        },
        Artifact {
            id: "nvidia-cuda-runtime-cu12",
            url: "https://files.pythonhosted.org/packages/bc/46/a92db19b8309581092a3add7e6fceb4c301a3fd233969856a8cbf042cd3c/nvidia_cuda_runtime_cu12-12.9.79-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl",
            sha256: "25bba2dfb01d48a9b59ca474a1ac43c6ebf7011f1b0b8cc44f54eb6ac48a96c3",
            size: 3_493_179,
            pack: Pack::Zip,
            keep: &[
                ("nvidia/cuda_runtime/lib/libcudart.so.12", "libcudart.so.12"),
                ("nvidia_cuda_runtime_cu12-12.9.79.dist-info/licenses/License.txt", "NVIDIA-CUDA-RUNTIME-LICENSE.txt"),
            ],
        },
        Artifact {
            id: "nvidia-cublas-cu12",
            url: "https://files.pythonhosted.org/packages/cb/c0/0a517bfe63ccd3b92eb254d264e28fca3c7cab75d07daea315250fb1bf73/nvidia_cublas_cu12-12.9.2.10-py3-none-manylinux_2_27_x86_64.whl",
            sha256: "e4f53a8ca8c5d6e8c492d0d0a3d565ecb59a751b19cfdaa4f6da0ab2104c1702",
            size: 581_240_110,
            pack: Pack::Zip,
            keep: &[
                ("nvidia/cublas/lib/libcublas.so.12", "libcublas.so.12"),
                ("nvidia/cublas/lib/libcublasLt.so.12", "libcublasLt.so.12"),
                ("nvidia_cublas_cu12-12.9.2.10.dist-info/licenses/License.txt", "NVIDIA-CUBLAS-LICENSE.txt"),
            ],
        },
    ];

    /// The NVIDIA driver is installed (its user-space library loads).
    pub fn gpu_driver_present() -> bool {
        // SAFETY: loading the driver's library runs its initialisers only.
        unsafe { libloading::Library::new("libcuda.so.1") }.is_ok()
    }

    pub fn bundled_dir() -> Option<std::path::PathBuf> {
        None
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod imp {
    use super::Artifact;

    pub const GPU_BACKEND: &str = "metal";
    pub const LIBRARY: &str = "libcrispasr.1.dylib";
    pub const GGML: &str = "libggml.0.dylib";

    /// Nothing is downloaded: the runtime ships in the bundle.
    pub const ARTIFACTS: &[Artifact] = &[];

    /// Every Apple Silicon Mac has a Metal GPU.
    pub fn gpu_driver_present() -> bool {
        true
    }

    /// `<bundle>.app/Contents/Frameworks` of the RUNNING executable
    /// (`Contents/MacOS/exp-desktop`), when it holds the runtime.
    pub fn bundled_dir() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
        let macos = exe.parent()?;
        if macos.file_name()? != "MacOS" {
            return None;
        }
        let frameworks = macos.parent()?.join("Frameworks");
        frameworks.join(LIBRARY).is_file().then_some(frameworks)
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod imp {
    use super::*;

    pub const GPU_BACKEND: &str = "vulkan";
    pub const LIBRARY: &str = "crispasr.dll";
    pub const GGML: &str = "ggml.dll";

    pub const ARTIFACTS: &[Artifact] = &[Artifact {
        id: "libcrispasr-windows-x86_64-vulkan",
        url: "https://github.com/CrispStrobe/CrispASR/releases/download/v0.8.45/libcrispasr-windows-x86_64-vulkan.tar.gz",
        sha256: "79c575d1143e4316551a84881fb93bc07b64c5834195873b2845ba4ad1bb4b95",
        size: 86_762_009,
        pack: Pack::TarGz,
        keep: &[
            ("libcrispasr-windows-x86_64-vulkan/bin/crispasr.dll", "crispasr.dll"),
            ("libcrispasr-windows-x86_64-vulkan/bin/ggml.dll", "ggml.dll"),
            ("libcrispasr-windows-x86_64-vulkan/bin/ggml-base.dll", "ggml-base.dll"),
            ("libcrispasr-windows-x86_64-vulkan/bin/ggml-cpu.dll", "ggml-cpu.dll"),
            ("libcrispasr-windows-x86_64-vulkan/bin/ggml-vulkan.dll", "ggml-vulkan.dll"),
            ("libcrispasr-windows-x86_64-vulkan/LICENSE", "LICENSE"),
            ("libcrispasr-windows-x86_64-vulkan/THIRD_PARTY_NOTICES.txt", "THIRD_PARTY_NOTICES.txt"),
        ],
    }];

    /// A Vulkan driver is installed (the loader library loads).
    pub fn gpu_driver_present() -> bool {
        // SAFETY: loading the Vulkan loader runs its initialisers only.
        unsafe { libloading::Library::new("vulkan-1.dll") }.is_ok()
    }

    pub fn bundled_dir() -> Option<std::path::PathBuf> {
        None
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64"),
)))]
mod imp {
    use super::Artifact;

    pub const GPU_BACKEND: &str = "cpu";
    pub const LIBRARY: &str = "";
    pub const GGML: &str = "";
    pub const ARTIFACTS: &[Artifact] = &[];

    pub fn gpu_driver_present() -> bool {
        false
    }

    pub fn bundled_dir() -> Option<std::path::PathBuf> {
        None
    }
}

/// The GPU backend CrispASR is asked for (`crispasr_set_gpu_backend`).
pub use imp::GPU_BACKEND;
/// The runtime's downloaded library files, every one pinned (empty where
/// the runtime ships in the app bundle).
pub use imp::ARTIFACTS;
/// The runtime inside the running app's bundle (macOS), if any.
pub use imp::bundled_dir;
/// The library opened (by absolute path, inside the runtime dir).
pub use imp::LIBRARY;
/// ggml's library (already loaded as a dependency; device enumeration).
pub use imp::GGML;
/// The cheap check BEFORE any download: the platform's GPU driver exists.
pub use imp::gpu_driver_present;

/// Whether this platform has a pinned GPU runtime at all.
pub fn supported() -> bool {
    !ARTIFACTS.is_empty() || bundled_dir().is_some()
}
