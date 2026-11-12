//! System notifications as Windows toast notifications.
//!
//! Exponential patch (#ERR-2): every WinRT toast call runs on a worker
//! thread of its own (a multithreaded COM apartment), never on the UI thread.
//! `ToastNotificationManager` and `ToastNotifier::Show` are cross-process
//! calls; made from the UI thread's single-threaded apartment, COM's modal
//! loop pumps that thread's window messages while it waits, so gpui's frame
//! callbacks ran while the caller (an `AsyncApp::update`) still held the App
//! borrow: "RefCell already borrowed" on every toast. The UI thread now only
//! queues a command.

use std::cell::RefCell;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash as _, Hasher as _};
use std::rc::Rc;
use std::sync::mpsc as std_mpsc;

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{
    ForegroundExecutor, SharedString, SystemNotification, SystemNotificationResponse, Task,
};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    ToastActivatedEventArgs, ToastNotification, ToastNotificationManager, ToastNotifier,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{IInspectable, Interface as _, h};

type ResponseCallback = Rc<RefCell<Option<Box<dyn FnMut(SystemNotificationResponse)>>>>;

/// What the UI thread asks of the toast worker.
pub(crate) enum ToastCommand {
    Show {
        has_package_identity: bool,
        app_identity: Option<(String, String)>,
        notification: SystemNotification,
    },
    Dismiss(SharedString),
}

/// A named thread draining commands in order, started on the first one.
/// Dropping it closes the channel and the thread ends.
pub(crate) struct CommandWorker<C: Send + 'static> {
    name: &'static str,
    sender: Option<std_mpsc::Sender<C>>,
    start: Option<Box<dyn FnOnce(std_mpsc::Receiver<C>) + Send>>,
}

impl<C: Send + 'static> CommandWorker<C> {
    pub(crate) fn new(name: &'static str, run: impl FnOnce(std_mpsc::Receiver<C>) + Send + 'static) -> Self {
        Self {
            name,
            sender: None,
            start: Some(Box::new(run)),
        }
    }

    /// Queue `command`; never blocks and never runs it on the caller's thread.
    pub(crate) fn send(&mut self, command: C) {
        if self.sender.is_none() {
            let Some(run) = self.start.take() else {
                return;
            };
            let (sender, receiver) = std_mpsc::channel();
            match std::thread::Builder::new()
                .name(self.name.into())
                .spawn(move || run(receiver))
            {
                Ok(_) => self.sender = Some(sender),
                Err(error) => {
                    log::warn!("failed to start the {} thread: {error}", self.name);
                    return;
                }
            }
        }
        if let Some(sender) = &self.sender {
            if sender.send(command).is_err() {
                log::warn!("the {} thread has stopped", self.name);
                self.sender = None;
            }
        }
    }
}

pub(crate) struct SystemNotificationState {
    worker: CommandWorker<ToastCommand>,
    response_receiver: Option<mpsc::UnboundedReceiver<SystemNotificationResponse>>,
    callback: ResponseCallback,
    _response_task: Option<Task<()>>,
}

impl SystemNotificationState {
    pub(crate) fn new() -> Self {
        let (response_sender, response_receiver) = mpsc::unbounded();
        Self {
            worker: CommandWorker::new("gpui-toasts", move |commands| {
                run_toast_worker(commands, response_sender)
            }),
            response_receiver: Some(response_receiver),
            callback: Rc::new(RefCell::new(None)),
            _response_task: None,
        }
    }

    pub(crate) fn show(
        &mut self,
        has_package_identity: bool,
        app_identity: Option<(&str, &str)>,
        notification: SystemNotification,
    ) -> windows::core::Result<()> {
        self.worker.send(ToastCommand::Show {
            has_package_identity,
            app_identity: app_identity
                .map(|(identifier, name)| (identifier.to_string(), name.to_string())),
            notification,
        });
        Ok(())
    }

    pub(crate) fn dismiss(&mut self, tag: &str) {
        self.worker.send(ToastCommand::Dismiss(tag.to_string().into()));
    }

