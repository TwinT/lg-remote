use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PERMISSIONS: [&str; 10] = [
    "LAUNCH", "CONTROL_AUDIO", "CONTROL_POWER", "CONTROL_INPUT_TEXT",
    "CONTROL_MOUSE_AND_KEYBOARD", "CONTROL_INPUT_TV", "READ_TV_CHANNEL_LIST", "CONTROL_DISPLAY",
    "READ_NETWORK_STATE", "CONTROL_INPUT_MEDIA_PLAYBACK",
];

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

    /// Botón del control: UP DOWN LEFT RIGHT ENTER BACK HOME EXIT MUTE ...
    pub async fn button(&mut self, name: &str) -> Result<()> {
        if self.ptr.is_none() {
            let p = self.request("ssap://com.webos.service.networkinput/getPointerInputSocket", json!({})).await?;
            let path = p["socketPath"].as_str().ok_or("sin socketPath")?;
            self.ptr = Some(connect_async(path).await?.0);
        }
        let ptr = self.ptr.as_mut().unwrap();
        ptr.send(Message::text(format!("type:button\nname:{name}\n\n"))).await?;
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
