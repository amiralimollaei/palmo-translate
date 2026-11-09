use anyhow::{Context, Result, bail};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct TranslationRequest {
    pub text: String,
    pub source: Option<String>,
    pub target: String,
    pub max_tokens: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TranslationEvent {
    Delta(String),
    Finished { truncated: bool },
    Failed(String),
}

fn completion_url(endpoint: &str) -> Result<Url> {
    let mut url =
        Url::parse(endpoint).map_err(|_| anyhow::anyhow!("Invalid server endpoint URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        bail!("Server endpoint must be an HTTP or HTTPS URL");
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("Server endpoint must not contain credentials, a query, or a fragment");
    }
    match url.path() {
        "" | "/" | "/v1" | "/v1/" => url.set_path("/v1/completions"),
        _ => bail!("Server endpoint path must be / or /v1"),
    }
    Ok(url)
}

fn prompt(request: &TranslationRequest) -> Result<String> {
    for (name, value) in [
        ("text", request.text.as_str()),
        ("target language", request.target.as_str()),
    ] {
        if value.contains("<|plamo:") {
            bail!("{name} must not contain the reserved <|plamo: token prefix");
        }
    }
    if let Some(source) = &request.source {
        if source.contains("<|plamo:") {
            bail!("source language must not contain the reserved <|plamo: token prefix");
        }
    }
    if request.target.trim().is_empty()
        || request.source.as_ref().is_some_and(|s| s.trim().is_empty())
    {
        bail!("Language codes must not be empty");
    }
    let mut result = String::from("<|plamo:tag|>dataset<|plamo:msg|>translation<|plamo:tag|>input");
    if let Some(source) = &request.source {
        result.push_str("<|plamo:key|>lang<|plamo:val|>");
        result.push_str(source);
    }
    result.push_str("<|plamo:msg|>");
    result.push_str(&request.text);
    result.push_str("<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>");
    result.push_str(&request.target);
    result.push_str("<|plamo:msg|>");
    Ok(result)
}

#[derive(Serialize)]
struct CompletionRequest {
    prompt: String,
    max_tokens: u32,
    temperature: u8,
    stop: [&'static str; 2],
    stream: bool,
}

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    text: String,
    finish_reason: Option<String>,
}

/// Incrementally decodes SSE lines as bytes so UTF-8 characters may span network chunks.
#[derive(Default)]
struct StreamParser {
    bytes: Vec<u8>,
    position: usize,
    skip_lf: bool,
    data: String,
    has_data: bool,
    has_text: bool,
    pending_finish: Option<bool>,
    finished: bool,
}

impl StreamParser {
    fn push(&mut self, bytes: &[u8]) {
        if self.position > 0 {
            self.bytes.drain(..self.position);
            self.position = 0;
        }
        self.bytes.extend_from_slice(bytes);
    }

    fn next_event(&mut self) -> Result<Option<TranslationEvent>> {
        if let Some(truncated) = self.pending_finish.take() {
            self.finished = true;
            return Ok(Some(TranslationEvent::Finished { truncated }));
        }
        if self.finished {
            return Ok(None);
        }
        loop {
            if self.skip_lf {
                if self.position == self.bytes.len() {
                    return Ok(None);
                }
                if self.bytes[self.position] == b'\n' {
                    self.position += 1;
                }
                self.skip_lf = false;
            }
            let Some(length) = self.bytes[self.position..]
                .iter()
                .position(|&byte| byte == b'\r' || byte == b'\n')
            else {
                return Ok(None);
            };
            let end = self.position + length;
            let line = std::str::from_utf8(&self.bytes[self.position..end])
                .context("Invalid UTF-8 in translation stream")?;
            let delimiter = self.bytes[end];
            self.position = end + 1;
            self.skip_lf = delimiter == b'\r';
            if line.is_empty() {
                if !self.has_data {
                    continue;
                }
                self.has_data = false;
                let data = std::mem::take(&mut self.data);
                if data == "[DONE]" {
                    bail!("Translation stream ended without a finish reason");
                }
                let value: serde_json::Value =
                    serde_json::from_str(&data).context("Invalid JSON in translation stream")?;
                if let Some(error) = value.get("error") {
                    let message = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .or_else(|| error.as_str())
                        .unwrap_or("Unknown server error");
                    bail!("Translation server error: {message}");
                }
                let response: CompletionResponse = serde_json::from_value(value)
                    .context("Invalid completion response in translation stream")?;
                let choice = response
                    .choices
                    .into_iter()
                    .next()
                    .context("Completion response has no choices")?;
                if let Some(reason) = choice.finish_reason {
                    if !self.has_text && choice.text.is_empty() {
                        bail!("Completion response contains an empty translation");
                    }
                    let truncated = reason == "length";
                    if choice.text.is_empty() {
                        self.finished = true;
                        return Ok(Some(TranslationEvent::Finished { truncated }));
                    }
                    self.pending_finish = Some(truncated);
                }
                if !choice.text.is_empty() {
                    self.has_text = true;
                    return Ok(Some(TranslationEvent::Delta(choice.text)));
                }
            } else if !line.starts_with(':') {
                let (field, value) = line.split_once(':').unwrap_or((line, ""));
                if field == "data" {
                    if self.has_data {
                        self.data.push('\n');
                    }
                    self.has_data = true;
                    self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
                }
            }
        }
    }

    fn end(&self) -> Result<()> {
        std::str::from_utf8(&self.bytes[self.position..])
            .context("Invalid UTF-8 in translation stream")?;
        if !self.finished {
            bail!("Translation stream ended before a finish reason");
        }
        Ok(())
    }
}

pub async fn translate(
    endpoint: &str,
    request: TranslationRequest,
    events: async_channel::Sender<TranslationEvent>,
) -> Result<()> {
    let url = completion_url(endpoint)?;
    let body = CompletionRequest {
        prompt: prompt(&request)?,
        max_tokens: request.max_tokens,
        temperature: 0,
        stop: ["<|plamo:tag|>", "<|plamo:bos|>"],
        stream: true,
    };
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("Could not create HTTP client")?;
    let mut response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.without_url())
        .context("Translation request failed")?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .map_err(|e| e.without_url())
            .context("Could not read translation response")?;
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message").or(Some(e)))
                    .and_then(|m| m.as_str())
                    .map(str::to_owned)
            });
        match message {
            Some(message) if !message.is_empty() => {
                bail!("Translation server returned HTTP {status}: {message}")
            }
            _ => bail!("Translation server returned HTTP {status}"),
        }
    }
    let mut parser = StreamParser::default();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| e.without_url())
        .context("Could not read translation stream")?
    {
        parser.push(&chunk);
        while let Some(event) = parser.next_event()? {
            let finished = matches!(event, TranslationEvent::Finished { .. });
            events
                .send(event)
                .await
                .context("Translation event receiver closed")?;
            if finished {
                return Ok(());
            }
        }
    }
    parser.end()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> TranslationRequest {
        TranslationRequest {
            text: "  hello\n".into(),
            source: None,
            target: "ja".into(),
            max_tokens: 100,
        }
    }

    #[test]
    fn prompt_preserves_text_and_optional_source() {
        let mut r = request();
        assert!(
            prompt(&r)
                .unwrap()
                .contains("<|plamo:msg|>  hello\n<|plamo:tag|>output")
        );
        r.source = Some("en".into());
        assert!(
            prompt(&r).unwrap().contains(
                "<|plamo:tag|>input<|plamo:key|>lang<|plamo:val|>en<|plamo:msg|>  hello\n"
            )
        );
        r.text = "<|plamo:unknown|>".into();
        assert!(prompt(&r).is_err());
        r.text.clear();
        r.target = "<|plamo:tag|>".into();
        assert!(prompt(&r).is_err());
    }

    #[test]
    fn endpoint_forms_and_rejections() {
        for endpoint in [
            "http://example.com",
            "http://example.com/",
            "http://example.com/v1",
            "http://example.com/v1/",
        ] {
            assert_eq!(
                completion_url(endpoint).unwrap().as_str(),
                "http://example.com/v1/completions"
            );
        }
        for endpoint in [
            "ftp://example.com",
            "http://user:pass@example.com",
            "http://example.com/?key=secret",
            "http://example.com/#fragment",
            "http://example.com/v2",
        ] {
            assert!(completion_url(endpoint).is_err());
        }
    }

    #[test]
    fn unicode_split_at_every_byte_and_crlf() {
        let wire = ": comment\r\ndata: {\"choices\":[{\"text\":\" 日本語\",\"finish_reason\":null}]}\r\n\r\ndata: {\"choices\":[{\"text\":\"!\",\"finish_reason\":\"stop\"}]}\r\n\r\n";
        let mut parser = StreamParser::default();
        let mut events = Vec::new();
        for byte in wire.bytes() {
            parser.push(&[byte]);
            while let Some(event) = parser.next_event().unwrap() {
                events.push(event);
            }
        }
        assert_eq!(
            events,
            vec![
                TranslationEvent::Delta(" 日本語".into()),
                TranslationEvent::Delta("!".into()),
                TranslationEvent::Finished { truncated: false },
            ]
        );
        parser.end().unwrap();
    }

    #[test]
    fn multiline_data_and_terminal_text() {
        for (reason, truncated) in [("length", true), ("stop", false)] {
            let wire = format!(
                "data: {{\"choices\":\ndata: [{{\"text\":\" end\",\"finish_reason\":\"{reason}\"}}]}}\n\n"
            );
            let mut parser = StreamParser::default();
            parser.push(wire.as_bytes());
            assert_eq!(
                parser.next_event().unwrap(),
                Some(TranslationEvent::Delta(" end".into()))
            );
            assert_eq!(
                parser.next_event().unwrap(),
                Some(TranslationEvent::Finished { truncated })
            );
            parser.end().unwrap();
        }
    }

    #[test]
    fn malformed_and_server_errors() {
        for wire in [
            "data: not-json\n\n",
            "data: {\"error\":{\"message\":\"server failure\"}}\n\n",
            "data: {\"choices\":[]}\n\n",
            "data: {\"choices\":[{\"text\":\"\",\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        ] {
            let mut parser = StreamParser::default();
            parser.push(wire.as_bytes());
            assert!(parser.next_event().is_err(), "{wire}");
        }
        let mut parser = StreamParser::default();
        parser.push(b"data: \xff\n\n");
        assert!(parser.next_event().is_err());
    }

    #[test]
    fn premature_eof_retains_previously_emitted_text() {
        let mut parser = StreamParser::default();
        parser.push(b"data: {\"choices\":[{\"text\":\"partial\",\"finish_reason\":null}]}\n\n");
        assert_eq!(
            parser.next_event().unwrap(),
            Some(TranslationEvent::Delta("partial".into()))
        );
        assert_eq!(parser.next_event().unwrap(), None);
        assert!(parser.end().is_err());
    }
}
