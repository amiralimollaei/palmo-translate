use crate::{languages, model::Settings};
use adw::prelude::*;

pub(crate) struct View {
    pub(crate) window: adw::ApplicationWindow,
    pub(crate) preferences: adw::PreferencesDialog,
    pub(crate) page: adw::PreferencesPage,
    pub(crate) endpoint: adw::EntryRow,
    pub(crate) model: adw::EntryRow,
    pub(crate) limit: adw::SpinRow,
    pub(crate) use_api_key: adw::SwitchRow,
    pub(crate) api_key: adw::PasswordEntryRow,
    pub(crate) save_key: gtk::Button,
    pub(crate) remove_key: gtk::Button,
    pub(crate) settings_button: gtk::Button,
    pub(crate) source: gtk::DropDown,
    pub(crate) target: gtk::DropDown,
    pub(crate) swap: gtk::Button,
    pub(crate) input: gtk::TextView,
    pub(crate) output: gtk::TextView,
    pub(crate) copy: gtk::Button,
    pub(crate) cancel: gtk::Button,
    pub(crate) translate: gtk::Button,
    pub(crate) result_status: gtk::Label,
    pub(crate) overlay: adw::ToastOverlay,
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

impl View {
    pub(crate) fn new(app: &adw::Application, settings: &Settings) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Plamo Translate")
            .default_width(960)
            .default_height(740)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(
            "Plamo Translate",
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
        let model = adw::EntryRow::builder()
            .title("Model ID (optional)")
            .text(&settings.model)
            .build();
        connection.add(&model);
        let limit = adw::SpinRow::with_range(64.0, 16384.0, 64.0);
        limit.set_title("Maximum output tokens");
        limit.set_subtitle("Input and output together must fit the server context window.");
        limit.set_value(settings.max_tokens.clamp(64, 16384) as f64);
        connection.add(&limit);
        page.add(&connection);
        let authentication = adw::PreferencesGroup::builder()
            .title("API authentication")
            .description("Keys are stored in the OS credential store for this server only. HTTP exposes keys in transit; use HTTPS for remote servers.")
            .build();
        let use_api_key = adw::SwitchRow::builder()
            .title("Use API key")
            .subtitle("Enable after saving a key. Missing or locked keys block translation.")
            .active(settings.use_api_key)
            .build();
        authentication.add(&use_api_key);
        let api_key = adw::PasswordEntryRow::builder()
            .title("New API key")
            .build();
        let save_key = gtk::Button::with_label("Save key");
        save_key.set_valign(gtk::Align::Center);
        api_key.add_suffix(&save_key);
        authentication.add(&api_key);
        let remove_key = gtk::Button::with_label("Remove saved key");
        remove_key.set_margin_top(12);
        authentication.add(&remove_key);
        page.add(&authentication);
        preferences.add(&page);
        let settings_button = gtk::Button::from_icon_name("emblem-system-symbolic");
        settings_button.set_tooltip_text(Some("Settings"));
        header.pack_end(&settings_button);

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
        let search_expression = gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            gtk::Expression::NONE,
            "string",
        );
        source.set_expression(Some(&search_expression));
        source.set_enable_search(true);
        source.update_property(&[gtk::accessible::Property::Label("Source language")]);
        let target = gtk::DropDown::from_strings(&language_names);
        target.set_selected(languages::index(&settings.target));
        target.set_expression(Some(&search_expression));
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
        let result_status = gtk::Label::new(None);
        result_status.set_xalign(0.0);
        result_status.set_wrap(true);
        result_status.set_selectable(true);
        result_status.add_css_class("error");
        result_status.set_visible(false);
        body.append(&result_status);

        Self {
            window,
            preferences,
            page,
            endpoint,
            model,
            limit,
            use_api_key,
            api_key,
            save_key,
            remove_key,
            settings_button,
            source,
            target,
            swap,
            input,
            output,
            copy,
            cancel,
            translate,
            result_status,
            overlay,
        }
    }
}