    pub(crate) fn on_response(
        &mut self,
        executor: &ForegroundExecutor,
        callback: Box<dyn FnMut(SystemNotificationResponse)>,
    ) {
        *self.callback.borrow_mut() = Some(callback);

        if let Some(mut receiver) = self.response_receiver.take() {
            let callback = self.callback.clone();
            self._response_task = Some(executor.spawn(async move {
                while let Some(response) = receiver.next().await {
                    // Take the callback out for the call: it may re-enter the
                    // platform (e.g. to dismiss the notification it was told
                    // about) or replace itself.
                    let taken = callback.borrow_mut().take();
                    if let Some(mut taken) = taken {
                        taken(response);
                        callback.borrow_mut().get_or_insert(taken);
                    }
                }
            }));
        }
    }
}

/// The toast worker's loop: its own multithreaded apartment, the notifier
/// and the shown toasts live here and nowhere else.
fn run_toast_worker(
    commands: std_mpsc::Receiver<ToastCommand>,
    response_sender: mpsc::UnboundedSender<SystemNotificationResponse>,
) {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let mut toasts = Toasts {
        notifier: None,
        active_toasts: HashMap::new(),
        response_sender,
    };
    while let Ok(command) = commands.recv() {
        match command {
            ToastCommand::Show {
                has_package_identity,
                app_identity,
                notification,
            } => {
                let app_identity = app_identity
                    .as_ref()
                    .map(|(identifier, name)| (identifier.as_str(), name.as_str()));
                if let Err(error) = toasts.show(has_package_identity, app_identity, notification) {
                    log::error!("failed to show a system notification: {error}");
                }
            }
            ToastCommand::Dismiss(tag) => toasts.dismiss(&tag),
        }
    }
    drop(toasts);
    if initialized {
        unsafe { CoUninitialize() };
    }
}

struct Toasts {
    notifier: Option<ToastNotifier>,
    active_toasts: HashMap<SharedString, ToastNotification>,
    response_sender: mpsc::UnboundedSender<SystemNotificationResponse>,
}

impl Toasts {
    fn show(
        &mut self,
        has_package_identity: bool,
        app_identity: Option<(&str, &str)>,
        notification: SystemNotification,
    ) -> windows::core::Result<()> {
        let Some(notifier) = self.notifier(has_package_identity, app_identity)? else {
            return Ok(());
        };

        let document = toast_document(&notification)?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        // Windows caps toast tags at 64 characters (post-Creators Update),
        // so hash the arbitrary GPUI tag down to a fixed-width value.
        let tag = {
            let mut hasher = DefaultHasher::new();
            notification.tag.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        };
        toast.SetTag(&tag.into())?;

        let sender = self.response_sender.clone();
        let response_tag = notification.tag.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_sender, arguments| {
                let action_id = arguments
                    .as_ref()
                    .and_then(|arguments| arguments.cast::<ToastActivatedEventArgs>().ok())
                    .and_then(|arguments| arguments.Arguments().ok())
                    .filter(|arguments| !arguments.is_empty())
                    .map(|arguments| SharedString::from(arguments.to_string()));
                sender
                    .unbounded_send(SystemNotificationResponse {
                        tag: response_tag.clone(),
                        action_id,
                    })
                    .ok();
                Ok(())
            },
        ))?;

        if let Some(previous) = self.active_toasts.remove(&notification.tag) {
            notifier.Hide(&previous)?;
        }
        notifier.Show(&toast)?;
        self.active_toasts.insert(notification.tag, toast);
        Ok(())
    }

    fn dismiss(&mut self, tag: &str) {
        let Some(toast) = self.active_toasts.remove(tag) else {
            return;
        };
        let Some(notifier) = &self.notifier else {
            return;
        };
        if let Err(error) = notifier.Hide(&toast) {
            log::warn!("failed to dismiss system notification: {error}");
        }
    }

    fn notifier(
        &mut self,
        has_package_identity: bool,
        app_identity: Option<(&str, &str)>,
    ) -> windows::core::Result<Option<ToastNotifier>> {
        if let Some(notifier) = &self.notifier {
            return Ok(Some(notifier.clone()));
        }

        let notifier = if has_package_identity {
            ToastNotificationManager::CreateToastNotifier()?
        } else {
            let Some((app_identifier, app_name)) = app_identity else {
                log::warn!(
                    "cannot show a system notification without an app identity; \
                     call `App::set_app_identity` during startup"
                );
                return Ok(None);
            };
            register_app_user_model_id(app_identifier, app_name);
            ToastNotificationManager::CreateToastNotifierWithId(&app_identifier.into())?
        };

        self.notifier = Some(notifier.clone());
        Ok(Some(notifier))
    }
}

