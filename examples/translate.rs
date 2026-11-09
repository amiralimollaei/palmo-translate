#[path = "../src/api.rs"]
mod api;

use anyhow::Result;
use std::io::{self, Write};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let text = args
        .next()
        .unwrap_or_else(|| "会議は来週の月曜日に変更されました。".to_owned());
    let target = args.next().unwrap_or_else(|| "en".to_owned());
    let endpoint = args
        .next()
        .unwrap_or_else(|| "http://fedora.local:9931".to_owned());
    if args.next().is_some() {
        anyhow::bail!("Usage: translate [text] [target] [endpoint]");
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let (sender, receiver) = async_channel::unbounded();
        let request = api::TranslationRequest {
            text,
            source: None,
            target,
            max_tokens: 512,
        };
        let translate = async {
            let result = api::translate(&endpoint, request, sender.clone()).await;
            if let Err(error) = &result {
                let _ = sender
                    .send(api::TranslationEvent::Failed(format!("{error:#}")))
                    .await;
            }
            result
        };
        let display = async {
            let mut stdout = io::stdout().lock();
            while let Ok(event) = receiver.recv().await {
                match event {
                    api::TranslationEvent::Delta(text) => {
                        write!(stdout, "{text}")?;
                        stdout.flush()?;
                    }
                    api::TranslationEvent::Finished { truncated } => {
                        if truncated {
                            eprintln!("\nWarning: translation was truncated at the token limit.");
                        }
                        return Ok(());
                    }
                    api::TranslationEvent::Failed(message) => anyhow::bail!("{message}"),
                }
            }
            Ok(())
        };
        let (result, display_result): (Result<()>, Result<()>) = tokio::join!(translate, display);
        result?;
        display_result
    })?;
    Ok(())
}
