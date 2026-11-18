use std::{
    fs::File,
    io::{ErrorKind, Write},
    os::fd::{AsRawFd, BorrowedFd, OwnedFd},
};

use calloop::{LoopHandle, PostAction};
use filedescriptor::Pipe;
use strum::IntoEnumIterator;
use wayland_client::{Connection, protocol::wl_data_offer::WlDataOffer};
use wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1;

use crate::linux::clipboard_formats::{has_text, image_offers};
use crate::linux::platform::{PIPE_READ_TIMEOUT, read_fd_with_timeout};
use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat, hash};

/// Text mime types that we'll offer to other programs.
pub(crate) const TEXT_MIME_TYPES: [&str; 3] =
    ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
pub(crate) const FILE_LIST_MIME_TYPE: &str = "text/uri-list";

/// Text mime types that we'll accept from other programs.
pub(crate) const ALLOWED_TEXT_MIME_TYPES: [&str; 2] = ["text/plain;charset=utf-8", "UTF8_STRING"];

/// Exponential patch (#EXP-1946): one MIME type a written item offers, with its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Offer {
    mime_type: &'static str,
    bytes: Vec<u8>,
}

/// Exponential patch (#EXP-1946): an item's offers: its image (its own format, then a PNG copy),
/// then its text under every text MIME type. An image-only item offers no text; an item with
/// neither offers empty text, as upstream did.
fn item_offers(item: &ClipboardItem) -> Vec<Offer> {
    let mut offers: Vec<Offer> = image_offers(item)
        .into_iter()
        .map(|offer| Offer {
            mime_type: offer.format.mime_type(),
            bytes: offer.bytes,
        })
        .collect();
    if offers.is_empty() || has_text(item) {
        let text = item.text().unwrap_or_default().into_bytes();
        offers.extend(TEXT_MIME_TYPES.iter().map(|&mime_type| Offer {
            mime_type,
            bytes: text.clone(),
        }));
    }
    offers
}

/// Exponential patch (#EXP-1946): the bytes for a requested MIME type. Any other type (our own
/// `pid/` marker) gets the text, as upstream sent for every type, when the item has text.
fn offer_bytes<'a>(offers: &'a [Offer], mime_type: &str) -> Option<&'a [u8]> {
    offers
        .iter()
        .find(|offer| offer.mime_type == mime_type)
        .or_else(|| {
            offers
                .iter()
                .find(|offer| offer.mime_type == TEXT_MIME_TYPES[0])
        })
        .map(|offer| offer.bytes.as_slice())
}

pub(crate) struct Clipboard {
    connection: Connection,
    loop_handle: LoopHandle<'static, ()>,
    self_mime: String,

    // Internal clipboard
    contents: Option<ClipboardItem>,
    primary_contents: Option<ClipboardItem>,
    // Exponential patch (#EXP-1946): what `contents` and `primary_contents` offer, by MIME type.
    offers: Vec<Offer>,
    primary_offers: Vec<Offer>,

    // External clipboard
    cached_read: Option<ClipboardItem>,
    current_offer: Option<DataOffer<WlDataOffer>>,
    cached_primary_read: Option<ClipboardItem>,
    current_primary_offer: Option<DataOffer<ZwpPrimarySelectionOfferV1>>,
}

pub(crate) trait ReceiveData {
    fn receive_data(&self, mime_type: String, fd: BorrowedFd<'_>);
}

impl ReceiveData for WlDataOffer {
    fn receive_data(&self, mime_type: String, fd: BorrowedFd<'_>) {
        self.receive(mime_type, fd);
    }
}

impl ReceiveData for ZwpPrimarySelectionOfferV1 {
    fn receive_data(&self, mime_type: String, fd: BorrowedFd<'_>) {
        self.receive(mime_type, fd);
    }
}

#[derive(Clone, Debug)]
/// Wrapper for `WlDataOffer` and `ZwpPrimarySelectionOfferV1`, used to help track mime types.
pub(crate) struct DataOffer<T: ReceiveData> {
    pub inner: T,
    mime_types: Vec<String>,
}

impl<T: ReceiveData> DataOffer<T> {
    pub fn new(offer: T) -> Self {
        Self {
            inner: offer,
            mime_types: Vec::new(),
        }
    }

