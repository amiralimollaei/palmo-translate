use crate::{api, credentials, languages, model::{self, Settings}, view::View};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .into()
}

pub(crate) fn build_window(app: &adw::Application, runtime: Arc<tokio::runtime::Runtime>) {
    let (settings, load_error) = model::load();
    let View {
        window, preferences, page, endpoint, model, limit, use_api_key, api_key,
        save_key, remove_key, source, target, swap, input, output, copy, cancel,
        translate, result_status, overlay, settings_button,
    } = View::new(app, &settings);

    endpoint.connect_changed(glib::clone!(
        #[weak]
        api_key,
        move |_| api_key.set_text("")
    ));
    settings_button.connect_clicked(glib::clone!(
        #[weak]
        window,
        #[strong]
        preferences,
        move |_| preferences.present(Some(&window))
    ));

    preferences.connect_closed(glib::clone!(
        #[weak]
        endpoint,
        #[weak]
        limit,
        #[weak]
        model,
        #[weak]
        use_api_key,
        #[weak]
        api_key,
        #[weak]
        source,
        #[weak]
        target,
        #[weak]
        overlay,
        move |_| {
            api_key.set_text("");
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
                model: model.text().trim().to_owned(),
                use_api_key: use_api_key.is_active(),
            };
            if let Err(error) = settings.save() {
                overlay.add_toast(adw::Toast::new(&format!("Settings not saved: {error}")));
            }
        }
    ));

    let job = Rc::new(RefCell::new(None::<tokio::task::JoinHandle<()>>));
    let generation = Rc::new(std::cell::Cell::new(0u64));
    let credential_busy = Rc::new(std::cell::Cell::new(false));
    for (button, remove) in [(&save_key, false), (&remove_key, true)] {
        button.connect_clicked(glib::clone!(
            #[weak] endpoint, #[weak] api_key, #[weak] page, #[weak] preferences,
            #[weak] overlay,
            #[strong] runtime, #[strong] credential_busy, #[strong] job,
            move |_| {
                if credential_busy.get() || job.borrow().is_some() {
                    preferences.add_toast(adw::Toast::new("Wait for the current operation to finish"));
                    return;
                }
                let key = if remove { String::new() } else { api_key.text().to_string() };
                if !remove && key.trim().is_empty() {
                    preferences.add_toast(adw::Toast::new("Enter an API key to save"));
                    return;
                }
                let url = endpoint.text().to_string();
                api_key.set_text("");
                credential_busy.set(true);
                page.set_sensitive(false);
                let task = runtime.spawn_blocking(move || {
                    if remove { credentials::delete(&url) } else { credentials::save(&url, &key) }
                });
                glib::spawn_future_local(glib::clone!(
                    #[weak] page, #[weak] preferences, #[weak] overlay,
                    #[strong] credential_busy,
                    async move {
                        let result = task.await.unwrap_or_else(|_| Err(anyhow::anyhow!("Credential-store operation failed")));
                        credential_busy.set(false);
                        page.set_sensitive(true);
                        let message = match result {
                            Ok(()) if remove => "API key removed · Disable Use API key for unauthenticated requests".to_owned(),
                            Ok(()) => "API key saved securely · Enable Use API key to send it".to_owned(),
                            Err(error) => error.to_string(),
                        };
                        let toast = adw::Toast::new(&message);
                        if preferences.is_mapped() {
                            preferences.add_toast(toast);
                        } else {
                            overlay.add_toast(toast);
                        }
                    }
                ));
            }
        ));
    }
    input.buffer().connect_changed(glib::clone!(
        #[weak]
        translate,
        #[strong]
        job,
        move |buffer| {
            translate.set_sensitive(job.borrow().is_none() && !text(buffer).trim().is_empty());
        }
    ));
    output.buffer().connect_changed(glib::clone!(
        #[weak]
        copy,
        #[weak]
        result_status,
        move |buffer| {
            copy.set_sensitive(buffer.char_count() > 0);
            if buffer.char_count() == 0 {
                result_status.set_visible(false);
            }
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
        #[weak] endpoint, #[weak] limit, #[weak] model, #[weak] use_api_key,
        #[weak] translate, #[weak] cancel, #[weak] overlay,
        #[weak] swap, #[weak] copy, #[weak] result_status,
        #[strong] job, #[strong] generation, #[strong] runtime, #[strong] credential_busy,
        move |_, _| {
            if credential_busy.get() {
                overlay.add_toast(adw::Toast::new("Wait for the credential-store operation to finish"));
                return;
            }
            if job.borrow().is_some() || text(&input.buffer()).trim().is_empty() { return; }
            let request = api::TranslationRequest {
                text: text(&input.buffer()),
                source: source.selected().checked_sub(1).map(|i| languages::LANGUAGES[i as usize].0.to_owned()),
                target: languages::LANGUAGES[target.selected() as usize].0.to_owned(),
                max_tokens: limit.value() as u32,
                model: Some(model.text().trim().to_owned()).filter(|value| !value.is_empty()),
            };
            let url = endpoint.text().to_string();
            let authenticate = use_api_key.is_active();
            let settings = Settings { endpoint: url.clone(), source: request.source.clone(), target: request.target.clone(), max_tokens: request.max_tokens, model: model.text().trim().to_owned(), use_api_key: authenticate };
            if let Err(error) = settings.save() {
                overlay.add_toast(adw::Toast::new(&format!("Settings not saved: {error}")));
            }
            generation.set(generation.get() + 1);
            let id = generation.get();
            output.buffer().set_text("");
            result_status.set_visible(false);
            copy.set_sensitive(false);
            for widget in [input.upcast_ref::<gtk::Widget>(), source.upcast_ref(), target.upcast_ref(), endpoint.upcast_ref(), limit.upcast_ref(), model.upcast_ref(), use_api_key.upcast_ref(), swap.upcast_ref(), translate.upcast_ref()] { widget.set_sensitive(false); }
            cancel.set_visible(true);
            let (sender, receiver) = async_channel::bounded(16);
            let handle = runtime.spawn(async move {
                let result = async {
                    let key = if authenticate {
                        let endpoint = url.clone();
                        Some(tokio::task::spawn_blocking(move || credentials::load(&endpoint))
                            .await.map_err(|_| anyhow::anyhow!("Credential-store operation failed"))??)
                    } else { None };
                    api::translate(&url, request, key.as_deref(), sender.clone()).await
                }.await;
                if let Err(error) = result {
                    let _ = sender.send(api::TranslationEvent::Failed(format!("{error:#}"))).await;
                }
            });
            *job.borrow_mut() = Some(handle);
            glib::spawn_future_local(glib::clone!(
                #[weak] input, #[weak] output, #[weak] source, #[weak] target,
                #[weak] endpoint, #[weak] limit, #[weak] model, #[weak] use_api_key,
                #[weak] translate, #[weak] cancel,
                #[weak] swap, #[weak] result_status,
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
                                if truncated {
                                    result_status.set_text("Output limit reached · Translation is incomplete; increase output tokens");
                                    result_status.set_visible(true);
                                }
                                terminal = true;
                                break;
                            }
                            api::TranslationEvent::Failed(error) => {
                                let partial = if output.buffer().char_count() > 0 { " · Partial translation retained" } else { "" };
                                result_status.set_text(&format!("{error}{partial}"));
                                result_status.set_visible(true);
                                terminal = true;
                                break;
                            }
                        }
                    }
                    if generation.get() != id { return; }
                    if !terminal {
                        result_status.set_text("Translation stream ended unexpectedly · Any partial translation is retained");
                        result_status.set_visible(true);
                    }
                    job.borrow_mut().take();
                    for widget in [input.upcast_ref::<gtk::Widget>(), source.upcast_ref(), target.upcast_ref(), endpoint.upcast_ref(), limit.upcast_ref(), model.upcast_ref(), use_api_key.upcast_ref(), swap.upcast_ref()] { widget.set_sensitive(true); }
                    translate.set_sensitive(!text(&input.buffer()).trim().is_empty());
                    cancel.set_visible(false);
                }
            ));
        }
    ));
    window.add_action(&start);
    app.set_accels_for_action("win.translate", &["<Primary>Return"]);
    translate.set_action_name(Some("win.translate"));
    cancel.connect_clicked(glib::clone!(
        #[weak]
        output,
        #[weak]
        result_status,
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
        model,
        #[weak]
        use_api_key,
        #[weak]
        translate,
        #[weak]
        swap,
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
                model.upcast_ref(),
                use_api_key.upcast_ref(),
            ] {
                widget.set_sensitive(true);
            }
            translate.set_sensitive(!text(&input.buffer()).trim().is_empty());
            cancel.set_visible(false);
            if output.buffer().char_count() > 0 {
                result_status.set_text("Cancelled · Partial translation retained");
                result_status.set_visible(true);
            }
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
