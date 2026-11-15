use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PERMISSIONS: [&str; 13] = [
    "LAUNCH", "CONTROL_AUDIO", "CONTROL_POWER", "CONTROL_INPUT_TEXT",
    "CONTROL_MOUSE_AND_KEYBOARD", "CONTROL_INPUT_TV", "READ_TV_CHANNEL_LIST", "CONTROL_DISPLAY",
    "READ_NETWORK_STATE", "CONTROL_INPUT_MEDIA_PLAYBACK",
    "READ_INSTALLED_APPS", "READ_INPUT_DEVICE_LIST", "READ_RUNNING_APPS",
];

/// Entrada o app de la tele (con la URL de su ícono, servido por la propia tele).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub icon: String,
}

fn items(v: &Value, title_key: &str) -> Vec<Item> {
    v.as_array().into_iter().flatten().filter_map(|x| {
        Some(Item {
            id: x["id"].as_str()?.into(),
            title: x[title_key].as_str()?.into(),
            icon: x["icon"].as_str().unwrap_or("").into(),
        })
    }).collect()
}

pub struct Tv {
    ws: Ws,
    ptr: Option<Ws>,
    n: u32,
}

impl Tv {
    /// Conecta y registra. `key` = client-key guardado (None => la tele pide aceptar en pantalla).
    /// Devuelve el client-key para persistir.
    pub async fn connect(ip: &str, key: Option<&str>) -> Result<(Tv, String)> {
        // ponytail: ws plano en :3000; si firmware nuevo lo cierra, wss :3001 con rustls
        let (ws, _) = connect_async(format!("ws://{ip}:3000")).await?;
        let mut tv = Tv { ws, ptr: None, n: 0 };
        let mut payload = json!({
            "pairingType": "PROMPT",
            "manifest": {"manifestVersion": 1, "permissions": PERMISSIONS},
        });
        if let Some(k) = key {
            payload["client-key"] = k.into();
        }
        tv.send(&json!({"type": "register", "id": "register_0", "payload": payload})).await?;
        loop {
            let m = tv.recv().await?;
            match m["type"].as_str() {
                Some("registered") => {
                    let key = m["payload"]["client-key"].as_str().ok_or("registered sin client-key")?;
                    return Ok((tv, key.to_owned()));
                }
                Some("error") => return Err(format!("register: {}", m["error"]).into()),
                _ => {} // "response" con PROMPT: esperando que el usuario acepte en la tele
            }
        }
    }

    /// Request SSAP, p. ej. `ssap://audio/volumeUp`. Devuelve el payload.
    pub async fn request(&mut self, uri: &str, payload: Value) -> Result<Value> {
        self.n += 1;
        let id = format!("r{}", self.n);
        self.send(&json!({"type": "request", "id": id, "uri": uri, "payload": payload})).await?;
        loop {
            let m = self.recv().await?;
            if m["id"] == id {
                if m["type"] == "error" {
                    return Err(format!("{uri}: {}", m["error"]).into());
                }
                return Ok(m["payload"].clone());
            }
        }
    }

    /// MACs (wifi y cable) de la tele, para Wake-on-LAN.
    pub async fn macs(&mut self) -> Result<Vec<String>> {
        let p = self.request("ssap://com.webos.service.connectionmanager/getinfo", json!({})).await?;
        Ok(["wifiInfo", "wiredInfo"].iter()
            .filter_map(|k| p[*k]["macAddress"].as_str().map(str::to_owned))
            .collect())
    }

    /// Entradas externas (HDMI, AV...).
    pub async fn inputs(&mut self) -> Result<Vec<Item>> {
        let p = self.request("ssap://tv/getExternalInputList", json!({})).await?;
        Ok(items(&p["devices"], "label"))
    }

    /// Apps instaladas (sin las entradas HDMI, que van en `inputs`).
    pub async fn apps(&mut self) -> Result<Vec<Item>> {
        let p = self.request("ssap://com.webos.applicationManager/listLaunchPoints", json!({})).await?;
        let mut apps = items(&p["launchPoints"], "title");
        apps.retain(|a| !a.id.starts_with("com.webos.app.hdmi"));
        Ok(apps)
    }

    pub async fn launch(&mut self, app_id: &str) -> Result<()> {
        self.request("ssap://system.launcher/launch", json!({"id": app_id})).await.map(drop)
    }

    pub async fn switch_input(&mut self, input_id: &str) -> Result<()> {
        self.request("ssap://tv/switchInput", json!({"inputId": input_id})).await.map(drop)
    }

    /// Botón del control: UP DOWN LEFT RIGHT ENTER BACK HOME EXIT MUTE 0-9 RED GREEN YELLOW BLUE ...
    pub async fn button(&mut self, name: &str) -> Result<()> {
        self.pointer(format!("type:button\nname:{name}\n\n")).await
    }

    pub async fn move_cursor(&mut self, dx: i32, dy: i32) -> Result<()> {
        self.pointer(format!("type:move\ndx:{dx}\ndy:{dy}\ndown:0\n\n")).await
    }

    pub async fn click(&mut self) -> Result<()> {
        self.pointer("type:click\n\n".into()).await
    }

    pub async fn scroll(&mut self, dy: i32) -> Result<()> {
        self.pointer(format!("type:scroll\ndx:0\ndy:{dy}\n\n")).await
    }

    /// Mensaje por el socket del puntero (se abre la primera vez).
    async fn pointer(&mut self, msg: String) -> Result<()> {
        if self.ptr.is_none() {
            let p = self.request("ssap://com.webos.service.networkinput/getPointerInputSocket", json!({})).await?;
            let path = p["socketPath"].as_str().ok_or("sin socketPath")?;
            self.ptr = Some(connect_async(path).await?.0);
        }
        self.ptr.as_mut().unwrap().send(Message::text(msg)).await?;
        Ok(())
    }

    async fn send(&mut self, v: &Value) -> Result<()> {
        Ok(self.ws.send(Message::text(v.to_string())).await?)
    }

    async fn recv(&mut self) -> Result<Value> {
        loop {
            match self.ws.next().await.ok_or("conexión cerrada")?? {
                Message::Text(t) => return Ok(serde_json::from_str(&t)?),
                Message::Close(_) => return Err("conexión cerrada".into()),
                _ => {}
            }
        }
    }
}

/// GET HTTP/1.0 simple (la tele sirve los íconos sin TLS ni chunked). Devuelve el cuerpo.
pub async fn http_get(url: &str) -> Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let rest = url.strip_prefix("http://").ok_or("solo http://")?;
    let (host, path) = rest.split_once('/').map_or((rest, "/".to_owned()), |(h, p)| (h, format!("/{p}")));
    let mut s = TcpStream::connect(if host.contains(':') { host.to_owned() } else { format!("{host}:80") }).await?;
    s.write_all(format!("GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n").as_bytes()).await?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await?;
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or("respuesta HTTP inválida")?;
    if !buf.starts_with(b"HTTP/1.1 200") && !buf.starts_with(b"HTTP/1.0 200") {
        return Err(format!("HTTP: {}", String::from_utf8_lossy(&buf[..buf.len().min(20)])).into());
    }
    Ok(buf[end + 4..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_items() {
        let v = json!([
            {"id": "HDMI_1", "label": "HDMI1", "icon": "http://tv/a.png", "connected": false},
            {"label": "sin id"},
            {"id": "AV_1", "label": "AV"}
        ]);
        let got = items(&v, "label");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], Item { id: "HDMI_1".into(), title: "HDMI1".into(), icon: "http://tv/a.png".into() });
        assert_eq!(got[1].icon, "");
    }
}