    pub fn add_mime_type(&mut self, mime_type: String) {
        self.mime_types.push(mime_type)
    }

    fn has_mime_type(&self, mime_type: &str) -> bool {
        self.mime_types.iter().any(|t| t == mime_type)
    }

    fn read_bytes(&self, connection: &Connection, mime_type: &str) -> Option<Vec<u8>> {
        let pipe = Pipe::new().unwrap();
        self.inner.receive_data(mime_type.to_string(), unsafe {
            BorrowedFd::borrow_raw(pipe.write.as_raw_fd())
        });
        let fd = pipe.read;
        drop(pipe.write);

        connection.flush().unwrap();

        match read_fd_with_timeout(fd, PIPE_READ_TIMEOUT) {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                log::error!("error reading clipboard pipe: {err:?}");
                None
            }
        }
    }

    fn read_text(&self, connection: &Connection) -> Option<ClipboardItem> {
        let mime_type = self.mime_types.iter().find(|&mime_type| {
            ALLOWED_TEXT_MIME_TYPES
                .iter()
                .any(|&allowed| allowed == mime_type)
        })?;
        let bytes = self.read_bytes(connection, mime_type)?;
        let text_content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(e) => {
                log::error!("Failed to convert clipboard content to UTF-8: {}", e);
                return None;
            }
        };

        // Normalize the text to unix line endings, otherwise
        // copying from eg: firefox inserts a lot of blank
        // lines, and that is super annoying.
        let result = text_content.replace("\r\n", "\n");
        Some(ClipboardItem::new_string(result))
    }

    fn read_image(&self, connection: &Connection) -> Option<ClipboardItem> {
        for format in ImageFormat::iter() {
            let mime_type = format.mime_type();
            if !self.has_mime_type(mime_type) {
                continue;
            }

            if let Some(bytes) = self.read_bytes(connection, mime_type) {
                let id = hash(&bytes);
                return Some(ClipboardItem {
                    entries: vec![ClipboardEntry::Image(Image { format, bytes, id })],
                });
            }
        }
        None
    }
}

impl Clipboard {
    pub fn new(connection: Connection, loop_handle: LoopHandle<'static, ()>) -> Self {
        Self {
            connection,
            loop_handle,
            self_mime: format!("pid/{}", std::process::id()),

            contents: None,
            primary_contents: None,
            offers: Vec::new(),
            primary_offers: Vec::new(),

            cached_read: None,
            current_offer: None,
            cached_primary_read: None,
            current_primary_offer: None,
        }
    }

    pub fn set(&mut self, item: ClipboardItem) {
        self.offers = item_offers(&item);
        self.contents = Some(item);
    }

    pub fn set_primary(&mut self, item: ClipboardItem) {
        self.primary_offers = item_offers(&item);
        self.primary_contents = Some(item);
    }

