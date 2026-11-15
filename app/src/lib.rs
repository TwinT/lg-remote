slint::include_modules!();

use lg_core::{Item, Tv, discover, http_get, wake};
use serde_json::json;
use slint::{ModelRc, VecModel, Weak};
use std::{path::{Path, PathBuf}, time::Duration};
use tokio::{sync::mpsc, time::{sleep, timeout}};

// ponytail: sensibilidad fija del cursor; hacerla configurable si hace falta
const CURSOR_GAIN: i32 = 3;

enum Cmd {
    Button(String),
    Request(String),
    Launch(String),
    Input(String),
    Move(i32, i32),
    Scroll(i32),
    Click,
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

/// `dir`: carpeta privada donde se guardan `tv.txt` e `icons/`.
pub fn run(dir: PathBuf) {
    let ui = MainWindow::new().unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    macro_rules! on {
        ($setter:ident, |$($a:ident),*| $cmd:expr) => {{
            let tx = tx.clone();
            ui.$setter(move |$($a),*| { let _ = tx.send($cmd); });
        }};
    }
    on!(on_button, |n| Cmd::Button(n.into()));
    on!(on_request, |u| Cmd::Request(u.into()));
    on!(on_launch, |id| Cmd::Launch(id.into()));
    on!(on_input, |id| Cmd::Input(id.into()));
    on!(on_ptr_move, |dx, dy| Cmd::Move(dx * CURSOR_GAIN, dy * CURSOR_GAIN));
    on!(on_ptr_scroll, |dy| Cmd::Scroll(dy));
    on!(on_ptr_click, | | Cmd::Click);
    on!(on_power, | | Cmd::Power);
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
            .block_on(worker(dir, rx, weak));
    });
    ui.run().unwrap();
}

async fn worker(dir: PathBuf, mut rx: mpsc::UnboundedReceiver<Cmd>, ui: Weak<MainWindow>) {
    let status = |s: &str| {
        let s = s.to_string();
        let _ = ui.upgrade_in_event_loop(move |u| u.set_status(s.into()));
    };
    show_cached_lists(&dir, &ui); // entradas/apps ya vistas, antes de conectar
    let mut tv = connect(&dir, &status, &ui).await;
    status(if tv.is_some() { "Conectado" } else { "Sin conexion" });
    let mut backlog: Option<Cmd> = None;
    loop {
        let cmd = match backlog.take() {
            Some(c) => c,
            None => match rx.recv().await { Some(c) => c, None => break },
        };
        // El arrastre del touchpad genera muchos Move: se suman los que ya esperan en cola.
        let cmd = if let Cmd::Move(mut dx, mut dy) = cmd {
            while let Ok(next) = rx.try_recv() {
                match next {
                    Cmd::Move(a, b) => { dx += a; dy += b; }
                    other => { backlog = Some(other); break; }
                }
            }
            Cmd::Move(dx, dy)
        } else {
            cmd
        };
        if matches!(cmd, Cmd::Power) {
            power(&dir, &mut tv, &status, &ui).await;
            continue;
        }
        for _ in 0..2 {
            if tv.is_none() {
                tv = connect(&dir, &status, &ui).await;
            }
            let Some(t) = tv.as_mut() else { status("Sin conexion"); break };
            if matches!(timeout(Duration::from_secs(3), exec(t, &cmd)).await, Ok(Ok(()))) {
                status("Conectado");
                break;
            }
            tv = None; // reintenta una vez con conexion nueva
        }
    }
}

async fn exec(t: &mut Tv, cmd: &Cmd) -> lg_core::Result<()> {
    match cmd {
        Cmd::Button(n) => t.button(n).await,
        Cmd::Request(u) => t.request(u, json!({})).await.map(drop),
        Cmd::Launch(id) => t.launch(id).await,
        Cmd::Input(id) => t.switch_input(id).await,
        Cmd::Move(dx, dy) => t.move_cursor(*dx, *dy).await,
        Cmd::Scroll(dy) => t.scroll(*dy).await,
        Cmd::Click => t.click().await,
        Cmd::Power => Ok(()),
    }
}

