use clap::Parser;
use color_eyre::eyre;
use crawl::{AtomicSet, LinkParser, Requester};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::{sync::Mutex, try_join};
use url::Url;

/// Given a starting URL, visits each URL with the same subdomain and prints each URL visited as well as a list of links found on each page
#[derive(Parser, Debug)]
#[clap(name = "crawl", author, version, about, long_about = None)]
pub struct Args {
    #[clap(value_parser)]
    url: Url,

    /// Concurrent http request limit
    #[clap(short, long, default_value = "6")]
    concurrency: usize,

    /// http request timeout in seconds
    #[clap(short, long, default_value = "5")]
    timeout: u64,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();

    let args = Args::parse();
    let url = args.url;
    let timeout = Duration::from_secs(args.timeout);
    let concurrency = args.concurrency;

    let original_subdomain = match url.domain() {
        Some(d) => d.to_owned(),
        None => eyre::bail!("URL should have a valid DNS subdomain"),
    };

    let mut seen = HashSet::new();
    seen.insert(url.path().trim_end_matches('/').to_owned());
    let seen: AtomicSet = Arc::new(Mutex::new(seen));

    let (url_tx, url_rx) = flume::unbounded();
    let (html_tx, html_rx) = flume::unbounded();

    url_tx.send_async(url.clone()).await?;

    let requester_handle = tokio::spawn(async move {
        let requester = Requester::new(url_rx, html_tx, concurrency, timeout);
        if let Err(e) = requester.run().await {
            log::error!("{}", e);
        }
    });

    let link_parser_handle = tokio::spawn(async move {
        let link_parser = LinkParser::new(
            &original_subdomain,
            Arc::clone(&seen),
            html_rx,
            url_tx,
            timeout,
        );
        if let Err(e) = link_parser.run().await {
            log::error!("{}", e);
        }
    });

    try_join!(requester_handle, link_parser_handle)?;
    Ok(())
}

#[cfg(test)]
mod tests {}
