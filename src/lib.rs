use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;

mod link_parser;
mod requester;

pub use link_parser::*;
pub use requester::*;

pub type Result<T> = eyre::Result<T>;
pub type AtomicSet = Arc<RwLock<HashSet<String>>>;
