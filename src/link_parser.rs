use std::time::Duration;

use crate::{AtomicSet, Result};
use flume::{Receiver, Sender};
use scraper::{Html, Selector};
use tokio::time::timeout;
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

    /// Concurrently parses urls from html strings received via a channel and sends resultant urls on a separate channel
    pub async fn run(&self) -> Result<()> {
        loop {
            log::info!("Link Parser: Waiting to recv html");
            let html = match timeout(self.timeout, self.html_rx.recv_async()).await {
                Ok(recv) => match recv {
                    Ok(url) => url,
                    Err(_) => {
                        log::info!("Link Parser: Html channel dropped");
                        return Ok(());
                    }
                },
                Err(_) => {
                    log::info!("Link Parser: recv timed out");
                    return Ok(());
                }
            };
            let urls = parse_urls(&html, self.subdomain);
            for url in urls.into_iter() {
                if url.path().ends_with(".pdf") || url.path().ends_with(".mp3") {
                    continue;
                }
                let path = url.path().trim_end_matches('/').to_owned();
                let mut visited_paths = self.visited_paths.lock().await;
                if visited_paths.contains(&path) {
                    drop(visited_paths);
                    continue;
                }
                visited_paths.insert(path);
                if self.url_tx.send_async(url).await.is_err() {
                    return Ok(());
                }
                drop(visited_paths);
            }
        }
    }
}

