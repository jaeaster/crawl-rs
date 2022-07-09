use std::{collections::HashSet, sync::Arc};

use clap::Parser;
use color_eyre::eyre;
use flume::{Receiver, Sender};
use scraper::{Html, Selector};
use tokio::sync::RwLock;
use url::Url;

type Result<T> = eyre::Result<T>;

/// Given a starting URL, visits each URL with the same subdomain and prints each URL visited as well as a list of links found on each page
#[derive(Parser, Debug)]
#[clap(name = "crawl", author, version, about, long_about = None)]
pub struct Args {
    #[clap(value_parser)]
    url: Url,
}

type AtomicSet = Arc<RwLock<HashSet<String>>>;
struct Crawler<'a> {
    subdomain: &'a str,
    visited_paths: AtomicSet,
    client: reqwest::Client,
    rx: Receiver<Url>,
    tx: Sender<Url>,
}

impl<'a> Crawler<'a> {
    pub fn new(
        subdomain: &'a str,
        visited_paths: AtomicSet,
        rx: Receiver<Url>,
        tx: Sender<Url>,
    ) -> Self {
        Self {
            subdomain,
            visited_paths,
            client: reqwest::Client::new(),
            rx,
            tx,
        }
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            println!("Waiting to recv");
            let url = match self.rx.try_recv() {
                Ok(url) => url,
                Err(_) => return Ok(()),
            };

            println!("Visited URL: {}", url);
            let response = match self.client.get(url).send().await?.error_for_status() {
                Ok(res) => res,
                Err(e) => {
                    log::error!("URL {:?} returned status {:?}", e.url(), e.status());
                    continue;
                }
            };

            let urls = parse_urls(response.text().await?, self.subdomain);
            for url in urls {
                let path = url.path().to_owned();
                if self.visited_paths.read().await.contains(&path) {
                    continue;
                }
                self.visited_paths.write().await.insert(path);
                if self.tx.send_async(url.clone()).await.is_err() {
                    return Ok(());
                }
            }
        }
    }
}

fn parse_urls(response_body: String, original_subdomain: &str) -> Vec<Url> {
    let mut urls: Vec<Url> = Vec::new();
    let mut documents = Vec::new();
    let document = Html::parse_document(&response_body);

    let noscript_selector = Selector::parse("noscript").unwrap();
    for noscript in document.select(&noscript_selector) {
        let document = Html::parse_document(&noscript.text().collect::<Vec<_>>().join(""));
        documents.push(document);
    }
    documents.push(document);

    let anchor_selector = Selector::parse("a").unwrap();
    for document in documents {
        for link in document.select(&anchor_selector) {
            if let Some(href) = link.value().attr("href") {
                let href = if href.starts_with("/") {
                    format!("https://{original_subdomain}{href}")
                } else {
                    href.to_owned()
                };
                match Url::parse(&href) {
                    Ok(url) => {
                        println!("Found URL: {}", url);
                        let subdomain = url.domain().unwrap();
                        if subdomain == original_subdomain {
                            urls.push(url);
                        }
                    }
                    Err(e) => {
                        println!("Found Error: {:?}", e);
                    }
                }
            } else {
                println!("Found <a> without href: {:?}", link.value());
            }
        }
    }
    urls
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();

    let args = Args::parse();
    let url = args.url;
    let original_subdomain = match url.domain() {
        Some(d) => d.to_owned(),
        None => eyre::bail!("URL should have a valid DNS subdomain"),
    };

    let mut seen = HashSet::new();
    seen.insert(url.path().to_owned());
    let seen = Arc::new(RwLock::new(seen));

    let (tx, rx) = flume::unbounded();

    tx.send_async(url.clone()).await?;

    let original_subdomain = Arc::new(original_subdomain);
    let handle = tokio::spawn(async move {
        let crawler = Crawler::new(
            &original_subdomain,
            Arc::clone(&seen),
            rx.clone(),
            tx.clone(),
        );
        if let Err(e) = crawler.run().await {
            log::error!("{}", e);
        }
        drop(tx);
        drop(rx);
    });
    handle.await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use scraper::{Html, Selector};

    #[test]
    fn community_monzo_com() {
        let mut count = 0;

        let html = std::fs::read_to_string("tests/fixtures/community.monzo.com.html").unwrap();
        let html = Html::parse_document(&html);
        let noscript_selector = Selector::parse("noscript").unwrap();
        for noscript in html.select(&noscript_selector) {
            let html = Html::parse_document(&noscript.text().collect::<Vec<_>>().join(""));
            for _ in html.select(&anchor_selector) {
                count += 1;
            }
        }
        let anchor_selector = Selector::parse("a").unwrap();
        assert_eq!(count, 16);
    }

    #[test]
    fn community_monzo_com_select() {
        use select::document::Document;
        use select::predicate::Name;

        let mut count = 0;
        let html = std::fs::read_to_string("tests/fixtures/community.monzo.com.html").unwrap();
        let document = Document::from(html.as_str());
        for noscript in document.find(Name("noscript")) {
            let document = Document::from(noscript.text().as_str());
            for _ in document.find(Name("a")) {
                count += 1;
            }
        }

        assert_eq!(count, 16);
    }
}
