//! macOS: NSOpenPanel/NSSavePanel, NSSharingServicePicker, NSPrintOperation
//! (PDFKit for PDFs, an NSImageView for PNG/JPEG) and NSUserNotificationCenter
//! through mac-notification-sys.
//!
//! THE THREADING RULE: AppKit only on the main thread. A call dispatches its
//! UI work to the main queue and waits on the CALLER's thread, so a call ON
//! the main thread would deadlock and answers `failed` instead. Panels and
//! sheets use their non-modal, completion-handler forms, so the main queue
//! keeps draining (gpui's foreground executor runs on it) while they are open.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, Once};
use std::time::Duration;

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSImage, NSImageScaling, NSImageView, NSModalResponse, NSModalResponseOK, NSOpenPanel, NSPrintInfo, NSPrintOperation,
    NSSavePanel, NSSharingService, NSSharingServicePicker, NSSharingServicePickerDelegate, NSView, NSWindow,
};
#[allow(deprecated)]
use objc2_foundation::{NSArray, NSBundle, NSData, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSUserNotificationCenter, NSURL};

use crate::{
    DesktopConfig, Dialogs, NamedBytes, NativeError, NativeResult, Notifier, OpenRequest, Printer, SaveRequest, ShareItem, ShareSheet, Shown,
    ShownHandle,
};

// PDFDocument (PDFKit) through the runtime: objc2-pdf-kit is not in the lock.
#[link(name = "PDFKit", kind = "framework")]
extern "C" {}

/// How long the main queue may take to START a dialog before the call gives
/// up (a host whose main thread runs no event loop never answers).
const START_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn dialogs(_: &DesktopConfig) -> Option<Arc<dyn Dialogs>> {
    Some(Arc::new(MacDialogs))
}

pub(super) fn share_sheet(_: &DesktopConfig) -> Option<Arc<dyn ShareSheet>> {
    Some(Arc::new(MacShare))
}

pub(super) fn printer(_: &DesktopConfig) -> Option<Arc<dyn Printer>> {
    Some(Arc::new(MacPrinter))
}

pub(super) fn notifier(config: &DesktopConfig) -> Option<Arc<dyn Notifier>> {
    Some(Arc::new(MacNotifier { app_id: config.app_id.clone(), shown: Mutex::new(HashMap::new()), next: AtomicU64::new(1), setup: Once::new() }))
}

fn failed(message: impl Into<String>) -> NativeError {
    NativeError::Failed(message.into())
}

type Reply<T> = mpsc::Sender<NativeResult<T>>;

/// Run `work` on the main queue and wait for the answer it sends.
fn on_main<T: Send + 'static>(work: impl FnOnce(MainThreadMarker, Reply<T>) + Send + 'static) -> NativeResult<T> {
    if MainThreadMarker::new().is_some() {
        return Err(failed("called on the main thread; call capabilities from a worker thread"));
    }
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (tx, rx) = mpsc::channel();
    DispatchQueue::main().exec_async(move || {
        let _ = started_tx.send(());
        let mtm = MainThreadMarker::new().expect("the main queue runs on the main thread");
        work(mtm, tx);
    });
    started_rx.recv_timeout(START_TIMEOUT).map_err(|_| failed("the main thread runs no event loop"))?;
    rx.recv().unwrap_or_else(|_| Err(failed("the dialog closed without an answer")))
}

#[allow(deprecated)]
fn activate(mtm: MainThreadMarker) {
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

fn key_window(mtm: MainThreadMarker) -> Option<Retained<NSWindow>> {
    let app = NSApplication::sharedApplication(mtm);
    app.keyWindow().or_else(|| app.mainWindow()).or_else(|| app.windows().iter().next())
}

#[allow(deprecated)]
fn set_types(panel: &NSSavePanel, extensions: &[String]) {
    if extensions.is_empty() {
        return;
    }
    let types: Vec<Retained<NSString>> = extensions.iter().map(|e| NSString::from_str(e)).collect();
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&types)));
}