    /// Exponential patch (#EXP-1946): the MIME types the clipboard's item offers.
    pub fn mime_types(&self) -> Vec<&'static str> {
        self.offers.iter().map(|offer| offer.mime_type).collect()
    }

    /// Exponential patch (#EXP-1946): the MIME types the primary selection's item offers.
    pub fn primary_mime_types(&self) -> Vec<&'static str> {
        self.primary_offers
            .iter()
            .map(|offer| offer.mime_type)
            .collect()
    }

    pub fn set_offer(&mut self, data_offer: Option<DataOffer<WlDataOffer>>) {
        self.cached_read = None;
        self.current_offer = data_offer;
    }

    pub fn set_primary_offer(&mut self, data_offer: Option<DataOffer<ZwpPrimarySelectionOfferV1>>) {
        self.cached_primary_read = None;
        self.current_primary_offer = data_offer;
    }

    pub fn self_mime(&self) -> String {
        self.self_mime.clone()
    }

    // Exponential patch (#EXP-1946): sends the bytes of the requested MIME type; upstream sent
    // the text whatever the type.
    pub fn send(&self, mime_type: String, fd: OwnedFd) {
        if let Some(bytes) = offer_bytes(&self.offers, &mime_type) {
            self.send_bytes(fd, bytes.to_owned());
        }
    }

    pub fn send_primary(&self, mime_type: String, fd: OwnedFd) {
        if let Some(bytes) = offer_bytes(&self.primary_offers, &mime_type) {
            self.send_bytes(fd, bytes.to_owned());
        }
    }

    pub fn read(&mut self) -> Option<ClipboardItem> {
        let offer = self.current_offer.as_ref()?;
        if let Some(cached) = self.cached_read.clone() {
            return Some(cached);
        }

        if offer.has_mime_type(&self.self_mime) {
            return self.contents.clone();
        }

        let item = offer
            .read_text(&self.connection)
            .or_else(|| offer.read_image(&self.connection))?;

        self.cached_read = Some(item.clone());
        Some(item)
    }

    pub fn read_primary(&mut self) -> Option<ClipboardItem> {
        let offer = self.current_primary_offer.as_ref()?;
        if let Some(cached) = self.cached_primary_read.clone() {
            return Some(cached);
        }

        if offer.has_mime_type(&self.self_mime) {
            return self.primary_contents.clone();
        }

        let item = offer
            .read_text(&self.connection)
            .or_else(|| offer.read_image(&self.connection))?;

        self.cached_primary_read = Some(item.clone());
        Some(item)
    }

    pub fn send_bytes(&self, fd: OwnedFd, bytes: Vec<u8>) {
        let mut written = 0;
        self.loop_handle
            .insert_source(
                calloop::generic::Generic::new(
                    File::from(fd),
                    calloop::Interest::WRITE,
                    calloop::Mode::Level,
                ),
                move |_, file, _| {
                    let file = unsafe { file.get_mut() };
                    loop {
                        match file.write(&bytes[written..]) {
                            Ok(n) if written + n == bytes.len() => {
                                written += n;
                                break Ok(PostAction::Remove);
                            }
                            Ok(n) => written += n,
                            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                                break Ok(PostAction::Continue);
                            }
                            Err(_) => break Ok(PostAction::Remove),
                        }
                    }
                },
            )
            .unwrap();
    }
}

// Exponential patch (#EXP-1946)
#[cfg(test)]
mod tests {
    use super::{TEXT_MIME_TYPES, item_offers, offer_bytes};
    use gpui::{ClipboardEntry, ClipboardItem, ClipboardString, Image, ImageFormat};

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn mime_types(item: &ClipboardItem) -> Vec<&'static str> {
        item_offers(item).iter().map(|offer| offer.mime_type).collect()
    }

    #[test]
    fn clipboard_text_item_offers_the_text_types_as_before() {
        let item = ClipboardItem::new_string("hi".into());
        assert_eq!(mime_types(&item), TEXT_MIME_TYPES.to_vec());
        let offers = item_offers(&item);
        for mime_type in TEXT_MIME_TYPES {
            assert_eq!(offer_bytes(&offers, mime_type), Some(&b"hi"[..]));
        }
        assert_eq!(offer_bytes(&offers, "pid/42"), Some(&b"hi"[..]));
    }

    #[test]
    fn clipboard_image_item_offers_image_png_only() {
        let png = png();
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, png.clone()));
        assert_eq!(mime_types(&item), vec!["image/png"]);
        let offers = item_offers(&item);
        assert_eq!(offer_bytes(&offers, "image/png"), Some(png.as_slice()));
        assert_eq!(offer_bytes(&offers, "text/plain"), None);
        assert_eq!(offer_bytes(&offers, "pid/42"), None);
    }

    #[test]
    fn clipboard_jpeg_item_offers_jpeg_and_png() {
        let mut jpeg = Vec::new();
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Jpeg, jpeg.clone()));
        assert_eq!(mime_types(&item), vec!["image/jpeg", "image/png"]);
        assert_eq!(offer_bytes(&item_offers(&item), "image/jpeg"), Some(jpeg.as_slice()));
    }

    #[test]
    fn clipboard_mixed_item_offers_image_and_text() {
        let png = png();
        let item = ClipboardItem {
            entries: vec![
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, png.clone())),
                ClipboardEntry::String(ClipboardString::new("alt".into())),
            ],
        };
        let mut expected = vec!["image/png"];
        expected.extend(TEXT_MIME_TYPES);
        assert_eq!(mime_types(&item), expected);
        let offers = item_offers(&item);
        assert_eq!(offer_bytes(&offers, "image/png"), Some(png.as_slice()));
        assert_eq!(offer_bytes(&offers, "UTF8_STRING"), Some(&b"alt"[..]));
    }
}