/// Parses Html string into Vector of Urls based on nested anchor tags
fn parse_urls(html: &str, original_subdomain: &str) -> Vec<Url> {
    let mut urls: Vec<Url> = Vec::new();
    let documents = parse_html(html);

    let anchor_selector = Selector::parse("a").unwrap();
    for document in documents {
        for link in document.select(&anchor_selector) {
            if let Some(href) = link.value().attr("href") {
                let href = normalize_href(href, original_subdomain);
                match Url::parse(&href) {
                    Ok(url) => {
                        println!("Found URL: {}", url);
                        if let Some(subdomain) = url.domain() {
                            if subdomain == original_subdomain {
                                urls.push(url);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Link Parser: Found href that is not a valid Url {} : {:?}",
                            href,
                            e
                        );
                    }
                }
            } else {
                log::warn!("Link Parser: Found <a> without href: {:?}", link.value());
            }
        }
    }
    urls
}

/// Parses Html string with nested `noscript` tags into Vector of Html documents
fn parse_html(html: &str) -> Vec<Html> {
    let mut documents = Vec::new();
    let document = Html::parse_document(html);
    let noscript_selector = Selector::parse("noscript").unwrap();
    for noscript in document.select(&noscript_selector) {
        let document = Html::parse_document(&noscript.text().collect::<Vec<_>>().join(""));
        documents.push(document);
    }
    documents.push(document);
    documents
}

/// Prepends `subdomain` if `href` is a relative path and removes hash mark fragments
fn normalize_href(href: &str, subdomain: &str) -> String {
    if href == "" || href == "/" || href == "#" || href == "/#" {
        return format!("https://{subdomain}");
    }

    let href = href.split("#").next().unwrap();

    if href.starts_with("/") {
        format!("https://{}{}", subdomain, href)
    } else {
        href.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod run {
        use std::{collections::HashSet, sync::Arc};

        use tokio::{join, sync::Mutex};

        use super::*;

        #[tokio::test]
        async fn basic() {
            let url = Url::parse("https://community.monzo.com").unwrap();
            let original_subdomain = "community.monzo.com";

            let mut seen = HashSet::new();
            seen.insert(url.path().to_owned());

            let seen: AtomicSet = Arc::new(Mutex::new(seen));

            let (url_tx, url_rx) = flume::unbounded();
            let (html_tx, html_rx) = flume::unbounded();

            let html = std::fs::read_to_string("tests/fixtures/community.monzo.com.html").unwrap();
            html_tx.send(html).unwrap();
            drop(html_tx);

            let link_parser = LinkParser::new(
                &original_subdomain,
                Arc::clone(&seen),
                html_rx,
                url_tx,
                Duration::from_secs(0),
            );
            let link_parser_handle = tokio::spawn(async move {
                link_parser.run().await.unwrap();
            });

            let mut urls = Vec::new();
            loop {
                if urls.len() == 14 {
                    break;
                }
                urls.push(url_rx.recv_async().await.unwrap());
            }

            join!(link_parser_handle).0.unwrap();
            assert_eq!(
                vec![
                    "https://community.monzo.com/",
                    "https://community.monzo.com/c/monzo/5",
                    "https://community.monzo.com/c/customersupport/10",
                    "https://community.monzo.com/c/feedback/35",
                    "https://community.monzo.com/c/usa/46",
                    "https://community.monzo.com/c/developers/43",
                    "https://community.monzo.com/c/community/19",
                    "https://community.monzo.com/c/making-monzo/39",
                    "https://community.monzo.com/c/foyer/24",
                    "https://community.monzo.com/c/financial-chat/36",
                    "https://community.monzo.com/categories",
                    "https://community.monzo.com/guidelines",
                    "https://community.monzo.com/tos",
                    "https://community.monzo.com/privacy"
                ],
                urls.iter().map(|url| url.as_str()).collect::<Vec<_>>()
            );
        }
    }

    mod parse_urls {
        use super::*;

        #[test]
        fn basic() {
            let html = "<html><body>\
                <a href='/'></a>\
                <a href='#'></a>\
                <a href='/#'></a>\
                <a href='https://monzo.com'></a>\
                <a href='https://monzo.com/blog'></a>\
                <a href='https://monzo.com/blog/5#cool-topic'></a>\
                <a href='https://community.monzo.com/is-not-valid'></a>\
                <a href='corrupted-href'></a>\
                <a></a>\
                <a href='mailto:usa-help@monzo.com'></a>\
                <a href='tel:+448008021281'></a>\
                <a href='../blog/2017/03/10/transparent-by-default/'></a>\
                </body></html>";
            let urls = parse_urls(html, "monzo.com");
            assert_eq!(6, urls.len())
        }

        #[test]
        fn community_monzo_com() {
            let html = std::fs::read_to_string("tests/fixtures/community.monzo.com.html").unwrap();
            let urls = parse_urls(&html, "community.monzo.com");
            assert_eq!(15, urls.len())
        }
    }

    mod parse_html {
        use super::*;

        #[test]
        fn basic() {
            let html = "<html><body></body></html>";
            assert_eq!(1, parse_html(html).len());
        }

        #[test]
        fn noscript() {
            let html = "<html><body><noscript></noscript><noscript></noscript></body></html>";
            assert_eq!(3, parse_html(html).len());
        }
    }

    mod normalize_href {
        use super::*;

        #[test]
        fn handles_absolute() {
            assert_eq!(
                "https://example.com",
                normalize_href("https://example.com", "example.com")
            );
        }

        #[test]
        fn handles_relative() {
            assert_eq!(
                "https://example.com/rofl",
                normalize_href("/rofl", "example.com")
            );
            assert_eq!(
                "https://example.com/rofl/copter",
                normalize_href("/rofl/copter", "example.com")
            );
        }

        #[test]
        fn handles_bespoke() {
            assert_eq!("https://example.com", normalize_href("/", "example.com"));
            assert_eq!("https://example.com", normalize_href("", "example.com"));
            assert_eq!("https://example.com", normalize_href("#", "example.com"));
            assert_eq!("https://example.com", normalize_href("/#", "example.com"));
        }

        #[test]
        fn handles_hash() {
            assert_eq!(
                "https://example.com/lol",
                normalize_href("/lol#wut", "example.com")
            );

            assert_eq!(
                "https://example.com/",
                normalize_href("https://example.com/#", "example.com")
            );
            assert_eq!(
                "https://example.com",
                normalize_href("https://example.com#", "example.com")
            );
            assert_eq!(
                "https://example.com/lol",
                normalize_href("https://example.com/lol#wut", "example.com")
            );
        }
    }
}