struct MacDialogs;

impl Dialogs for MacDialogs {
    fn open(&self, request: &OpenRequest) -> NativeResult<Option<Vec<PathBuf>>> {
        let request = request.clone();
        on_main(move |mtm, tx| {
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setCanChooseFiles(true);
            panel.setCanChooseDirectories(false);
            panel.setAllowsMultipleSelection(request.multiple);
            panel.setMessage(Some(&NSString::from_str(&request.title)));
            set_types(&panel, &request.filter.extensions);
            activate(mtm);
            let answered = panel.clone();
            let tx = RefCell::new(Some(tx));
            let handler = RcBlock::new(move |response: NSModalResponse| {
                let Some(tx) = tx.borrow_mut().take() else { return };
                let paths = (response == NSModalResponseOK).then(|| answered.URLs().iter().filter_map(|url| url.to_file_path()).collect());
                let _ = tx.send(Ok(paths));
            });
            panel.beginWithCompletionHandler(&handler);
        })
    }

    fn save(&self, request: &SaveRequest) -> NativeResult<Option<PathBuf>> {
        let request = request.clone();
        on_main(move |mtm, tx| {
            let panel = NSSavePanel::savePanel(mtm);
            panel.setNameFieldStringValue(&NSString::from_str(&request.name));
            activate(mtm);
            let answered = panel.clone();
            let tx = RefCell::new(Some(tx));
            let handler = RcBlock::new(move |response: NSModalResponse| {
                let Some(tx) = tx.borrow_mut().take() else { return };
                let path = if response == NSModalResponseOK { answered.URL().and_then(|url| url.to_file_path()) } else { None };
                let _ = tx.send(Ok(path));
            });
            panel.beginWithCompletionHandler(&handler);
        })
    }
}

/// A shared or printed file on disk (the services read it after the call
/// answers, so it stays in the temp dir; the OS cleans it).
fn temp_file(kind: &str, file: &NamedBytes) -> NativeResult<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let dir = std::env::temp_dir()
        .join(format!("eengine-{kind}"))
        .join(format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).map_err(|e| failed(format!("cannot write a temp file: {e}")))?;
    let name: String = file.name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '-' } else { c }).collect();
    let path = dir.join(if name.is_empty() { "file".to_string() } else { name });
    std::fs::write(&path, &file.bytes).map_err(|e| failed(format!("cannot write a temp file: {e}")))?;
    Ok(path)
}

thread_local! {
    /// The open share picker and its delegate (the delegate property is weak).
    static SHARE: RefCell<Option<(Retained<NSSharingServicePicker>, Retained<ShareDelegate>)>> = const { RefCell::new(None) };
    /// The running print operation's delegate.
    static PRINT: RefCell<Option<Retained<PrintDelegate>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "EEngineDesktopShareDelegate"]
    #[ivars = RefCell<Option<Reply<bool>>>]
    struct ShareDelegate;

    unsafe impl NSObjectProtocol for ShareDelegate {}

    unsafe impl NSSharingServicePickerDelegate for ShareDelegate {
        #[unsafe(method(sharingServicePicker:didChooseSharingService:))]
        fn did_choose(&self, _picker: &NSSharingServicePicker, service: Option<&NSSharingService>) {
            // A chosen service = shared (AppKit does not tell whether it finished).
            if let Some(tx) = self.ivars().borrow_mut().take() {
                let _ = tx.send(Ok(service.is_some()));
            }
            DispatchQueue::main().exec_async(|| {
                SHARE.with(|share| share.borrow_mut().take());
            });
        }
    }
);

