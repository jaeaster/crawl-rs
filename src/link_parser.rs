use std::time::Duration;

use crate::{AtomicSet, Result};
use flume::{Receiver, Sender};
use scraper::{Html, Selector};
use url::Url;

pub struct LinkParser<'a> {
    subdomain: &'a str,
    visited_paths: AtomicSet,
    html_rx: Receiver<String>,
    url_tx: Sender<Url>,
    timeout: Duration,
}

impl<'a> LinkParser<'a> {
    pub fn new(
        subdomain: &'a str,
        visited_paths: AtomicSet,
        html_rx: Receiver<String>,
        url_tx: Sender<Url>,
        timeout: Duration,
    ) -> Self {
        Self {
            subdomain,
            visited_paths,
            html_rx,
            url_tx,
            timeout,
        }
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            log::info!("Waiting to recv html to parse");
            let html = match self.html_rx.recv_timeout(self.timeout * 2) {
                Ok(url) => url,
                Err(_) => {
                    log::error!("Link Parser receiver timed out");
                    return Ok(());
                }
            };
            let urls = self.parse_urls(html);
            for url in urls.into_iter() {
                let path = url.path().to_owned();
                if self.visited_paths.read().await.contains(&path) {
                    continue;
                }
                self.visited_paths.write().await.insert(path);
                if self.url_tx.send_async(url).await.is_err() {
                    return Ok(());
                }
            }
        }
    }

    fn parse_urls(&self, html: String) -> Vec<Url> {
        let mut urls: Vec<Url> = Vec::new();
        let mut documents = Vec::new();
        let document = Html::parse_document(&html);

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
                    let href = if href.starts_with("/") || href.starts_with("#") {
                        format!("https://{}{}", self.subdomain, href)
                    } else {
                        href.to_owned()
                    };
                    match Url::parse(&href) {
                        Ok(url) => {
                            println!("Found URL: {}", url);
                            let subdomain = url.domain().unwrap();
                            if subdomain == self.subdomain {
                                urls.push(url);
                            }
                        }
                        Err(e) => {
                            println!("Found Error with href {} : {:?}", href, e);
                        }
                    }
                } else {
                    println!("Found <a> without href: {:?}", link.value());
                }
            }
        }
        urls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_urls {
        use super::*;

        #[test]
        fn community_monzo_com() {
            let mut count = 0;

            let html = std::fs::read_to_string("tests/fixtures/community.monzo.com.html").unwrap();
            let html = Html::parse_document(&html);
            let noscript_selector = Selector::parse("noscript").unwrap();
            let anchor_selector = Selector::parse("a").unwrap();
            for noscript in html.select(&noscript_selector) {
                let html = Html::parse_document(&noscript.text().collect::<Vec<_>>().join(""));
                for _ in html.select(&anchor_selector) {
                    count += 1;
                }
            }
            assert_eq!(count, 16);
        }
    }
}
