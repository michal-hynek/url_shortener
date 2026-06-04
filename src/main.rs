use axum::{Router, routing::post};
use tokio::net::TcpListener;

mod api;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/link", post(crate::api::create_link));

    let addr = "0.0.0.0:8000";
    let listener = TcpListener::bind(addr).await.unwrap();

    println!("Listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