impl ShareDelegate {
    fn new(mtm: MainThreadMarker, tx: Reply<bool>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(Some(tx)));
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "EEngineDesktopPrintDelegate"]
    #[ivars = RefCell<Option<Reply<bool>>>]
    struct PrintDelegate;

    unsafe impl NSObjectProtocol for PrintDelegate {}

    impl PrintDelegate {
        #[unsafe(method(printOperationDidRun:success:contextInfo:))]
        fn did_run(&self, _operation: &NSPrintOperation, success: bool, _context: *mut c_void) {
            if let Some(tx) = self.ivars().borrow_mut().take() {
                let _ = tx.send(Ok(success));
            }
            DispatchQueue::main().exec_async(|| {
                PRINT.with(|print| print.borrow_mut().take());
            });
        }
    }
);

impl PrintDelegate {
    fn new(mtm: MainThreadMarker, tx: Reply<bool>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(Some(tx)));
        unsafe { msg_send![super(this), init] }
    }
}

fn object<T: objc2::Message>(value: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(value) }
}

struct MacShare;

impl ShareSheet for MacShare {
    fn share(&self, item: &ShareItem) -> NativeResult<bool> {
        let file = item.file.as_ref().map(|file| temp_file("share", file)).transpose()?;
        let text = item.text.clone();
        let url = item.url.clone();
        on_main(move |mtm, tx| {
            let mut items: Vec<Retained<AnyObject>> = Vec::new();
            if let Some(text) = &text {
                items.push(object(NSString::from_str(text)));
            }
            if let Some(url) = url.as_deref().and_then(|u| NSURL::URLWithString(&NSString::from_str(u))) {
                items.push(object(url));
            }
            if let Some(url) = file.as_deref().and_then(NSURL::from_file_path) {
                items.push(object(url));
            }
            let Some(view) = key_window(mtm).and_then(|w| w.contentView()) else {
                let _ = tx.send(Err(failed("no window to show the share sheet on")));
                return;
            };
            let items = NSArray::from_retained_slice(&items);
            let picker = unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
            let delegate = ShareDelegate::new(mtm, tx);
            picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            let bounds = view.bounds();
            let anchor = NSRect::new(NSPoint::new(bounds.size.width / 2.0, bounds.size.height / 2.0), NSSize::new(1.0, 1.0));
            activate(mtm);
            picker.showRelativeToRect_ofView_preferredEdge(anchor, &view, NSRectEdge::MinY);
            let previous = SHARE.with(|share| share.borrow_mut().replace((picker, delegate)));
            if let Some((previous, _)) = previous {
                previous.close();
            }
        })
    }
}

fn print_operation(mtm: MainThreadMarker, job: &NamedBytes) -> NativeResult<Retained<NSPrintOperation>> {
    let data = NSData::with_bytes(&job.bytes);
    let info = NSPrintInfo::sharedPrintInfo();
    if job.mime_type == "application/pdf" {
        let class = AnyClass::get(c"PDFDocument").ok_or_else(|| failed("PDFKit is not available"))?;
        let alloc: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        let document: Option<Retained<AnyObject>> = unsafe { msg_send![alloc, initWithData: &*data] };
        let document = document.ok_or_else(|| failed("not a readable PDF"))?;
        // kPDFPrintPageScaleToFit = 1.
        let operation: Option<Retained<NSPrintOperation>> =
            unsafe { msg_send![&*document, printOperationForPrintInfo: &*info, scalingMode: 1isize, autoRotate: true] };
        return operation.ok_or_else(|| failed("PDFKit made no print operation"));
    }
    let image = NSImage::initWithData(NSImage::alloc(), &data).ok_or_else(|| failed("not a readable image"))?;
    let size = image.size();
    if size.width <= 0.0 || size.height <= 0.0 {
        return Err(failed("an empty image"));
    }
    let paper = info.paperSize();
    let width = (paper.width - info.leftMargin() - info.rightMargin()).max(1.0);
    let height = (paper.height - info.topMargin() - info.bottomMargin()).max(1.0);
    let scale = (width / size.width).min(height / size.height).min(1.0);
    let view = NSImageView::imageViewWithImage(&image, mtm);
    view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(size.width * scale, size.height * scale)));
    let view: &NSView = &view;
    Ok(NSPrintOperation::printOperationWithView_printInfo(view, &info))
}

