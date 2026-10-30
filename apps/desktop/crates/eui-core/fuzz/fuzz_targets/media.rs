//! The media header readers and the URL / media policy on arbitrary bytes:
//! image dimensions and frames of any byte string, the pixel cap, and
//! `decide_url` / `media_request` on arbitrary urls and policies. Never a
//! panic.
#![no_main]

use arbitrary::Arbitrary;
use eui_core::host::{decide_url, image_dimensions, image_frames, is_private_host, media_image_within_limits, media_request, MediaOptions, UrlPolicy};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    bytes: Vec<u8>,
    url: String,
    policy: String,
}

fuzz_target!(|input: Input| {
    let _ = image_dimensions(&input.bytes);
    let _ = image_frames(&input.bytes);
    let _ = media_image_within_limits(&input.bytes);
    let _ = is_private_host(&input.url);
    let policy = serde_json::from_str::<UrlPolicy>(&input.policy).ok();
    let d = decide_url(policy.as_ref(), &input.url);
    if d.allowed {
        assert!(d.url.is_some(), "an allowed url is absolute");
    }
    let options = serde_json::from_str::<MediaOptions>(&input.policy).unwrap_or_default();
    let _ = media_request(&input.url, &options);
});
