use std::convert::Infallible;

use tower::util::{option_layer, MapErrLayer};
use tower::{Layer, Service, ServiceExt};

// Regression test for #665: `option_layer` previously returned an
// `Either<L, Identity>` whose service required both branches to share an error
// type. When the optional layer changed the error type, the result did not
// implement `Service` at all. `option_layer` now unifies both branches' errors
// to `BoxError`.
#[tokio::test]
async fn option_layer_unifies_branch_errors() {
    let inner = tower::service_fn(|()| async { Ok::<_, Infallible>(()) });

    // The optional layer changes the error type (`Infallible` -> `String`),
    // which differs from the unlayered branch's error type (`Infallible`).
    let layer = option_layer(Some(MapErrLayer::new(|e: Infallible| -> String {
        match e {}
    })));

    let mut svc = layer.layer(inner);

    let response = svc.ready().await.unwrap().call(()).await;
    assert!(response.is_ok());
}
