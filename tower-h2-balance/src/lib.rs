#[macro_use]
extern crate futures;
extern crate http;
extern crate h2;
extern crate tower_balance;
extern crate tower_h2;

use futures::Poll;
use tower_balance::load::Measure;
use tower_h2::Body;

pub struct MeasureFirstData;

pub struct MeasureEos;

pub struct MeasureFirstDataBody<T, B> {
    instrument: Option<T>,
    body: B,
}

pub struct MeasureEosBody<T, B> {
    instrument: Option<T>,
    body: B,
}

impl<T, B> Measure<T, http::Response<B>> for MeasureFirstData
where
    T: Sync + Send + 'static,
    B: Body + 'static,
{
    type Measured = http::Response<MeasureFirstDataBody<T, B>>;

    fn measure(instrument: T, rsp: http::Response<B>) -> Self::Measured {
        let (parts, body) = rsp.into_parts();
        http::Response::from_parts(parts, MeasureFirstDataBody {
            instrument: Some(instrument),
            body,
        })
    }
}

impl<T, B> Measure<T, http::Response<B>> for MeasureEos
where
    T: Sync + Send + 'static,
    B: Body + 'static,
{
    type Measured = http::Response<MeasureEosBody<T, B>>;

    fn measure(instrument: T, rsp: http::Response<B>) -> Self::Measured {
        let (parts, body) = rsp.into_parts();
        http::Response::from_parts(parts, MeasureEosBody {
            instrument: Some(instrument),
            body,
        })
    }
}

impl<T, B: Body> Body for MeasureFirstDataBody<T, B> {
    type Data = B::Data;

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    /// Polls a stream of data.
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, h2::Error> {
        let data = try_ready!(self.body.poll_data());
        drop(self.instrument.take());
        Ok(data.into())
    }

    /// Returns possibly **one** `HeaderMap` for trailers.
    fn poll_trailers(&mut self) -> Poll<Option<http::HeaderMap>, h2::Error> {
        self.body.poll_trailers()
    }
}

impl<T, B: Body> Body for MeasureEosBody<T, B> {
    type Data = B::Data;

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    /// Polls a stream of data.
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, h2::Error> {
        let data = try_ready!(self.body.poll_data());
        if data.is_none() {
            drop(self.instrument.take());
        }
        Ok(data.into())
    }

    /// Returns possibly **one** `HeaderMap` for trailers.
    fn poll_trailers(&mut self) -> Poll<Option<http::HeaderMap>, h2::Error> {
        let trls = try_ready!(self.body.poll_trailers());
        drop(self.instrument.take());
        Ok(trls.into())
    }
}
