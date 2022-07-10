# crawl

## Overview

A blazingly fast web crawler that leverages async multi-producer, multi-consumer channels

## Notes

crawl uses the popular [tokio library](https://github.com/tokio-rs/tokio) as an async executor.

The structure of the program consists of two async tasks and two channels to pass data between the tasks.

The `Requester`:
  * reads urls from a channel
  * requests html from the url
  * sends the html into the html channel

You can control the amount of concurrent requests with the `--concurrency` option.

The `LinkParser`:
  * reads html from a channel
  * parses the html for valid links on the same subdomain that have not been requested already
  * sends the links into the url channel

Because these two tasks depend on each other via the two channels, the process would deadlock once there are no more urls and html to process.

To get around this issue, there is a `--timeout` option. The timeout parameter controls both the http request timeout AND the html channel read timeout.

## Installation

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
rustup-init

# Run the tests
cargo test

# Build the executable
cargo build --release

./target/release/crawl --help
```

## Usage

```bash
crawl 0.1.0
Given a starting URL, visits each URL with the same subdomain and prints each URL visited as well as a list of links found on each page

USAGE:
    crawl [OPTIONS] <URL>

ARGS:
    <URL>

OPTIONS:
    -c, --concurrency <CONCURRENCY>    Concurrent http request limit [default: 6]
    -h, --help                         Print help information
    -t, --timeout <TIMEOUT>            http request timeout in seconds [default: 5]
    -V, --version                      Print version information
```
## Examples

```bash
crawl https://example.com
```

```bash
crawl https://monzo.com --concurrency 1 --timeout 5
```
