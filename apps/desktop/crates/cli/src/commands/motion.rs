//! `exponential motion` (EXP-1603): the eUI Motion tooling on the engine
//! itself (`eui-motion`), offline.
//!
//! - `import <file.json|file.lottie>`: a Lottie or dotLottie → the stored
//!   format (validated Lottie JSON, images inlined), ready to attach;
//! - `compile <file>`: the authoring dialect → Lottie;
//! - `decompile <file>`: Lottie or dotLottie → the dialect;
//! - `check <file>`: validate either format (expressions, limits, dialect
//!   errors); exit 1 on errors;
//! - `render <file> [--frame N | --at ms] [--size WxH] [--out f.png]`, or
//!   `--gif [--fps N]` for an animated preview.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context as _};
use eui_motion::{dialect, dotlottie, raster, DisplayList, Limits, Motion};
use serde_json::Value;

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};

const USAGE: &str = "\
Usage: exponential motion <command> <file> [options]

Commands:
  import <file.json|file.lottie> [--out f.json]   Lottie/dotLottie → validated Lottie (images inlined)
  compile <file> [--out f.json]                   Dialect → Lottie
  decompile <file> [--out f.json]                 Lottie/dotLottie → dialect
  check <file>                                    Validate a dialect or Lottie file
  render <file> [--frame N | --at ms] [--size WxH] [--out f.png]
  render <file> --gif [--fps N] [--size WxH] [--out f.gif]
";

pub fn run(args: &[String]) -> CommandResult {
    let Some((cmd, rest)) = args.split_first() else {
        print!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    let mut rest = rest.to_vec();
    match cmd.as_str() {
        "import" => import(&mut rest),
        "compile" => convert(&mut rest, Mode::Compile),
        "decompile" => convert(&mut rest, Mode::Decompile),
        "check" => check(&mut rest),
        "render" => render(&mut rest),
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("Unknown motion command `{other}`.\n");
            print!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

enum Mode {
    Compile,
    Decompile,
}

fn input(args: &mut Vec<String>) -> anyhow::Result<PathBuf> {
    let file = args.iter().position(|a| !a.starts_with('-')).map(|i| args.remove(i));
    match file {
        Some(f) => Ok(PathBuf::from(f)),
        None => bail!("missing <file>\n\n{USAGE}"),
    }
}

/// The document a file holds: Lottie, the dialect, or a dotLottie's first animation.
enum Doc {
    Lottie(Value),
    Dialect(Value),
}

fn is_dialect(v: &Value) -> bool {
    // Lottie has `w`/`h`/`fr`; the dialect has `size` (and no `w`).
    v.get("size").is_some() && v.get("w").is_none()
}

fn load(path: &Path) -> anyhow::Result<(Doc, Option<dotlottie::DotLottie>)> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if bytes.starts_with(b"PK") {
        let dl = dotlottie::read(&bytes).map_err(|e| anyhow::anyhow!(issues_text(&e)))?;
        let first = dl.animations.first().map(|(_, a)| a.clone()).unwrap_or(Value::Null);
        return Ok((Doc::Lottie(first), Some(dl)));
    }
    let v: Value = serde_json::from_slice(&bytes).with_context(|| format!("{} is not JSON", path.display()))?;
    Ok((if is_dialect(&v) { Doc::Dialect(v) } else { Doc::Lottie(v) }, None))
}

fn issues_text(issues: &[eui_motion::Issue]) -> String {
    issues
        .iter()
        .map(|i| if i.path.is_empty() { format!("{}: {}", i.code, i.message) } else { format!("{} at {}: {}", i.code, i.path, i.message) })
        .collect::<Vec<_>>()
        .join("\n")
}

fn to_lottie(doc: Doc) -> anyhow::Result<Value> {
    match doc {
        Doc::Lottie(v) => Ok(v),
        Doc::Dialect(d) => dialect::compile(&d).map_err(|e| anyhow::anyhow!(issues_text(&e))),
    }
}

/// Inline images given as relative files (next to the source) as data URLs.
fn inline_local_images(lottie: &mut Value, base: &Path) {
    let Some(Value::Array(assets)) = lottie.get_mut("assets") else { return };
    for a in assets {
        let (Some(p), u) = (a.get("p").and_then(Value::as_str), a.get("u").and_then(Value::as_str).unwrap_or("")) else { continue };
        if p.starts_with("data:") || p.contains("://") || a.get("layers").is_some() {
            continue;
        }
        let file = base.join(u).join(p);
        if let Ok(bytes) = std::fs::read(&file) {
            let mime = if p.ends_with(".png") { "image/png" } else if p.ends_with(".webp") { "image/webp" } else { "image/jpeg" };
            use base64::Engine as _;
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            a["u"] = Value::String(String::new());
            a["p"] = Value::String(format!("data:{mime};base64,{data}"));
            a["e"] = Value::from(1);
        }
    }
}

fn write_out(out: Option<String>, text: &[u8]) -> anyhow::Result<()> {
    match out {
        Some(path) if !path.is_empty() => std::fs::write(&path, text).with_context(|| format!("writing {path}")),
        _ => {
            use std::io::Write as _;
            std::io::stdout().write_all(text)?;
            println!();
            Ok(())
        }
    }
}

fn import(args: &mut Vec<String>) -> CommandResult {
    let out = take_value(args, "--out");
    let path = input(args)?;
    reject_unknown_flags(args)?;
    let (doc, _) = load(&path)?;
    let mut lottie = to_lottie(doc)?;
    inline_local_images(&mut lottie, path.parent().unwrap_or(Path::new(".")));
    let bytes = serde_json::to_vec(&lottie)?;
    let report = eui_motion::validate::check(&bytes, &Limits::default());
    for w in &report.warnings {
        eprintln!("warning: {}", issues_text(std::slice::from_ref(w)));
    }
    if !report.errors.is_empty() {
        eprintln!("{}", issues_text(&report.errors));
        return Ok(ExitCode::FAILURE);
    }
    write_out(out, &bytes)?;
    Ok(ExitCode::SUCCESS)
}

fn convert(args: &mut Vec<String>, mode: Mode) -> CommandResult {
    let out = take_value(args, "--out");
    let path = input(args)?;
    reject_unknown_flags(args)?;
    let (doc, _) = load(&path)?;
    let value = match (mode, doc) {
        (Mode::Compile, doc) => to_lottie(doc)?,
        (Mode::Decompile, Doc::Lottie(l)) => dialect::decompile(&l),
        (Mode::Decompile, Doc::Dialect(d)) => d,
    };
    write_out(out, serde_json::to_string_pretty(&value)?.as_bytes())?;
    Ok(ExitCode::SUCCESS)
}

fn check(args: &mut Vec<String>) -> CommandResult {
    let path = input(args)?;
    reject_unknown_flags(args)?;
    let (doc, _) = load(&path)?;
    let lottie = match to_lottie(doc) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::FAILURE);
        }
    };
    let bytes = serde_json::to_vec(&lottie)?;
    let report = eui_motion::validate::check(&bytes, &Limits::default());
    for w in &report.warnings {
        println!("warning: {}", issues_text(std::slice::from_ref(w)));
    }
    if !report.errors.is_empty() {
        eprintln!("{}", issues_text(&report.errors));
        return Ok(ExitCode::FAILURE);
    }
    let motion = Motion::load(&bytes).map_err(|e| anyhow::anyhow!(issues_text(&e)))?;
    let (w, h) = motion.size();
    println!("ok: {w}×{h}, {:.2} s at {} fps, {} markers", motion.duration(), motion.frame_rate(), motion.markers().len());
    Ok(ExitCode::SUCCESS)
}