fn toast_document(notification: &SystemNotification) -> windows::core::Result<XmlDocument> {
    let document = XmlDocument::new()?;
    let toast = document.CreateElement(h!("toast"))?;
    document.AppendChild(&toast)?;

    let visual = document.CreateElement(h!("visual"))?;
    toast.AppendChild(&visual)?;
    let binding = document.CreateElement(h!("binding"))?;
    binding.SetAttribute(h!("template"), h!("ToastGeneric"))?;
    visual.AppendChild(&binding)?;
    for text in [&notification.title, &notification.body] {
        let element = document.CreateElement(h!("text"))?;
        element.SetInnerText(&text.as_ref().into())?;
        binding.AppendChild(&element)?;
    }

    if !notification.actions.is_empty() {
        let actions = document.CreateElement(h!("actions"))?;
        toast.AppendChild(&actions)?;
        for action in &notification.actions {
            let action_element = document.CreateElement(h!("action"))?;
            action_element.SetAttribute(h!("content"), &action.label.as_ref().into())?;
            action_element.SetAttribute(h!("arguments"), &action.id.as_ref().into())?;
            actions.AppendChild(&action_element)?;
        }
    }

    let audio = document.CreateElement(h!("audio"))?;
    audio.SetAttribute(h!("silent"), h!("true"))?;
    toast.AppendChild(&audio)?;
    Ok(document)
}

/// Registers the app's AUMID so toasts display correctly for an unpackaged app without a Start Menu shortcut.
fn register_app_user_model_id(app_identifier: &str, app_name: &str) {
    let result = windows_registry::CURRENT_USER
        .create(format!(r"Software\Classes\AppUserModelId\{app_identifier}"))
        .and_then(|key| key.set_string("DisplayName", app_name));
    if let Err(error) = result {
        log::warn!("failed to register AppUserModelID; notifications may not display: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// #ERR-2: a command never runs on the thread that sent it (the UI
    /// thread holding the App borrow), and commands run in send order.
    #[test]
    fn commands_run_in_order_on_the_worker_thread() {
        let seen: Arc<Mutex<Vec<(u32, std::thread::ThreadId)>>> = Arc::default();
        let (done_sender, done) = std_mpsc::channel();
        let mut worker = CommandWorker::new("toast-test", {
            let seen = seen.clone();
            move |commands: std_mpsc::Receiver<u32>| {
                while let Ok(command) = commands.recv() {
                    seen.lock().unwrap().push((command, std::thread::current().id()));
                    if command == 3 {
                        done_sender.send(()).unwrap();
                    }
                }
            }
        });
        for command in 1..=3 {
            worker.send(command);
        }
        done.recv_timeout(Duration::from_secs(10)).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.iter().map(|(command, _)| *command).collect::<Vec<_>>(), [1, 2, 3]);
        let caller = std::thread::current().id();
        assert!(seen.iter().all(|(_, thread)| *thread != caller));
        assert!(seen.windows(2).all(|pair| pair[0].1 == pair[1].1));
    }

    /// The platform's `show` only queues: it returns at once, with no
    /// notifier created on the calling thread.
    #[test]
    fn show_queues_without_touching_winrt_on_the_caller() {
        let mut state = SystemNotificationState::new();
        assert!(state.worker.sender.is_none());
        state
            .show(
                false,
                None,
                SystemNotification {
                    tag: "t".into(),
                    title: "title".into(),
                    body: "body".into(),
                    actions: Vec::new(),
                },
            )
            .unwrap();
        assert!(state.worker.sender.is_some());
        state.dismiss("t");
    }
}
