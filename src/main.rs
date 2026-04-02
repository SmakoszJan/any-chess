#[tokio::main]
async fn main() {
    chess_server::serve().await.unwrap();
}
