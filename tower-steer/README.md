# tower-steer

This crate provides functionality to aid managing routing requests between Tower [`Service`]s.

## Example
```rust
type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;
struct MyService(u8);

impl Service<String> for MyService {
    type Response = ();
    type Error = StdError;
    type Future = Ready<Result<(), Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: String) -> Self::Future {
        println!("{}: {}", self.0, req);
        ready(Ok(()))
    }
}

#[tokio::main]
async fn main() {
    let mut s = Steer::new(
        vec![MyService(0), MyService(1)],
        // one service handles strings with uppercase first letters. the other handles the rest.
        |r: &String, _: &[_]| if r.chars().next().unwrap().is_uppercase() { 0 } else { 1 },
    );

    let reqs = vec!["A", "b", "C", "d"];
    let reqs: Vec<String> = reqs.into_iter().map(String::from).collect();
    for r in reqs {
        poll_fn(|cx| s.poll_ready(cx)).await.unwrap();
        s.call(r).await;
    }
}
```
