use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match lg_core::discover(Duration::from_secs(3)).await {
        Ok(Some(addr)) => println!("TV: {}", addr.ip()),
        Ok(None) => println!("sin respuesta SSDP"),
        Err(e) => println!("error: {e}"),
    }
}
