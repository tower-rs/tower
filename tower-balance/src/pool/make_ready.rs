#![allow(missing_docs)]
#![doc(hidden)]

use crate::error;
use futures::{future, try_ready, Future, Poll};
use tower_service::Service;
use tower_util::MakeService;

pub(super) enum MakeReady<MS, Tgt, Req>
where
    MS: MakeService<Tgt, Req>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    Make(MS::Future),
    Ready(MS::Service),
}

impl<MS, Tgt, Req> MakeReady<MS, Tgt, Req>
where
    MS: MakeService<Tgt, Req>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    pub fn from_future(fut: MS::Future) -> Self {
        MakeReady::Make(fut)
    }
}

impl<MS, Tgt, Req> Service<Req> for MakeReady<MS, Tgt, Req>
where
    MS: MakeService<Tgt, Req>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    type Response = MS::Response;
    type Error = error::Error;
    type Future =
        future::MapErr<<MS::Service as Service<Req>>::Future, fn(MS::Error) -> Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        loop {
            *self = match self {
                MakeReady::Make(ref mut fut) => {
                    let svc = try_ready!(fut.poll().map_err(Into::into));
                    MakeReady::Ready(svc)
                }
                MakeReady::Ready(ref mut svc) => {
                    return svc.poll_ready().map_err(Into::into);
                }
            };
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        match self {
            MakeReady::Make(_) => panic!("not ready"),
            MakeReady::Ready(ref mut svc) => {
                return svc.call(req).map_err(Into::into);
            }
        }
    }
}
