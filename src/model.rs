use gtk::glib;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) endpoint: String,
    pub(crate) source: Option<String>,
    pub(crate) target: String,
    pub(crate) max_tokens: u32,
    pub(crate) model: String,
    pub(crate) use_api_key: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9931".into(),
            source: None,
            target: "en".into(),
            max_tokens: 4096,
            model: String::new(),
            use_api_key: false,
        }
    }
}

impl Settings {
    pub(crate) fn save(&self) -> anyhow::Result<()> {
        let path = settings_path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

fn settings_path() -> std::path::PathBuf {
    glib::user_config_dir()
        .join("plamo-translate")
        .join("settings.json")
}

pub(crate) fn load() -> (Settings, Option<String>) {
    match std::fs::read(settings_path()) {
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
    }
}
