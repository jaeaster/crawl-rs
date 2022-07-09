use std::time::Duration;

use crate::Result;
use flume::{Receiver, Sender};
use url::Url;

pub struct Requester {
    client: reqwest::Client,
    url_rx: Receiver<Url>,
    html_tx: Sender<String>,
}

impl Requester {
    pub fn new(url_rx: Receiver<Url>, html_tx: Sender<String>, timeout: Duration) -> Self {
        Self {
            client: reqwest::Client::builder().timeout(timeout).build().unwrap(),
            url_rx,
            html_tx,
        }
    }

    pub async fn run(&self) -> Result<()> {
        use futures::stream::StreamExt;

        self.url_rx
            .stream()
            .for_each_concurrent(6, |url| async move {
                println!("Visited URL: {}", url);
                let response = match self.client.get(url).send().await {
                    Ok(res) => match res.error_for_status() {
                        Ok(res) => res,
                        Err(e) => {
                            log::error!("URL {:?} returned status {:?}", e.url(), e.status());
                            return;
                        }
                    },
                    Err(e) => {
                        log::error!("URL {:?} returned status {:?}", e.url(), e.status());
                        return;
                    }
                };

                let html = match response.text().await {
                    Ok(html) => html,
                    Err(e) => {
                        log::error!("Error decoding response text: {:?}", e);
                        return;
                    }
                };
                match self.html_tx.send_async(html).await {
                    Ok(_) => (),
                    Err(e) => log::error!("Error sending html to channel: {:?}", e),
                }
            })
            .await;
        Ok(())
    }
}
