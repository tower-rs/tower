#![allow(warnings)]

use futures::prelude::*;
use std::{thread, time::Duration};
use tokio_executor::{SpawnError, TypedExecutor};
use tower_ready_cache::ReadyCache;
use tower_service::Service;
use tower_test::mock;

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::lazy;
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
