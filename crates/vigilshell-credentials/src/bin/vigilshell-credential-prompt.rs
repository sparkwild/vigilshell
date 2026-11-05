//! 独立原生输入进程。stdout 仅写有界二进制秘密响应。
#[cfg(target_os = "macos")]
mod native {
    use objc2::{
        AllocAnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
    };
    use objc2_app_kit::{
        NSAlert, NSAlertFirstButtonReturn, NSAppearance, NSAppearanceCustomization,
        NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
        NSApplicationActivationPolicy, NSImage, NSMenu, NSMenuItem, NSModalPanelRunLoopMode,
        NSOpenPanel, NSSecureTextField,
    };
    use objc2_foundation::{
        NSObject, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSSize, NSString, NSTimer,
    };
    use std::{
        cell::RefCell,
        io::{self, BufRead, Read},
        sync::mpsc,
    };
    use vigilshell_credentials::{MAX_SECRET, PromptRequest, messages::message};
    use zeroize::Zeroizing;
    enum Surface {
        Alert(Retained<NSAlert>, Retained<NSSecureTextField>),
        File(Retained<NSOpenPanel>),
    }
    struct Ui {
        surface: Surface,
        context: PromptRequest,
        rx: mpsc::Receiver<Option<PromptRequest>>,
    }
    thread_local! {static UI:RefCell<Option<Ui>>=const{RefCell::new(None)};}
    fn string(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }
    fn load_application_icon() -> io::Result<Retained<NSImage>> {
        let executable = std::env::current_exe()?;
        let contents = executable
            .parent()
            .and_then(|path| path.parent())
            .ok_or_else(|| io::Error::other("missing bundle"))?;
        let path = contents.join("Resources/vigilshell.icns");
        let image = NSImage::initWithContentsOfFile(
            NSImage::alloc(),
            &string(
                path.to_str()
                    .ok_or_else(|| io::Error::other("invalid icon path"))?,
            ),
        )
        .ok_or_else(|| io::Error::other("missing application icon"))?;
        let name = string("NSApplicationIcon");
        if let Some(previous) = NSImage::imageNamed(&name) {
            previous.setName(None);
        }
        if !image.setName(Some(&name)) {
            return Err(io::Error::other("application icon registration failed"));
        }
        Ok(image)
    }
    fn install_edit_menu(locale: &str) {
        let mtm = MainThreadMarker::new().expect("native main thread");
        let main = NSMenu::new(mtm);
        let edit = NSMenu::new(mtm);
        let item = NSMenuItem::new(mtm);
        item.setTitle(&string(message("native_edit", locale)));
        edit.setTitle(&string(message("native_edit", locale)));
        for (key, action, shortcut) in [
            ("native_paste", sel!(paste:), "v"),
            ("native_select_all", sel!(selectAll:), "a"),
        ] {
            unsafe {
                edit.addItemWithTitle_action_keyEquivalent(
                    &string(message(key, locale)),
                    Some(action),
                    &string(shortcut),
                );
            }
        }
        item.setSubmenu(Some(&edit));
        main.addItem(&item);
        NSApplication::sharedApplication(mtm).setMainMenu(Some(&main));
    }
    fn apply(ui: &Ui) {
        let c = &ui.context;
        let locale = c.locale.as_str();
        install_edit_menu(locale);
        let appearance = NSAppearance::appearanceNamed(unsafe {
            if c.theme == "dark" {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        });
        match &ui.surface {
            Surface::Alert(alert, field) => {
                let passphrase = c.kind == "passphrase";
                alert.setMessageText(&string(message(
                    if passphrase {
                        "native_passphrase_title"
                    } else {
                        "native_password_title"
                    },
                    locale,
                )));
                alert.setInformativeText(&string(message(
                    if passphrase {
                        "native_passphrase_hint"
                    } else {
                        "native_password_hint"
                    },
                    locale,
                )));
                field.setPlaceholderString(Some(&string(message(
                    if passphrase {
                        "native_passphrase_placeholder"
                    } else {
                        "native_password_placeholder"
                    },
                    locale,
                ))));
                let buttons = alert.buttons();
                buttons
                    .objectAtIndex(0)
                    .setTitle(&string(message("native_continue", locale)));
                buttons
                    .objectAtIndex(1)
                    .setTitle(&string(message("native_cancel", locale)));
                alert.window().setAppearance(appearance.as_deref());
                alert.layout();
            }
            Surface::File(panel) => {
                panel.setTitle(Some(&string(message("native_private_key_title", locale))));
                panel.setPrompt(Some(&string(message("native_continue", locale))));
                panel.setAppearance(appearance.as_deref());
            }
        }
    }
    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind=MainThreadOnly]
        struct Ticker;
        unsafe impl NSObjectProtocol for Ticker {}
        impl Ticker {
            #[unsafe(method(tick:))]
            fn tick(&self,_timer:&NSTimer){UI.with(|state|{let mut state=state.borrow_mut();let Some(ui)=state.as_mut() else{return};
                loop {match ui.rx.try_recv(){
                    Ok(Some(context))=>{ui.context=context;apply(ui);},
                    Ok(None)|Err(mpsc::TryRecvError::Disconnected)=>{NSApplication::sharedApplication(self.mtm()).abortModal();break;},
                    Err(mpsc::TryRecvError::Empty)=>break,
                }}
            });}
        }
    );
    impl Ticker {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            unsafe { msg_send![Self::alloc(mtm), init] }
        }
    }
    fn request(r: &mut impl BufRead) -> Option<PromptRequest> {
        let mut text = String::new();
        let n = r.take(4097).read_line(&mut text).ok()?;
        if n == 0 || n > 4096 {
            return None;
        }
        serde_json::from_str(&text).ok()
    }
    pub fn run() -> io::Result<()> {
        let mut input = io::BufReader::new(io::stdin());
        let Some(context) = request(&mut input) else {
            return Ok(());
        };
        let kind = context.kind.clone();
        if !["password", "passphrase", "key"].contains(&kind.as_str()) {
            return Ok(());
        }
        let (tx, rx) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            while let Some(context) = request(&mut input) {
                if tx.send(Some(context)).is_err() {
                    return;
                }
            }
            let _ = tx.send(None);
        });
        let mtm = MainThreadMarker::new().expect("native main thread");
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        let _icon = load_application_icon()?;
        let surface = if kind == "key" {
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setCanChooseDirectories(false);
            panel.setCanChooseFiles(true);
            panel.setAllowsMultipleSelection(false);
            Surface::File(panel)
        } else {
            let alert = NSAlert::new(mtm);
            alert.addButtonWithTitle(&string("Continue"));
            alert.addButtonWithTitle(&string("Cancel"));
            let field = NSSecureTextField::new(mtm);
            field.setUsesSingleLineMode(true);
            if let Some(cell) = field.cell() {
                cell.setWraps(false);
                cell.setScrollable(true);
            }
            field.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(340.0, 26.0)));
            alert.setAccessoryView(Some(&field));
            Surface::Alert(alert, field)
        };
        UI.with(|ui| {
            *ui.borrow_mut() = Some(Ui {
                surface,
                context,
                rx,
            })
        });
        UI.with(|ui| apply(ui.borrow().as_ref().unwrap()));
        let ticker = Ticker::new(mtm);
        // The modal loop uses its own run-loop mode; updates must run there too.
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                0.04,
                &ticker,
                sel!(tick:),
                None,
                true,
            )
        };
        unsafe {
            NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSModalPanelRunLoopMode);
        }
        let (alert, panel) = UI.with(|ui| match &ui.borrow().as_ref().unwrap().surface {
            Surface::Alert(a, f) => {
                a.window().setInitialFirstResponder(Some(f));
                a.window().makeFirstResponder(Some(f));
                (Some(a.clone()), None)
            }
            Surface::File(p) => (None, Some(p.clone())),
        });
        let response = if let Some(a) = &alert {
            a.runModal()
        } else {
            panel.as_ref().unwrap().runModal()
        };
        timer.invalidate();
        let mut status = 0u8;
        let mut label = String::new();
        let mut value = Zeroizing::new(Vec::new());
        let mut input_consistent = None;
        if (alert.is_some() && response == NSAlertFirstButtonReturn)
            || (panel.is_some() && response == 1)
        {
            if let Some(panel) = panel {
                if let Some(path) = panel
                    .URL()
                    .and_then(|url| url.path())
                    .map(|p| std::path::PathBuf::from(p.to_string()))
                {
                    let read = (|| {
                        let file = std::fs::File::open(&path)?;
                        if !file.metadata()?.is_file() {
                            return Err(io::ErrorKind::InvalidInput.into());
                        }
                        file.take((MAX_SECRET + 1) as u64).read_to_end(&mut value)?;
                        Ok::<_, io::Error>(())
                    })();
                    if read.is_ok() && value.len() <= MAX_SECRET {
                        label = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .chars()
                            .take(128)
                            .collect();
                        status = 1;
                    } else {
                        status = 2;
                        value.clear();
                    }
                }
            } else {
                UI.with(|ui| {
                    if let Surface::Alert(_, field) = &ui.borrow().as_ref().unwrap().surface {
                        let captured = Zeroizing::new(field.stringValue().to_string());
                        if std::env::var_os("VIGILSHELL_AUTH_DIAGNOSTICS").is_some_and(|v| v == "1")
                        {
                            input_consistent = field.currentEditor().map(|editor| {
                                let editing = Zeroizing::new(editor.string().to_string());
                                editing.as_bytes() == captured.as_bytes()
                            });
                        }
                        value.extend_from_slice(captured.as_bytes());
                        field.setStringValue(&string(""));
                    }
                });
                status = 1;
            }
        }
        UI.with(|ui| {
            if let Some(ui) = ui.borrow_mut().take()
                && let Surface::Alert(_, field) = ui.surface
            {
                field.setStringValue(&string(""));
            }
        });
        vigilshell_credentials::prompt_response::write(
            &mut io::stdout().lock(),
            status,
            input_consistent,
            &label,
            &value,
        )
    }
}
fn main() {
    #[cfg(target_os = "macos")]
    {
        let _ = native::run();
    }
}
