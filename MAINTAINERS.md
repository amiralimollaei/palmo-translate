# Maintainer guide

Plamo Translate is a GTK4 and libadwaita desktop client for a separate PLaMo 3 Translate server. The [user README](README.md) describes server requirements, installation, configuration, and source-build prerequisites.

## Architecture

The application follows a model-view-controller separation: `src/model.rs` manages saved settings, `src/view.rs` constructs the GTK widgets, and `src/controller.rs` connects user actions to settings, credentials, and translation requests. `src/main.rs` starts the application; `src/api.rs`, `src/credentials.rs`, and `src/languages.rs` provide the translation, credential storage, and language data used by the controller and view.

## Translation API

The client follows the model author's [production recommendation](https://huggingface.co/pfnet/plamo-3-translate-8b-GGUF#pfml3-prompt-format): construct PFML3 and POST to `/v1/completions` with `stream: true`, temperature zero, an explicit output limit, and `<|plamo:tag|>` / `<|plamo:bos|>` stop sequences. SSE text deltas are appended immediately; a broken stream is reported instead of treating partial text as a completed translation. A configured Model ID is sent as `model`; when empty, that field is omitted to use the server's default model. Literal PLaMo control tokens in source text are rejected rather than interpreted as prompt structure.

## Verification

Run the automated tests with:

```sh
cargo test --locked
```

To exercise the API against a running server without authentication, provide source text, a target language code, and the server URL; an optional fourth argument specifies the Model ID:

```sh
cargo run --locked --example translate -- '会議は来週の月曜日に変更されました。' en http://localhost:9931
```

## Publishing releases

Publishing a GitHub Release, including a prerelease, triggers `.github/workflows/release.yml`. It checks out the release tag, tests and compiles natively, then attaches the platform archives and `SHA256SUMS` after all builds succeed.

Push the workflow and packaging script before creating the release tag, then publish a release for that tag. Draft releases and tag pushes alone do not trigger builds. The workflow uses the built-in `GITHUB_TOKEN`; no extra repository secrets are required. Failed builds leave the published release without new assets; after fixing the cause, rerun the failed workflow as appropriate. Rerunning successful builds replaces assets with the same names.
