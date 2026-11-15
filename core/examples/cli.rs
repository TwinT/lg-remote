// uso: cli discover | cli <ip> request <uri> | button <NOMBRE> | launch <appId> | input <inputId>
// El client-key se guarda en ~/.lg-remote-key
use lg_core::{Result, Tv, discover};
use serde_json::json;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.first().map(String::as_str) == Some("discover") {
        match discover(Duration::from_secs(3)).await? {
            Some(addr) => println!("TV: {}", addr.ip()),
            None => println!("sin respuesta SSDP"),
        }
        return Ok(());
    }
    let (ip, cmd, arg) = (&a[0], &a[1], &a[2]);
    let path = format!("{}/.lg-remote-key", std::env::var("HOME")?);
    let saved = std::fs::read_to_string(&path).ok();
    let t = std::time::Instant::now();
    let (mut tv, key) = Tv::connect(ip, saved.as_deref().map(str::trim)).await?;
    println!("conectado en {:?}", t.elapsed());
    if saved.as_deref() != Some(&key) {
        std::fs::write(&path, &key)?;
    }
    match cmd.as_str() {
        "request" => println!("{}", tv.request(arg, json!({})).await?),
        "button" => tv.button(arg).await?,
        "launch" => tv.launch(arg).await?,
        "input" => tv.switch_input(arg).await?,
        _ => eprintln!("comando desconocido"),
    }
    println!("listo en {:?}", t.elapsed());
    Ok(())
}
