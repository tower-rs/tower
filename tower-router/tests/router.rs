extern crate futures;
extern crate futures_test;
extern crate tower_router;
extern crate tower_service;

use tower_router::*;
use tower_service::Service;

use futures::*;
use futures::future::FutureResult;
use futures_test::Harness;

use std::collections::HashMap;

macro_rules! assert_ready {
    ($service:expr) => {{
        let s = $service;
        let mut t = Harness::poll_fn(|| s.poll_ready());
        assert!(t.poll().unwrap().is_ready());
    }};
}

macro_rules! assert_not_ready {
    ($service:expr) => {{
        let s = $service;
        let mut t = Harness::poll_fn(|| s.poll_ready());
        assert!(!t.poll().unwrap().is_ready());
    }};
}

#[test]
fn basic_routing() {
    let mut recognize = MapRecognize::new();
    recognize.map.insert("one".into(), StringService::ok("hello"));
    recognize.map.insert("two".into(), StringService::ok("world"));

    let mut service = Router::new(recognize);

    // Router is ready by default
    assert_ready!(&mut service);

    let resp = service.call("one".into());

    assert_not_ready!(&mut service);
    assert_eq!(resp.wait().unwrap(), "hello");

    // Router ready again
    assert_ready!(&mut service);

    let resp = service.call("two".into());
    assert_eq!(resp.wait().unwrap(), "world");

    // Try invalid routing
    let resp = service.call("three".into());
    assert!(resp.wait().is_err());
}

#[test]
fn inner_service_err() {
    let mut recognize = MapRecognize::new();
    recognize.map.insert("one".into(), StringService::ok("hello"));
    recognize.map.insert("two".into(), StringService::err());

    let mut service = Router::new(recognize);

    let resp = service.call("two".into());
    assert!(resp.wait().is_err());

    assert_ready!(&mut service);

    let resp = service.call("one".into());
    assert_eq!(resp.wait().unwrap(), "hello");
}

#[test]
fn inner_service_not_ready() {
    let mut recognize = MapRecognize::new();
    recognize.map.insert("one".into(), MaybeService::new("hello"));
    recognize.map.insert("two".into(), MaybeService::none());

    let mut service = Router::new(recognize);

    let resp = service.call("two".into());
    let mut resp = Harness::new(resp);
    assert!(!resp.poll().unwrap().is_ready());

    assert_not_ready!(&mut service);

    let resp = service.call("one".into());
    assert!(resp.wait().is_err());
}

// ===== impl MapRecognize =====

#[derive(Debug)]
struct MapRecognize<T> {
    map: HashMap<String, T>,
}

impl<T> MapRecognize<T> {
    fn new() -> Self {
        MapRecognize { map: HashMap::new() }
    }
}

impl<T> Recognize<String> for MapRecognize<T>
where T: Service<String, Response=String, Error = ()> + 'static,
{
    type Response = String;
    type Error = ();
    type RouteError = ();
    type Service = T;

    fn recognize(&mut self, request: &String)
        -> Result<&mut Self::Service, Self::RouteError>
    {
        match self.map.get_mut(request) {
            Some(service) => Ok(service),
            None => Err(()),
        }
    }
}

// ===== impl services =====

#[derive(Debug)]
struct StringService {
    string: Result<String, ()>,
}

impl StringService {
    pub fn ok(string: &str) -> Self {
        StringService {
            string: Ok(string.into()),
        }
    }

    pub fn err() -> Self {
        StringService {
            string: Err(()),
        }
    }
}

impl Service<String> for StringService {
    type Response = String;
    type Error = ();
    type Future = FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    fn call(&mut self, _: String) -> Self::Future {
        future::result(self.string.clone())
    }
}

#[derive(Debug)]
struct MaybeService {
    string: Option<String>,
}

impl MaybeService {
    pub fn new(string: &str) -> Self {
        MaybeService {
            string: Some(string.into()),
        }
    }

    pub fn none() -> Self {
        MaybeService {
            string: None,
        }
    }
}

impl Service<String> for MaybeService {
    type Response = String;
    type Error = ();
    type Future = FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.string.is_some() {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }

    fn call(&mut self, _: String) -> Self::Future {
        match self.string.clone() {
            Some(string) => future::ok(string),
            None => future::err(()),
        }
    }
}
