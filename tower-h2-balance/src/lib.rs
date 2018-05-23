#[macro_use]
extern crate futures;
extern crate http;
extern crate h2;
extern crate tower_balance;
extern crate tower_h2;
//extern crate tower_service;

use futures::Poll;
use tower_balance::load::Track;
use tower_h2::Body;
//use tower_service::Service;

pub struct TrackFirstData;

pub struct TrackTrailers;

pub struct TrackFirstDataBody<T, B> {
    tracker: Option<T>,
    body: B,
}

pub struct TrackTrailersBody<T, B> {
    tracker: Option<T>,
    body: B,
}

impl<T, B> Track<T, http::Response<B>> for TrackFirstData
where
    T: Sync + Send + 'static,
    B: Body + 'static,
{
    type Tracked = http::Response<TrackFirstDataBody<T, B>>;

    fn track(tracker: T, rsp: http::Response<B>) -> Self::Tracked {
        let (parts, body) = rsp.into_parts();
        http::Response::from_parts(parts, TrackFirstDataBody {
            tracker: Some(tracker),
            body,
        })
    }
}

impl<T, B> Track<T, http::Response<B>> for TrackTrailers
where
    T: Sync + Send + 'static,
    B: Body + 'static,
{
    type Tracked = http::Response<TrackTrailersBody<T, B>>;

    fn track(tracker: T, rsp: http::Response<B>) -> Self::Tracked {
        let (parts, body) = rsp.into_parts();
        http::Response::from_parts(parts, TrackTrailersBody {
            tracker: Some(tracker),
            body,
        })
    }
}

impl<T, B: Body> Body for TrackFirstDataBody<T, B> {
    type Data = B::Data;

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    /// Polls a stream of data.
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, h2::Error> {
        let data = try_ready!(self.body.poll_data());
        drop(self.tracker.take());
        Ok(data.into())
    }

    /// Returns possibly **one** `HeaderMap` for trailers.
    fn poll_trailers(&mut self) -> Poll<Option<http::HeaderMap>, h2::Error> {
        self.body.poll_trailers()
    }
}

impl<T, B: Body> Body for TrackTrailersBody<T, B> {
    type Data = B::Data;

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    /// Polls a stream of data.
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, h2::Error> {
        let data = try_ready!(self.body.poll_data());
        if data.is_none() {
            drop(self.tracker.take());
        }
        Ok(data.into())
    }

    /// Returns possibly **one** `HeaderMap` for trailers.
    fn poll_trailers(&mut self) -> Poll<Option<http::HeaderMap>, h2::Error> {
        let trls = try_ready!(self.body.poll_trailers());
        drop(self.tracker.take());
        Ok(trls.into())
    }
}
