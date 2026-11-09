mod api;
mod languages;

use adw::prelude::*;
use gtk::{gio, glib};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    endpoint: String,
    source: Option<String>,
    target: String,
    max_tokens: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9931".into(),
            source: None,
            target: "en".into(),
            max_tokens: 4096,
        }
    }
}

impl Settings {
    fn save(&self) -> anyhow::Result<()> {
        let path = settings_path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

fn settings_path() -> std::path::PathBuf {
    glib::user_config_dir()
        .join("palmo-translate")
        .join("settings.json")
}

fn main() -> glib::ExitCode {
    let runtime =
        Arc::new(tokio::runtime::Runtime::new().expect("Cannot start networking runtime"));
    let app = adw::Application::builder()
        .application_id("io.github.amiralimollaei.PalmoTranslate")
        .build();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            build_window(app, runtime.clone());
        }
    });
    app.run()
}

fn text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .into()
}

fn editor(title: &str, editable: bool) -> (gtk::Box, gtk::TextView) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
    panel.set_hexpand(true);
    panel.set_vexpand(true);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    panel.append(&label);
    let view = gtk::TextView::builder()
        .editable(editable)
        .cursor_visible(editable)
        .wrap_mode(gtk::WrapMode::WordChar)
        .left_margin(16)
        .right_margin(16)
        .top_margin(16)
        .bottom_margin(16)
        .vexpand(true)
        .build();
    view.update_property(&[gtk::accessible::Property::Label(title)]);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(170)
        .vexpand(true)
        .build();
    scroll.add_css_class("card");
    panel.append(&scroll);
    (panel, view)
}