fn parse_size(s: &str) -> anyhow::Result<(u32, u32)> {
    let (w, h) = s.split_once('x').context("--size is WxH")?;
    Ok((w.parse()?, h.parse()?))
}

fn render(args: &mut Vec<String>) -> CommandResult {
    let gif = take_flag(args, "--gif");
    let out = take_value(args, "--out");
    let frame = take_value(args, "--frame");
    let at = take_value(args, "--at");
    let size = take_value(args, "--size");
    let fps = take_value(args, "--fps");
    let path = input(args)?;
    reject_unknown_flags(args)?;
    let (doc, _) = load(&path)?;
    let mut lottie = to_lottie(doc)?;
    inline_local_images(&mut lottie, path.parent().unwrap_or(Path::new(".")));
    let bytes = serde_json::to_vec(&lottie)?;
    let motion = Motion::load(&bytes).map_err(|e| anyhow::anyhow!(issues_text(&e)))?;
    let images = raster::decode_images(&motion, path.parent());
    let (cw, ch) = motion.size();
    let (w, h) = match size {
        Some(s) => parse_size(&s)?,
        None => {
            // Contain within 600×600 by default.
            let s = (600.0 / cw.max(ch).max(1.0)).min(1.0);
            ((cw * s).round().max(1.0) as u32, (ch * s).round().max(1.0) as u32)
        }
    };
    let (ip, op) = motion.frames();
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "motion".into());
    let mut list = DisplayList::default();
    if gif {
        let fps: f64 = fps.map(|f| f.parse()).transpose()?.unwrap_or(motion.frame_rate().min(30.0)).clamp(1.0, 50.0);
        let out = out.unwrap_or_else(|| format!("{stem}.gif"));
        let file = std::fs::File::create(&out).with_context(|| format!("writing {out}"))?;
        let mut enc = gif::Encoder::new(std::io::BufWriter::new(file), w as u16, h as u16, &[])?;
        enc.set_repeat(gif::Repeat::Infinite)?;
        let n = ((op - ip) / motion.frame_rate() * fps).ceil().max(1.0) as usize;
        let delay = (100.0 / fps).round() as u16;
        for k in 0..n {
            let f = ip + k as f64 * motion.frame_rate() / fps;
            motion.sample(f, &mut list);
            let pix = raster::paint(&list, w, h, &images);
            // Un-premultiply over white (GIF has 1-bit transparency).
            let mut rgba: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
            for p in pix.pixels() {
                let a = p.alpha() as u32;
                let over = |c: u8| (c as u32 + (255 - a)).min(255) as u8;
                rgba.extend_from_slice(&[over(p.red()), over(p.green()), over(p.blue()), 255]);
            }
            let mut frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut rgba, 10);
            frame.delay = delay;
            enc.write_frame(&frame)?;
        }
        eprintln!("{out}: {n} frames, {w}×{h}");
        return Ok(ExitCode::SUCCESS);
    }
    let f = match (frame, at) {
        (Some(fr), _) => fr.parse::<f64>()?,
        (None, Some(ms)) => ip + ms.parse::<f64>()? / 1000.0 * motion.frame_rate(),
        (None, None) => ip,
    };
    motion.sample(f, &mut list);
    let pix = raster::paint(&list, w, h, &images);
    let out = out.unwrap_or_else(|| format!("{stem}.png"));
    pix.save_png(&out).with_context(|| format!("writing {out}"))?;
    eprintln!("{out}: frame {f}, {w}×{h}");
    Ok(ExitCode::SUCCESS)
}
