slint::include_modules!();

use lg_core::{Tv, discover};
use serde_json::json;
use std::{path::{Path, PathBuf}, time::Duration};
use tokio::{sync::mpsc, time::timeout};

enum Cmd {
    Button(String),
    Request(String),
}

/// `dir`: carpeta privada donde se guarda `tv.txt` (ip y client-key).
pub fn run(dir: PathBuf) {
    let ui = MainWindow::new().unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let btn_tx = tx.clone();
    ui.on_button(move |n| { let _ = btn_tx.send(Cmd::Button(n.into())); });
    ui.on_request(move |u| { let _ = tx.send(Cmd::Request(u.into())); });
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
            .block_on(worker(dir, rx, weak));
    });
    ui.run().unwrap();
}

async fn worker(dir: PathBuf, mut rx: mpsc::UnboundedReceiver<Cmd>, ui: slint::Weak<MainWindow>) {
    let status = |s: &str| {
        let s = s.to_string();
        let _ = ui.upgrade_in_event_loop(move |u| u.set_status(s.into()));
    };
    let mut tv = connect(&dir, &status).await;
    status(if tv.is_some() { "Conectado" } else { "Sin conexion" });
    while let Some(cmd) = rx.recv().await {
        for _ in 0..2 {
            if tv.is_none() {
                tv = connect(&dir, &status).await;
            }
            let Some(t) = tv.as_mut() else { status("Sin conexion"); break };
            let sent = timeout(Duration::from_secs(3), async {
                match &cmd {
                    Cmd::Button(n) => t.button(n).await,
                    Cmd::Request(u) => t.request(u, json!({})).await.map(drop),
                }
            }).await;
            if matches!(sent, Ok(Ok(()))) {
                status("Conectado");
                break;
            }
            tv = None; // reintenta una vez con conexion nueva
        }
    }
}

/// IP+key guardados primero (rapido); si fallan, SSDP y pairing (el usuario acepta en la TV).
async fn connect(dir: &Path, status: &impl Fn(&str)) -> Option<Tv> {
    let file = dir.join("tv.txt");
    let saved = std::fs::read_to_string(&file).ok()
        .and_then(|s| { let mut l = s.lines(); Some((l.next()?.to_owned(), l.next()?.to_owned())) });
    if let Some((ip, key)) = &saved {
        if let Ok(Ok((tv, _))) = timeout(Duration::from_secs(3), Tv::connect(ip, Some(key))).await {
            return Some(tv);
        }
    }
    status("Buscando TV...");
    let ip = discover(Duration::from_secs(3)).await.ok()??.ip().to_string();
    status("Acepta el pedido en la TV");
    let key = saved.map(|s| s.1);
    let (tv, key) = timeout(Duration::from_secs(60), Tv::connect(&ip, key.as_deref())).await.ok()?.ok()?;
    let _ = std::fs::write(&file, format!("{ip}\n{key}"));
    Some(tv)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    let dir = app.internal_data_path().unwrap_or_default();
    slint::android::init(app).unwrap();
    run(dir);
}
