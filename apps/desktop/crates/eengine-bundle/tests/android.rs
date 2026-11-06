//! `--target android` end to end on a small synthetic kit (the real one is
//! `scripts/runner-kits/android.sh`): the album sample → the .apk + .aab,
//! checked with the SDK's own tools. Runs where the toolchain is (ANDROID_HOME
//! with build-tools + a platform, a JDK, BUNDLETOOL); prints why it skips otherwise.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use eengine_bundle::run::{run, Options};

const TEMPLATE_ID: &str = "at.exponential.runner.template";

struct Sdk {
    build_tools: PathBuf,
    platform: (u64, PathBuf),
    java_bin: PathBuf,
}

fn sdk() -> Result<Sdk, String> {
    let home = std::env::var_os("ANDROID_HOME").or_else(|| std::env::var_os("ANDROID_SDK_ROOT")).map(PathBuf::from).ok_or("no ANDROID_HOME")?;
    let newest = |dir: &str, keep: &dyn Fn(&Path) -> bool| -> Option<PathBuf> {
        let mut all: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(home.join(dir)).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| keep(p)).map(|p| (p.file_name().unwrap().to_string_lossy().split(['.', '-']).filter_map(|n| n.parse().ok()).collect(), p)).collect();
        all.sort();
        all.pop().map(|(_, p)| p)
    };
    let build_tools = newest("build-tools", &|p| p.join("aapt2").is_file() && p.join("apksigner").is_file()).ok_or("no build-tools")?;
    let platform = newest("platforms", &|p| p.join("android.jar").is_file()).ok_or("no platform")?;
    let level = platform.file_name().unwrap().to_string_lossy().trim_start_matches("android-").parse().map_err(|_| "a platform with no API level")?;
    let java_bin = std::env::var_os("JAVA_HOME").map(|h| PathBuf::from(h).join("bin")).filter(|b| b.join("java").is_file()).ok_or("no JAVA_HOME")?;
    if std::env::var_os("BUNDLETOOL").is_none() {
        return Err("no BUNDLETOOL".into());
    }
    Ok(Sdk { build_tools, platform: (level, platform.join("android.jar")), java_bin })
}

fn sh(cmd: &mut Command, java_bin: &Path) -> String {
    let path = format!("{}:{}", java_bin.display(), std::env::var("PATH").unwrap_or_default());
    let out = cmd.env("PATH", path).env("LC_ALL", "C").output().expect("spawn");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{cmd:?} failed:\n{text}");
    text
}

/// A kit shaped like android.sh's: a merged-style manifest, compiled resources + their ids, a payload.
fn make_kit(sdk: &Sdk, kit: &Path) {
    std::fs::create_dir_all(kit.join("src/res/values")).unwrap();
    std::fs::create_dir_all(kit.join("src/res/mipmap")).unwrap();
    std::fs::write(kit.join("src/res/values/strings.xml"), "<resources><string name=\"runner_label\">Runner</string></resources>").unwrap();
    std::fs::write(kit.join("src/res/mipmap/ic_launcher.xml"), "<vector xmlns:android=\"http://schemas.android.com/apk/res/android\" android:width=\"48dp\" android:height=\"48dp\" android:viewportWidth=\"48\" android:viewportHeight=\"48\"><path android:fillColor=\"#FF000000\" android:pathData=\"M0,0h48v48h-48z\"/></vector>").unwrap();
    let manifest = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\" package=\"{TEMPLATE_ID}\" android:versionCode=\"1\" android:versionName=\"0.0.0\">\n    <uses-sdk android:minSdkVersion=\"26\" android:targetSdkVersion=\"{level}\" />\n    <!-- E7:PERMISSIONS -->\n    <!-- /E7:PERMISSIONS -->\n    <application android:icon=\"@mipmap/ic_launcher\" android:roundIcon=\"@mipmap/ic_launcher\" android:label=\"@string/runner_label\">\n        <activity android:name=\"at.exponential.runner.RunnerActivity\" android:exported=\"true\">\n            <intent-filter><action android:name=\"android.intent.action.MAIN\" /><category android:name=\"android.intent.category.LAUNCHER\" /></intent-filter>\n            <intent-filter><action android:name=\"android.intent.action.VIEW\" /><category android:name=\"android.intent.category.DEFAULT\" /><category android:name=\"android.intent.category.BROWSABLE\" /><data android:scheme=\"{TEMPLATE_ID}\" /></intent-filter>\n        </activity>\n        <provider android:name=\"androidx.core.content.FileProvider\" android:authorities=\"{TEMPLATE_ID}.share\" android:exported=\"false\" />\n    </application>\n</manifest>\n",
        level = sdk.platform.0
    );
    std::fs::write(kit.join("AndroidManifest.xml"), &manifest).unwrap();
    let aapt2 = sdk.build_tools.join("aapt2");
    sh(Command::new(&aapt2).arg("compile").arg("--dir").arg(kit.join("src/res")).arg("-o").arg(kit.join("resources.zip")), &sdk.java_bin);
    sh(Command::new(&aapt2).arg("link").arg("-o").arg(kit.join("src/template.apk")).arg("-I").arg(&sdk.platform.1).arg("--manifest").arg(kit.join("AndroidManifest.xml")).arg("--emit-ids").arg(kit.join("resource-ids.txt")).arg(kit.join("resources.zip")), &sdk.java_bin);
    let mut payload = zip::ZipWriter::new(std::fs::File::create(kit.join("payload.zip")).unwrap());
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, opts, bytes) in [("classes.dex", deflated, &b"dex\n035\0"[..]), ("lib/x86_64/libeengine_ffi.so", stored, &[0x7f, b'E', b'L', b'F'][..]), ("META-INF/services/x", deflated, &b"y"[..])] {
        payload.start_file(name, opts).unwrap();
        payload.write_all(bytes).unwrap();
    }
    payload.finish().unwrap();
    std::fs::write(kit.join("kit.json"), format!("{{\"templateId\": \"{TEMPLATE_ID}\", \"compileSdk\": {}}}", sdk.platform.0)).unwrap();
    std::fs::write(kit.join("NOTICES.txt"), "stub notices\n").unwrap();
}