fn build_window(app: &adw::Application, runtime: Arc<tokio::runtime::Runtime>) {
    let (settings, load_error) = match std::fs::read(settings_path()) {
        Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) => (settings, None),
            Err(error) => (
                Settings::default(),
                Some(format!("Could not read saved settings: {error}")),
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Settings::default(), None),
        Err(error) => (
            Settings::default(),
            Some(format!("Could not read saved settings: {error}")),
        ),
    };
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Palmo Translate")
        .default_width(960)
        .default_height(740)
        .build();
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Palmo Translate",
        "PLaMo API client",
    )));
    toolbar.add_top_bar(&header);
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&toolbar));
    window.set_content(Some(&overlay));
    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.set_margin_start(20);
    body.set_margin_end(20);
    body.set_margin_top(16);
    body.set_margin_bottom(20);
    toolbar.set_content(Some(&body));

    let preferences = adw::PreferencesDialog::builder().title("Settings").build();
    let page = adw::PreferencesPage::new();
    let connection = adw::PreferencesGroup::builder()
        .title("Translation server")
        .description("Text is sent only to this API server.")
        .build();
    let endpoint = adw::EntryRow::builder()
        .title("API server URL")
        .text(&settings.endpoint)
        .build();
    connection.add(&endpoint);
    let limit = adw::SpinRow::with_range(64.0, 16384.0, 64.0);
    limit.set_title("Maximum output tokens");
    limit.set_subtitle("Input and output together must fit the server context window.");
    limit.set_value(settings.max_tokens.clamp(64, 16384) as f64);
    connection.add(&limit);
    page.add(&connection);
    preferences.add(&page);
    let settings_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_button.set_tooltip_text(Some("Settings"));
    header.pack_end(&settings_button);
    settings_button.connect_clicked(glib::clone!(
        #[weak]
        window,
        #[strong]
        preferences,
        move |_| preferences.present(Some(&window))
    ));

    let language_names: Vec<&str> = languages::LANGUAGES.iter().map(|(_, name)| *name).collect();
    let mut source_names = vec!["Detect automatically"];
    source_names.extend_from_slice(&language_names);
    let source = gtk::DropDown::from_strings(&source_names);
    source.set_selected(
        settings
            .source
            .as_deref()
            .map(|s| languages::index(s) + 1)
            .unwrap_or(0),
    );
    source.set_hexpand(true);
    source.set_enable_search(true);
    source.update_property(&[gtk::accessible::Property::Label("Source language")]);
    let target = gtk::DropDown::from_strings(&language_names);
    target.set_selected(languages::index(&settings.target));
    target.set_hexpand(true);
    target.set_enable_search(true);
    target.update_property(&[gtk::accessible::Property::Label("Target language")]);
    let language_widths = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
    language_widths.add_widget(&source);
    language_widths.add_widget(&target);
    let swap = gtk::Button::from_icon_name("object-flip-horizontal-symbolic");
    swap.set_tooltip_text(Some("Swap languages and use translation as source"));
    let direction = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    direction.append(&source);
    direction.append(&swap);
    direction.append(&target);
    body.append(&direction);

    let (input_panel, input) = editor("Source text", true);
    let (output_panel, output) = editor("Translation", false);
    let panels = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    panels.set_homogeneous(true);
    panels.set_vexpand(true);
    panels.append(&input_panel);
    panels.append(&output_panel);
    body.append(&panels);
    // Reflow rather than forcing a desktop-width layout on small windows.
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Sp,
    ));
    breakpoint.add_setter(
        &panels,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    window.add_breakpoint(breakpoint);
    window.set_size_request(380, 620);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    preferences.connect_closed(glib::clone!(
        #[weak]
        endpoint,
        #[weak]
        limit,
        #[weak]
        source,
        #[weak]
        target,
        #[weak]
        overlay,
        move |_| {
            let settings = Settings {
                endpoint: endpoint.text().to_string(),
                source: source
                    .selected()
                    .checked_sub(1)
                    .map(|i| languages::LANGUAGES[i as usize].0.to_owned()),
                target: languages::LANGUAGES[target.selected() as usize]
                    .0
                    .to_owned(),
                max_tokens: limit.value() as u32,
            };
            if let Err(error) = settings.save() {
                overlay.add_toast(adw::Toast::new(&format!("Settings not saved: {error}")));
            }
        }
    ));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    controls.append(&spacer);
    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_tooltip_text(Some("Copy translation"));
    copy.set_sensitive(false);
    controls.append(&copy);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_visible(false);
    controls.append(&cancel);
    let translate = gtk::Button::with_label("Translate");
    translate.add_css_class("suggested-action");
    translate.set_sensitive(false);
    translate.set_tooltip_text(Some("Translate (Ctrl+Enter)"));
    controls.append(&translate);
    body.append(&controls);
    let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);
    let status = gtk::Label::new(Some("Ready · Text is sent only to the configured server"));
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_selectable(true);
    status.add_css_class("dim-label");
    status_row.append(&spinner);
    status_row.append(&status);
    body.append(&status_row);

    let job = Rc::new(RefCell::new(None::<tokio::task::JoinHandle<()>>));
    let generation = Rc::new(std::cell::Cell::new(0u64));
    input.buffer().connect_changed(glib::clone!(
        #[weak]
        translate,
        #[weak]
        status,
        #[weak]
        copy,
        #[weak]
        output,
        #[strong]
        job,
        move |buffer| {
            translate.set_sensitive(job.borrow().is_none() && !text(buffer).trim().is_empty());
            if job.borrow().is_none() && !text(&output.buffer()).is_empty() {
                status.set_text("Source changed · Translate again to update the result");
                copy.set_sensitive(true);
            }
        }
    ));
    output.buffer().connect_changed(glib::clone!(
        #[weak]
        copy,
        move |buffer| {
            copy.set_sensitive(buffer.char_count() > 0);
        }
    ));
    copy.connect_clicked(glib::clone!(
        #[weak]
        output,
        #[weak]
        overlay,
        move |_| {
            output.clipboard().set_text(&text(&output.buffer()));
            overlay.add_toast(adw::Toast::new("Translation copied"));
        }
    ));
    swap.connect_clicked(glib::clone!(
        #[weak]
        source,
        #[weak]
        target,
        #[weak]
        input,
        #[weak]
        output,
        #[weak]
        overlay,
        move |_| {
            let old_source = source.selected();
            if old_source == 0 {
                overlay.add_toast(adw::Toast::new("Select a source language before swapping"));
                return;
            }
            source.set_selected(target.selected() + 1);
            target.set_selected(old_source - 1);
            let translated = text(&output.buffer());
            if !translated.is_empty() {
                input.buffer().set_text(&translated);
                output.buffer().set_text("");
            }
        }
    ));

    let start = gio::SimpleAction::new("translate", None);
    start.connect_activate(glib::clone!(
        #[weak] input, #[weak] output, #[weak] source, #[weak] target,
        #[weak] endpoint, #[weak] limit, #[weak] translate, #[weak] cancel,
        #[weak] swap, #[weak] spinner, #[weak] status, #[weak] copy,
        #[strong] job, #[strong] generation, #[strong] runtime,
        move |_, _| {
            if job.borrow().is_some() || text(&input.buffer()).trim().is_empty() { return; }
            let request = api::TranslationRequest {
                text: text(&input.buffer()),
                source: source.selected().checked_sub(1).map(|i| languages::LANGUAGES[i as usize].0.to_owned()),
                target: languages::LANGUAGES[target.selected() as usize].0.to_owned(),
                max_tokens: limit.value() as u32,
            };
            let url = endpoint.text().to_string();
            let settings = Settings { endpoint: url.clone(), source: request.source.clone(), target: request.target.clone(), max_tokens: request.max_tokens };
            let save_error = settings.save().err().map(|e| format!(" · Settings not saved: {e}"));
            generation.set(generation.get() + 1);
            let id = generation.get();
            output.buffer().set_text("");
            copy.set_sensitive(false);
            for widget in [input.upcast_ref::<gtk::Widget>(), source.upcast_ref(), target.upcast_ref(), endpoint.upcast_ref(), limit.upcast_ref(), swap.upcast_ref(), translate.upcast_ref()] { widget.set_sensitive(false); }
            cancel.set_visible(true);
            spinner.set_visible(true);
            spinner.start();
            status.remove_css_class("error");
            status.set_text("Translating…");
            let (sender, receiver) = async_channel::bounded(16);
            let handle = runtime.spawn(async move {
                if let Err(error) = api::translate(&url, request, sender.clone()).await {
                    let _ = sender.send(api::TranslationEvent::Failed(format!("{error:#}"))).await;
                }
            });
            *job.borrow_mut() = Some(handle);
            glib::spawn_future_local(glib::clone!(
                #[weak] input, #[weak] output, #[weak] source, #[weak] target,
                #[weak] endpoint, #[weak] limit, #[weak] translate, #[weak] cancel,
                #[weak] swap, #[weak] spinner, #[weak] status,
                #[strong] job, #[strong] generation,
                async move {
                    let mut terminal = false;
                    while let Ok(event) = receiver.recv().await {
                        if generation.get() != id { return; }
                        match event {
                            api::TranslationEvent::Delta(delta) => {
                                let buffer = output.buffer();
                                buffer.insert(&mut buffer.end_iter(), &delta);
                                output.scroll_to_iter(&mut buffer.end_iter(), 0.0, false, 0.0, 1.0);
                            }
                            api::TranslationEvent::Finished { truncated } => {
                                let message = if truncated { "Output limit reached · Translation is incomplete; increase output tokens" } else { "Translation complete" };
                                status.set_text(&format!("{message}{}", save_error.as_deref().unwrap_or("")));
                                terminal = true;
                                break;
                            }
                            api::TranslationEvent::Failed(error) => {
                                let partial = if output.buffer().char_count() > 0 { " · Partial translation retained" } else { "" };
                                status.set_text(&format!("{error}{partial}"));
                                status.add_css_class("error");
                                terminal = true;
                                break;
                            }
                        }
                    }
                    if generation.get() != id { return; }
                    if !terminal {
                        status.set_text("Translation stream ended unexpectedly · Any partial translation is retained");
                        status.add_css_class("error");
                    }
                    job.borrow_mut().take();
                    for widget in [input.upcast_ref::<gtk::Widget>(), source.upcast_ref(), target.upcast_ref(), endpoint.upcast_ref(), limit.upcast_ref(), swap.upcast_ref()] { widget.set_sensitive(true); }
                    translate.set_sensitive(!text(&input.buffer()).trim().is_empty());
                    cancel.set_visible(false);
                    spinner.stop();
                    spinner.set_visible(false);
                }
            ));
        }
    ));
    window.add_action(&start);
    app.set_accels_for_action("win.translate", &["<Primary>Return"]);
    translate.set_action_name(Some("win.translate"));
    cancel.connect_clicked(glib::clone!(
        #[weak]
        input,
        #[weak]
        source,
        #[weak]
        target,
        #[weak]
        endpoint,
        #[weak]
        limit,
        #[weak]
        translate,
        #[weak]
        swap,
        #[weak]
        spinner,
        #[weak]
        status,
        #[strong]
        job,
        #[strong]
        generation,
        move |cancel| {
            generation.set(generation.get() + 1);
            if let Some(handle) = job.borrow_mut().take() {
                handle.abort();
            }
            for widget in [
                input.upcast_ref::<gtk::Widget>(),
                source.upcast_ref(),
                target.upcast_ref(),
                endpoint.upcast_ref(),
                limit.upcast_ref(),
                swap.upcast_ref(),
            ] {
                widget.set_sensitive(true);
            }
            translate.set_sensitive(!text(&input.buffer()).trim().is_empty());
            cancel.set_visible(false);
            spinner.stop();
            spinner.set_visible(false);
            status.remove_css_class("error");
            status.set_text("Cancelled · Any partial translation is retained; the server may still finish generation");
        }
    ));
    window.connect_close_request(move |_| {
        generation.set(generation.get() + 1);
        if let Some(handle) = job.borrow_mut().take() {
            handle.abort();
        }
        glib::Propagation::Proceed
    });
    if let Some(error) = load_error {
        overlay.add_toast(adw::Toast::new(&error));
    }
    window.present();
    input.grab_focus();
}
