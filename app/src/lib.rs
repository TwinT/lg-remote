slint::include_modules!();

use lg_core::{Tv, discover, wake};
use serde_json::json;
use std::{path::{Path, PathBuf}, time::Duration};
use tokio::{sync::mpsc, time::{sleep, timeout}};

enum Cmd {
    Button(String),
    Request(String),
    Power,
}

/// Lo que se recuerda de la tele: `tv.txt` = ip, key y MACs (separadas por coma), una por línea.
#[derive(Clone)]
struct Saved {
    ip: String,
    key: String,
    macs: Vec<String>,
}

fn load(dir: &Path) -> Option<Saved> {
    let s = std::fs::read_to_string(dir.join("tv.txt")).ok()?;
    let mut l = s.lines();
    let (ip, key) = (l.next()?.to_owned(), l.next()?.to_owned());
    let macs = l.next().unwrap_or("").split(',').filter(|m| !m.is_empty()).map(str::to_owned).collect();
    Some(Saved { ip, key, macs })
}

fn save(dir: &Path, s: &Saved) {
    let _ = std::fs::write(dir.join("tv.txt"), format!("{}\n{}\n{}", s.ip, s.key, s.macs.join(",")));
}

/// `dir`: carpeta privada donde se guarda `tv.txt`.
pub fn run(dir: PathBuf) {
    let ui = MainWindow::new().unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let (t1, t2, t3) = (tx.clone(), tx.clone(), tx);
    ui.on_button(move |n| { let _ = t1.send(Cmd::Button(n.into())); });
    ui.on_request(move |u| { let _ = t2.send(Cmd::Request(u.into())); });
    ui.on_power(move || { let _ = t3.send(Cmd::Power); });
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
        if matches!(cmd, Cmd::Power) {
            power(&dir, &mut tv, &status).await;
            continue;
        }
        for _ in 0..2 {
            if tv.is_none() {
                tv = connect(&dir, &status).await;
            }
            let Some(t) = tv.as_mut() else { status("Sin conexion"); break };
            let sent = timeout(Duration::from_secs(3), async {
                match &cmd {
                    Cmd::Button(n) => t.button(n).await,
                    Cmd::Request(u) => t.request(u, json!({})).await.map(drop),
                    Cmd::Power => Ok(()),
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

/// Conectada => apagar. Si no responde (apagada) => Wake-on-LAN y esperar a que levante.
async fn power(dir: &Path, tv: &mut Option<Tv>, status: &impl Fn(&str)) {
    if let Some(t) = tv.as_mut() {
        let off = timeout(Duration::from_secs(3), t.request("ssap://system/turnOff", json!({}))).await;
        if matches!(off, Ok(Ok(_))) {
            *tv = None;
            status("Apagada");
            return;
        }
    }
    *tv = None;
    let Some(s) = load(dir) else { status("Sin TV guardada: abri la app con la TV prendida"); return };
    if s.macs.is_empty() {
        status("Sin MAC guardada: conecta con la TV prendida una vez");
        return;
    }
    status("Encendiendo TV...");
    // ponytail: ~40 s de espera fija; la TV tarda 10-25 s en abrir el puerto
    for i in 0..16 {
        if i % 4 == 0 {
            let _ = wake(&s.macs, &s.ip).await;
        }
        sleep(Duration::from_secs(1)).await;
        if let Some(t) = connect_saved(dir).await {
            *tv = Some(t);
            status("Conectado");
            return;
        }
    }
    status("La TV no respondio (revisa 'Encender con Wi-Fi')");
}

/// Reconexión rápida con IP+key guardados. Aprovecha para guardar las MAC si faltaban.
async fn connect_saved(dir: &Path) -> Option<Tv> {
    let s = load(dir)?;
    let (mut tv, _) = timeout(Duration::from_secs(3), Tv::connect(&s.ip, Some(&s.key))).await.ok()?.ok()?;
    if s.macs.is_empty() {
        if let Ok(Ok(macs)) = timeout(Duration::from_secs(3), tv.macs()).await {
            save(dir, &Saved { macs, ..s });
        }
    }
    Some(tv)
}

/// Guardados primero (rápido); si fallan, SSDP y pairing (el usuario acepta en la TV).
async fn connect(dir: &Path, status: &impl Fn(&str)) -> Option<Tv> {
    if let Some(tv) = connect_saved(dir).await {
        return Some(tv);
    }
    status("Buscando TV...");
    let ip = discover(Duration::from_secs(3)).await.ok()??.ip().to_string();
    status("Acepta el pedido en la TV");
    let key = load(dir).map(|s| s.key);
    let (mut tv, key) = timeout(Duration::from_secs(60), Tv::connect(&ip, key.as_deref())).await.ok()?.ok()?;
    let macs = timeout(Duration::from_secs(3), tv.macs()).await.ok().and_then(Result::ok).unwrap_or_default();
    save(dir, &Saved { ip, key, macs });
    Some(tv)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    let dir = app.internal_data_path().unwrap_or_default();
    slint::android::init(app).unwrap();
    run(dir);
}