#[test]
fn the_album_sample_bundles_to_a_signed_apk_and_aab() {
    let sdk = match sdk() {
        Ok(s) => s,
        Err(why) => return eprintln!("skipped: {why}"),
    };
    let tmp = tempfile::tempdir().unwrap();
    let runners = tmp.path().join("kits");
    make_kit(&sdk, &runners.join("android"));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    std::fs::write(tmp.path().join("icon.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#2563eb\"/><circle cx=\"32\" cy=\"32\" r=\"14\" fill=\"#fff\"/></svg>").unwrap();
    std::fs::write(tmp.path().join("bundle.toml"), "id = \"at.acme.album\"\nname = \"Album\"\nversion = \"1.2.3\"\n").unwrap();
    let out = tmp.path().join("dist");
    let opts = Options {
        package: root.join("apps/runner/web/samples/album.app.json"),
        icon: tmp.path().join("icon.svg"),
        config: tmp.path().join("bundle.toml"),
        targets: vec!["android".into()],
        runners: Some(runners),
        out: out.clone(),
        allow_unsigned: true,
        cache: tmp.path().join("cache"),
    };
    let done = run(&opts).expect("bundle");
    let (_, output) = &done[0];
    assert!(output.notes.iter().any(|n| n.contains("debug key")), "{:?}", output.notes);
    let apk = out.join("Album-1.2.3.apk");
    let aab = out.join("Album-1.2.3.aab");
    assert_eq!(output.files, vec![apk.clone(), aab.clone()]);

    let badging = sh(Command::new(sdk.build_tools.join("aapt2")).args(["dump", "badging"]).arg(&apk), &sdk.java_bin);
    assert!(badging.contains("package: name='at.acme.album' versionCode='1002003' versionName='1.2.3'"), "{badging}");
    assert!(badging.contains("application-label:'Album'"), "{badging}");
    // photos.read only, network off: no INTERNET.
    for p in ["READ_MEDIA_IMAGES", "READ_MEDIA_VISUAL_USER_SELECTED", "READ_EXTERNAL_STORAGE"] {
        assert!(badging.contains(&format!("uses-permission: name='android.permission.{p}'")), "{p}: {badging}");
    }
    assert!(!badging.contains("android.permission.INTERNET"), "{badging}");
    let xml = sh(Command::new(sdk.build_tools.join("aapt2")).args(["dump", "xmltree", "--file", "AndroidManifest.xml"]).arg(&apk), &sdk.java_bin);
    assert!(xml.contains("\"at.acme.album.share\"") && xml.contains("\"at.acme.album\" (Raw: \"at.acme.album\")") && !xml.contains("template"), "{xml}");
    let verify = sh(Command::new(sdk.build_tools.join("apksigner")).args(["verify", "--verbose"]).arg(&apk), &sdk.java_bin);
    assert!(verify.contains("Verified using v2 scheme (APK Signature Scheme v2): true"), "{verify}");

    let mut z = zip::ZipArchive::new(std::fs::File::open(&apk).unwrap()).unwrap();
    let mut read = |name: &str| {
        let mut b = Vec::new();
        z.by_name(name).unwrap_or_else(|_| panic!("{name}")).read_to_end(&mut b).unwrap();
        b
    };
    assert_eq!(read("assets/app/package.app.json"), std::fs::read(&opts.package).unwrap());
    assert_eq!(read("assets/NOTICES.txt"), b"stub notices\n");
    assert!(String::from_utf8(read("assets/app/runner.json")).unwrap().contains("\"at.acme.album\""));
    assert_eq!(read("classes.dex"), b"dex\n035\0");
    assert_eq!(z.by_name("lib/x86_64/libeengine_ffi.so").unwrap().compression(), zip::CompressionMethod::Stored);
    assert_eq!(z.by_name("resources.arsc").unwrap().compression(), zip::CompressionMethod::Stored);
    assert!(z.by_name("res/mipmap-xxxhdpi-v4/ic_launcher.png").is_ok());

    let jar = sh(Command::new(sdk.java_bin.join("jarsigner")).args(["-verify"]).arg(&aab), &sdk.java_bin);
    assert!(jar.contains("jar verified"), "{jar}");
    let bundletool = std::env::var("BUNDLETOOL").unwrap();
    let manifest = sh(Command::new(sdk.java_bin.join("java")).arg("-jar").arg(&bundletool).arg("dump").arg("manifest").arg(format!("--bundle={}", aab.display())), &sdk.java_bin);
    assert!(manifest.contains("package=\"at.acme.album\"") && manifest.contains("android:versionCode=\"1002003\""), "{manifest}");
    let mut b = zip::ZipArchive::new(std::fs::File::open(&aab).unwrap()).unwrap();
    for name in ["base/dex/classes.dex", "base/lib/x86_64/libeengine_ffi.so", "base/assets/app/package.app.json", "base/assets/NOTICES.txt", "base/root/META-INF/services/x", "base/resources.pb"] {
        assert!(b.by_name(name).is_ok(), "{name} in the .aab");
    }
}
