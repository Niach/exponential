//! #EDE-138 headless: an app that asks for the GPU in a space still draws.
//! A space takes shared memory buffers alone until spaces have a GPU path
//! (#EDE-162): while it renders in software its Vulkan apps see lavapipe
//! alone, and GL falls back to llvmpipe by itself. Needs lavapipe,
//! vulkan-tools (`vkcube`, `vulkaninfo`) and mesa-utils (`eglinfo`):
//! skipped without them, a failure where `EDE_EXPECT_SOFTWARE_RENDERING=1`
//! (the ede-scope job).

mod common;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use common::space::start_with_spaces;
use ede_compositor::{desk::DeskId, spaces::render::lavapipe_icds};
use ede_protocol::{Capability, CaptureTarget, SessionId};

/// `program` on `PATH`.
fn installed(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(program)).find(|path| path.is_file())
}

/// What a launch wrote to `file` once it is whole (`<file>.tmp`, renamed).
fn written(file: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !file.exists() {
        assert!(Instant::now() < deadline, "{} was never written", file.display());
        std::thread::sleep(Duration::from_millis(50));
    }
    std::fs::read_to_string(file).unwrap()
}

/// `sh -c "<script> > file"`, whole or not at all.
fn into(script: &str, file: &Path) -> Vec<String> {
    vec!["sh".into(), "-c".into(), format!("{script} > {0}.tmp 2>&1; mv {0}.tmp {0}; exec sleep 60", file.display())]
}

#[test]
fn a_spaces_apps_that_ask_for_the_gpu_draw_in_software() {
    let vkcube = installed("vkcube-wayland").map(|path| vec![path.to_string_lossy().into_owned()]).or_else(|| installed("vkcube").map(|path| vec![path.to_string_lossy().into_owned(), "--wsi".into(), "wayland".into()]));
    let mut missing = Vec::new();
    if vkcube.is_none() {
        missing.push("vkcube");
    }
    for program in ["vulkaninfo", "eglinfo"] {
        if installed(program).is_none() {
            missing.push(program);
        }
    }
    if lavapipe_icds(Path::new("/"), std::env::var_os("XDG_DATA_DIRS").as_deref()).is_empty() {
        missing.push("lavapipe");
    }
    if !missing.is_empty() {
        assert!(std::env::var_os("EDE_EXPECT_SOFTWARE_RENDERING").is_none(), "this check needs {missing:?}");
        eprintln!("skipped: no {missing:?}");
        return;
    }
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let grant = h.ipc().mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    let id = space.id.clone();
    let desk: DeskId = h.with(move |state| state.spaces.iter().find(|s| s.id == id).map(|s| s.desk.id)).expect("the space");
    let out = |name: &str| dir.path().join(name);

    // Vulkan sees one device, Mesa's on the CPU, whatever GPUs the machine has.
    let devices = written(&{
        let file = out("vulkaninfo.txt");
        agent.launch(into("vulkaninfo --summary", &file), Default::default(), None).unwrap();
        file
    });
    let types: Vec<&str> = devices.lines().filter_map(|line| line.trim().strip_prefix("deviceType")).map(|line| line.trim_start_matches([' ', '=']).trim()).collect();
    assert_eq!(types, ["PHYSICAL_DEVICE_TYPE_CPU"], "{devices}");
    assert!(devices.contains("llvmpipe"), "{devices}");

    // GL on the space's display: llvmpipe, by Mesa's own fallback.
    let egl = written(&{
        let file = out("eglinfo.txt");
        agent.launch(into("eglinfo -B -p wayland", &file), Default::default(), None).unwrap();
        file
    });
    let renderers: Vec<&str> = egl.lines().filter(|line| line.contains("renderer")).collect();
    assert!(!renderers.is_empty() && renderers.iter().all(|line| line.contains("llvmpipe")), "{egl}");

    // A Vulkan app that presents: vkcube's frames reach the space.
    let empty = agent.capture(CaptureTarget::Output { name: space.output.clone() }).unwrap().at(320, 240);
    agent.launch(vkcube.unwrap(), Default::default(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let shot = agent.capture(CaptureTarget::Output { name: space.output.clone() }).unwrap();
        let drawn = (0..shot.info.height).step_by(8).flat_map(|y| (0..shot.info.width).step_by(8).map(move |x| (x, y))).filter(|(x, y)| shot.at(*x, *y) != empty).count();
        if drawn > 1000 {
            break;
        }
        assert!(Instant::now() < deadline, "vkcube never drew in the space ({drawn} samples drawn)");
        std::thread::sleep(Duration::from_millis(100));
    }

    // Pointed by the variable ANGLE replaces with its SwiftShader alone
    // (#EDE-180): vulkaninfo above saw lavapipe alone through it.
    let env = written(&{
        let file = out("env-software.txt");
        agent.launch(into("env", &file), Default::default(), None).unwrap();
        file
    });
    assert!(env.lines().any(|line| line.starts_with("VK_ICD_FILENAMES=") && line.contains("lvp_icd")), "{env}");
    assert!(!env.lines().any(|line| line.starts_with("VK_DRIVER_FILES=")), "{env}");

    // The switch off (a space with a GPU path, #EDE-162): Vulkan is left to
    // find its devices.
    assert!(h.with(move |state| state.set_space_software_rendering(desk, false)));
    let env = written(&{
        let file = out("env.txt");
        agent.launch(into("env", &file), Default::default(), None).unwrap();
        file
    });
    assert!(!env.lines().any(|line| line.starts_with("VK_DRIVER_FILES=") || line.starts_with("VK_ICD_FILENAMES=")), "{env}");
    agent.destroy_space(space.id.clone()).unwrap();
}