/// Conectada => apagar. Si no responde (apagada) => Wake-on-LAN y esperar a que levante.
async fn power(dir: &Path, tv: &mut Option<Tv>, status: &impl Fn(&str), ui: &Weak<MainWindow>) {
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
    // ponytail: ~16 s de espera fija; la TV tarda 10-25 s en abrir el puerto
    for i in 0..16 {
        if i % 4 == 0 {
            let _ = wake(&s.macs, &s.ip).await;
        }
        sleep(Duration::from_secs(1)).await;
        if let Some(mut t) = connect_saved(dir).await {
            refresh_lists(&mut t, dir, ui).await;
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
async fn connect(dir: &Path, status: &impl Fn(&str), ui: &Weak<MainWindow>) -> Option<Tv> {
    let mut tv = match connect_saved(dir).await {
        Some(tv) => tv,
        None => {
            status("Buscando TV...");
            let ip = discover(Duration::from_secs(3)).await.ok()??.ip().to_string();
            status("Acepta el pedido en la TV");
            let key = load(dir).map(|s| s.key);
            let (mut tv, key) = timeout(Duration::from_secs(60), Tv::connect(&ip, key.as_deref())).await.ok()?.ok()?;
            let macs = timeout(Duration::from_secs(3), tv.macs()).await.ok().and_then(Result::ok).unwrap_or_default();
            save(dir, &Saved { ip, key, macs });
            tv
        }
    };
    refresh_lists(&mut tv, dir, ui).await;
    Some(tv)
}

// ---- entradas y apps (con íconos cacheados en disco) ----

/// `<dir>/icons/<hash>_<archivo>` a partir de `.../resources/<hash>/<archivo>`.
fn icon_path(dir: &Path, url: &str) -> PathBuf {
    let mut parts = url.rsplit('/');
    let (file, hash) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    dir.join("icons").join(format!("{hash}_{file}"))
}

fn tiles(dir: &Path, items: &[Item]) -> Vec<(String, String, PathBuf)> {
    items.iter().map(|i| (i.id.clone(), i.title.clone(), icon_path(dir, &i.icon))).collect()
}

fn push_tiles(ui: &Weak<MainWindow>, inputs: Vec<(String, String, PathBuf)>, apps: Vec<(String, String, PathBuf)>) {
    let _ = ui.upgrade_in_event_loop(move |u| {
        let model = |v: Vec<(String, String, PathBuf)>| {
            ModelRc::new(VecModel::from(v.into_iter().map(|(id, title, path)| Tile {
                id: id.into(),
                title: title.into(),
                icon: decode_png(&path).unwrap_or_default(),
            }).collect::<Vec<_>>()))
        };
        u.set_inputs(model(inputs));
        u.set_apps(model(apps));
    });
}

/// PNG -> imagen de Slint. Solo PNG: es lo que sirve la tele; otro formato => sin ícono (se muestra el nombre).
fn decode_png(path: &Path) -> Option<slint::Image> {
    let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, info.width, info.height);
    Some(slint::Image::from_rgba8(buffer))
}

/// Listas de la sesión anterior, para que el panel de apps aparezca al instante.
fn show_cached_lists(dir: &Path, ui: &Weak<MainWindow>) {
    let read = |name: &str| -> Vec<(String, String, PathBuf)> {
        std::fs::read_to_string(dir.join(name)).unwrap_or_default().lines().filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let (id, title, icon) = (f.next()?, f.next()?, f.next()?);
            Some((id.to_owned(), title.to_owned(), icon_path(dir, icon)))
        }).collect()
    };
    push_tiles(ui, read("inputs.tsv"), read("apps.tsv"));
}

fn write_list(dir: &Path, name: &str, items: &[Item]) {
    let s: String = items.iter().map(|i| format!("{}\t{}\t{}\n", i.id, i.title, i.icon)).collect();
    let _ = std::fs::write(dir.join(name), s);
}

/// Pide entradas y apps a la tele, las muestra y baja en segundo plano los íconos que falten.
async fn refresh_lists(tv: &mut Tv, dir: &Path, ui: &Weak<MainWindow>) {
    let got = timeout(Duration::from_secs(4), async { (tv.inputs().await, tv.apps().await) }).await;
    let Ok((Ok(inputs), Ok(apps))) = got else { return };
    write_list(dir, "inputs.tsv", &inputs);
    write_list(dir, "apps.tsv", &apps);
    push_tiles(ui, tiles(dir, &inputs), tiles(dir, &apps));
    let (dir, ui) = (dir.to_owned(), ui.clone());
    tokio::spawn(async move {
        let _ = std::fs::create_dir_all(dir.join("icons"));
        let mut downloaded = false;
        for item in inputs.iter().chain(&apps).filter(|i| !i.icon.is_empty()) {
            let path = icon_path(&dir, &item.icon);
            if path.exists() {
                continue;
            }
            if let Ok(Ok(bytes)) = timeout(Duration::from_secs(5), http_get(&item.icon)).await {
                downloaded |= std::fs::write(&path, bytes).is_ok();
            }
        }
        if downloaded {
            push_tiles(&ui, tiles(&dir, &inputs), tiles(&dir, &apps));
        }
    });
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    let dir = app.internal_data_path().unwrap_or_default();
    slint::android::init(app).unwrap();
    run(dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_path_uses_hash_and_file() {
        let p = icon_path(Path::new("/d"), "http://tv:3000/resources/abc123/icon.png");
        assert_eq!(p, Path::new("/d/icons/abc123_icon.png"));
    }

    #[test]
    fn decodes_rgb_png() {
        let path = std::env::temp_dir().join("lg_remote_test.png");
        let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), 2, 1);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 255, 0]).unwrap();
        let img = decode_png(&path).unwrap();
        assert_eq!((img.size().width, img.size().height), (2, 1));
        assert!(decode_png(Path::new("/no/existe.png")).is_none());
    }
}
