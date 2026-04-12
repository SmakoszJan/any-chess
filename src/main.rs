use dotenv::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();
    chess_server::serve().await.unwrap();
}
