use std::time::Duration;

use crate::Result;
use flume::{Receiver, Sender};
use url::Url;

pub struct Requester {
    client: reqwest::Client,
    url_rx: Receiver<Url>,
    html_tx: Sender<String>,
    concurrency: usize,
}

impl Requester {
    pub fn new(
        url_rx: Receiver<Url>,
        html_tx: Sender<String>,
        concurrency: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            client: reqwest::Client::builder().timeout(timeout).build().unwrap(),
            url_rx,
            html_tx,
            concurrency,
        }
    }

    pub async fn run(&self) -> Result<()> {
        use futures::stream::StreamExt;

        log::info!("Requester: Max concurrent connections {}", self.concurrency);
        self.url_rx
            .stream()
            .for_each_concurrent(self.concurrency, |url| async move {
                println!("Visited URL: {}", url);
                let response = match self.client.get(url).send().await {
                    Ok(res) => match res.error_for_status() {
                        Ok(res) => res,
                        Err(e) => {
                            log::warn!("URL {:?} returned status {:?}", e.url(), e.status());
                            return;
                        }
                    },
                    Err(e) => {
                        log::warn!("URL {:?} returned status {:?}", e.url(), e.status());
                        return;
                    }
                };

                let html = match response.text().await {
                    Ok(html) => html,
                    Err(e) => {
                        log::warn!("Error decoding response text: {:?}", e);
                        return;
                    }
                };
                match self.html_tx.send_async(html).await {
                    Ok(_) => (),
                    Err(e) => log::warn!("Error sending html to channel: {:?}", e),
                }
            })
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod run {
        use super::*;
        use tokio::join;

        #[tokio::test]
        async fn basic() {
            let html = std::fs::read_to_string("tests/fixtures/community.monzo.com.html").unwrap();
            let url = Url::parse(&mockito::server_url()).unwrap();
            let _m = mockito::mock("GET", "/")
                .with_status(200)
                .with_header("content-type", "text/html; charset=UTF-8")
                .with_body(html.clone())
                .create();

            let (url_tx, url_rx) = flume::unbounded();
            let (html_tx, html_rx) = flume::unbounded();

            url_tx.send(url).unwrap();
            drop(url_tx);

            let requester = Requester::new(url_rx, html_tx, 1, Duration::from_secs(1));
            let requester_handle = tokio::spawn(async move {
                if let Err(e) = requester.run().await {
                    log::error!("{}", e);
                }
            });
            let mut htmls = Vec::new();
            loop {
                if htmls.len() == 1 {
                    break;
                }
                htmls.push(html_rx.recv_async().await.unwrap());
            }

            join!(requester_handle).0.unwrap();
            assert_eq!(htmls, vec![html]);
        }
    }
}
