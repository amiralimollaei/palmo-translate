use anyhow::{Result, bail};

const SERVICE: &str = "io.github.amiralimollaei.PlamoTranslate";

fn entry(endpoint: &str) -> Result<keyring::Entry> {
    let account = crate::api::completion_url(endpoint)?.to_string();
    keyring::Entry::new(SERVICE, &account)
        .map_err(|_| anyhow::anyhow!("Could not access the OS credential store"))
}

// Call these blocking operations on a worker, never on the GTK thread.
// Do not format backend errors: some variants contain the raw secret bytes.
pub fn load(endpoint: &str) -> Result<String> {
    match entry(endpoint)?.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => bail!("No API key saved for this server; save one in Settings or disable Use API key"),
        Err(_) => bail!("Could not read the API key from the OS credential store; unlock or enable it and try again"),
    }
}

pub fn save(endpoint: &str, key: &str) -> Result<()> {
    crate::api::authorization_header(key)?;
    entry(endpoint)?.set_password(key)
        .map_err(|_| anyhow::anyhow!("Could not save the API key in the OS credential store; no plaintext copy was saved"))
}

pub fn delete(endpoint: &str) -> Result<()> {
    match entry(endpoint)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => bail!("Could not remove the API key from the OS credential store"),
    }
}
