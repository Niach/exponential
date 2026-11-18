//! Exponential patch (#EXP-1946): what a clipboard item offers other programs, shared by X11 and
//! Wayland. Upstream offered only text, so a copied image reached nobody.

use gpui::{ClipboardEntry, ClipboardItem, ImageFormat};

/// One representation of a clipboard item's image: its MIME type and bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageOffer {
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
}

/// Whether the item carries text: only then are text MIME types offered.
///
/// An image-only item offers no (empty) text, which a paste target would otherwise prefer.
pub(crate) fn has_text(item: &ClipboardItem) -> bool {
    item.entries()
        .iter()
        .any(|entry| matches!(entry, ClipboardEntry::String(_)))
}

/// The image representations the item offers: its first image in its own format, then a PNG copy
/// when that format is not PNG (the one image type every Linux program pastes). An SVG, which
/// the `image` crate cannot rasterize, or an image that fails to decode offers its own format only.
pub(crate) fn image_offers(item: &ClipboardItem) -> Vec<ImageOffer> {
    let Some(image) = item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image),
        _ => None,
    }) else {
        return Vec::new();
    };
    if image.bytes.is_empty() {
        return Vec::new();
    }
    let mut offers = vec![ImageOffer {
        format: image.format,
        bytes: image.bytes.clone(),
    }];
    if image.format != ImageFormat::Png
        && let Some(png) = convert_to_png(&image.bytes, image.format)
    {
        offers.push(ImageOffer {
            format: ImageFormat::Png,
            bytes: png,
        });
    }
    offers
}

fn convert_to_png(bytes: &[u8], format: ImageFormat) -> Option<Vec<u8>> {
    let format = match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Webp => image::ImageFormat::WebP,
        ImageFormat::Gif => image::ImageFormat::Gif,
        ImageFormat::Bmp => image::ImageFormat::Bmp,
        ImageFormat::Tiff => image::ImageFormat::Tiff,
        ImageFormat::Ico => image::ImageFormat::Ico,
        ImageFormat::Pnm => image::ImageFormat::Pnm,
        ImageFormat::Svg => return None,
    };
    let decoded = image::load_from_memory_with_format(bytes, format)
        .map_err(|error| log::warn!("Failed to decode a copied image for its PNG copy: {error}"))
        .ok()?;
    let mut png = Vec::new();
    decoded
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| log::warn!("Failed to encode a copied image as PNG: {error}"))
        .ok()?;
    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ClipboardString, Image};

    fn encoded(format: image::ImageFormat) -> Vec<u8> {
        let pixels = image::RgbImage::from_fn(3, 2, |x, y| image::Rgb([x as u8 * 80, y as u8 * 120, 7]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(pixels)
            .write_to(&mut std::io::Cursor::new(&mut bytes), format)
            .unwrap();
        bytes
    }

    fn image_item(format: ImageFormat, bytes: Vec<u8>) -> ClipboardItem {
        ClipboardItem::new_image(&Image::from_bytes(format, bytes))
    }

    #[test]
    fn text_only_item_offers_text_and_no_image() {
        let item = ClipboardItem::new_string("hello".into());
        assert!(has_text(&item));
        assert!(image_offers(&item).is_empty());
    }

    #[test]
    fn png_image_is_offered_once_as_is() {
        let png = encoded(image::ImageFormat::Png);
        let item = image_item(ImageFormat::Png, png.clone());
        assert!(!has_text(&item));
        assert_eq!(
            image_offers(&item),
            vec![ImageOffer {
                format: ImageFormat::Png,
                bytes: png
            }]
        );
    }

    #[test]
    fn jpeg_image_is_offered_as_jpeg_then_as_a_decodable_png() {
        let jpeg = encoded(image::ImageFormat::Jpeg);
        let offers = image_offers(&image_item(ImageFormat::Jpeg, jpeg.clone()));
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].format, ImageFormat::Jpeg);
        assert_eq!(offers[0].bytes, jpeg);
        assert_eq!(offers[1].format, ImageFormat::Png);
        let png = image::load_from_memory_with_format(&offers[1].bytes, image::ImageFormat::Png)
            .unwrap();
        assert_eq!((png.width(), png.height()), (3, 2));
    }

    #[test]
    fn svg_and_undecodable_images_offer_their_own_format_only() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec();
        let offers = image_offers(&image_item(ImageFormat::Svg, svg.clone()));
        assert_eq!(
            offers,
            vec![ImageOffer {
                format: ImageFormat::Svg,
                bytes: svg
            }]
        );
        let offers = image_offers(&image_item(ImageFormat::Webp, b"not a webp".to_vec()));
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].format, ImageFormat::Webp);
    }

    #[test]
    fn mixed_item_offers_text_and_its_first_image() {
        let png = encoded(image::ImageFormat::Png);
        let item = ClipboardItem {
            entries: vec![
                ClipboardEntry::String(ClipboardString::new("caption".into())),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, png.clone())),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Gif, b"second".to_vec())),
            ],
        };
        assert!(has_text(&item));
        let offers = image_offers(&item);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].bytes, png);
    }

    #[test]
    fn empty_image_offers_nothing() {
        assert!(image_offers(&image_item(ImageFormat::Png, Vec::new())).is_empty());
    }
}