struct MacPrinter;

impl Printer for MacPrinter {
    fn print(&self, job: &NamedBytes) -> NativeResult<bool> {
        let job = job.clone();
        on_main(move |mtm, tx| {
            let operation = match print_operation(mtm, &job) {
                Ok(operation) => operation,
                Err(error) => {
                    let _ = tx.send(Err(error));
                    return;
                }
            };
            operation.setJobTitle(Some(&NSString::from_str(&job.name)));
            operation.setShowsPrintPanel(true);
            operation.setShowsProgressPanel(true);
            activate(mtm);
            match key_window(mtm) {
                Some(window) => {
                    // The sheet form: the main queue keeps draining while it is open.
                    let delegate = PrintDelegate::new(mtm, tx);
                    unsafe {
                        operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
                            &window,
                            Some(&delegate),
                            Some(sel!(printOperationDidRun:success:contextInfo:)),
                            std::ptr::null_mut(),
                        );
                    }
                    PRINT.with(|print| print.borrow_mut().replace(delegate));
                }
                None => {
                    // No window to attach the sheet to: the app-modal panel.
                    let printed = operation.runOperation();
                    let _ = tx.send(Ok(printed));
                }
            }
        })
    }
}

/// NSUserNotificationCenter via mac-notification-sys: works without an app
/// bundle (the bundle id is set per process), a click is waited for on a
/// thread per notification. Needs the host's main run loop running.
struct MacNotifier {
    app_id: Option<String>,
    shown: Mutex<HashMap<u64, (String, String)>>,
    next: AtomicU64,
    setup: Once,
}

impl MacNotifier {
    fn ensure_application(&self) {
        self.setup.call_once(|| {
            let bundle = self.app_id.clone().or_else(|| NSBundle::mainBundle().bundleIdentifier().map(|id| id.to_string()));
            if let Some(bundle) = bundle {
                if let Err(error) = mac_notification_sys::set_application(&bundle) {
                    log::warn!("eengine-desktop: notifications as {bundle}: {error}");
                }
            }
        });
    }
}

impl Notifier for MacNotifier {
    fn show(&self, notification: &Shown, on_event: Box<dyn FnOnce(bool) + Send>) -> NativeResult<ShownHandle> {
        self.ensure_application();
        let handle = self.next.fetch_add(1, Ordering::Relaxed);
        let title = notification.title.clone();
        let body = notification.body.clone().unwrap_or_default();
        self.shown.lock().unwrap().insert(handle, (title.clone(), body.clone()));
        std::thread::Builder::new()
            .name("eengine-notification".into())
            .spawn(move || {
                let response = mac_notification_sys::Notification::new().title(&title).message(&body).wait_for_click(true).send();
                match response {
                    Ok(mac_notification_sys::NotificationResponse::Click) => on_event(true),
                    Ok(_) => on_event(false),
                    Err(error) => {
                        log::warn!("eengine-desktop: a notification failed: {error}");
                        on_event(false);
                    }
                }
            })
            .map_err(|e| failed(format!("cannot post: {e}")))?;
        Ok(ShownHandle(handle))
    }

    #[allow(deprecated)]
    fn close(&self, handle: ShownHandle) {
        let Some((title, body)) = self.shown.lock().unwrap().remove(&handle.0) else { return };
        DispatchQueue::main().exec_async(move || {
            let center = NSUserNotificationCenter::defaultUserNotificationCenter();
            for delivered in center.deliveredNotifications().iter() {
                let same_title = delivered.title().is_some_and(|t| t.to_string() == title);
                let same_body = delivered.informativeText().map(|t| t.to_string()).unwrap_or_default() == body;
                if same_title && same_body {
                    center.removeDeliveredNotification(&delivered);
                }
            }
        });
    }
}
