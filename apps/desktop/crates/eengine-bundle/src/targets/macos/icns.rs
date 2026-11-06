//! The `.icns` of the macOS icon set (`icons.macos`): PNG entries, one per
//! OSType the size fills (a 32 px PNG is both `icp5` 32 and `ic11` 16@2x).

use anyhow::Result;

use crate::icons::Icon;

/// The OSTypes a PNG of `size` px fills.
pub fn types(size: u32) -> &'static [&'static [u8; 4]] {
    match size {
        16 => &[b"icp4"],
        32 => &[b"icp5", b"ic11"],
        64 => &[b"icp6", b"ic12"],
        128 => &[b"ic07"],
        256 => &[b"ic08", b"ic13"],
        512 => &[b"ic09", b"ic14"],
        1024 => &[b"ic10"],
        _ => &[],
    }
}

/// `(size, png)` pairs → the `.icns` container (big-endian lengths, the header counts itself).
pub fn encode(pngs: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (size, png) in pngs {
        for ty in types(*size) {
            body.extend_from_slice(*ty);
            body.extend_from_slice(&(png.len() as u32 + 8).to_be_bytes());
            body.extend_from_slice(png);
        }
    }
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

/// The icon rendered at every `icons.macos` size, as one `.icns`.
pub fn render(icon: &Icon) -> Result<Vec<u8>> {
    let pngs = crate::icons::sizes("macos").into_iter().map(|(_, px)| icon.png(px).map(|png| (px, png))).collect::<Result<Vec<_>>>()?;
    Ok(encode(&pngs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_contract_size_fills_a_type() {
        for (_, px) in crate::icons::sizes("macos") {
            assert!(!types(px).is_empty(), "{px} px has no icns type");
        }
    }

    #[test]
    fn the_container_counts_its_entries() {
        let icns = encode(&[(16, vec![1, 2, 3]), (32, vec![4])]);
        assert_eq!(&icns[0..4], b"icns");
        assert_eq!(u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize, icns.len());
        // icp4 (3 bytes), icp5 + ic11 (1 byte each): 8 + 11 + 9 + 9.
        assert_eq!(icns.len(), 8 + 11 + 9 + 9);
        assert_eq!(&icns[8..12], b"icp4");
        assert_eq!(u32::from_be_bytes(icns[12..16].try_into().unwrap()), 11);
        assert_eq!(&icns[19..23], b"icp5");
        assert_eq!(&icns[28..32], b"ic11");
    }
}
