//! An `.ico` of PNG images (Windows Vista and later read PNG entries).

/// The `.ico` of `(size, png)` images, in the order given.
pub fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, png) in images {
        // 256 px is written as 0.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_directory_and_images() {
        let ico = ico(&[(16, vec![1, 2, 3]), (256, vec![4, 5])]);
        assert_eq!(&ico[0..6], &[0, 0, 1, 0, 2, 0]);
        // 16 px: w, h, colours, reserved, planes 1, 32 bpp, 3 bytes at 6 + 32.
        assert_eq!(&ico[6..22], &[16, 16, 0, 0, 1, 0, 32, 0, 3, 0, 0, 0, 38, 0, 0, 0]);
        assert_eq!(&ico[22..38], &[0, 0, 0, 0, 1, 0, 32, 0, 2, 0, 0, 0, 41, 0, 0, 0]);
        assert_eq!(&ico[38..], &[1, 2, 3, 4, 5]);
    }
}
