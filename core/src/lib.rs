mod tv;
pub use tv::*;

use std::{net::SocketAddr, time::Duration};
use tokio::{net::UdpSocket, time::timeout};

const SSDP: &str = "239.255.255.250:1900";
const MSEARCH: &str = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: urn:lge-com:service:webos-second-screen:1\r\n\r\n";

/// Busca TVs webOS por SSDP; devuelve la IP de la primera que responda.
pub async fn discover(wait: Duration) -> std::io::Result<Option<SocketAddr>> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.send_to(MSEARCH.as_bytes(), SSDP).await?;
    let mut buf = [0u8; 1024];
    let got = timeout(wait, async {
        loop {
            let (n, from) = sock.recv_from(&mut buf).await?;
            if is_webos(&buf[..n]) {
                return Ok::<_, std::io::Error>(from);
            }
        }
    })
    .await;
    Ok(got.ok().transpose()?)
}

/// Wake-on-LAN: magic packet a cada MAC, a broadcast global y al de la /24 de `ip`.
pub async fn wake(macs: &[String], ip: &str) -> std::io::Result<()> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.set_broadcast(true)?;
    let mut targets = vec!["255.255.255.255:9".to_owned()];
    // ponytail: asume máscara /24; calcular con la máscara real si hace falta
    if let Some((net, _)) = ip.rsplit_once('.') {
        targets.push(format!("{net}.255:9"));
    }
    for packet in macs.iter().filter_map(|m| magic_packet(m)) {
        for t in &targets {
            sock.send_to(&packet, t).await?;
        }
    }
    Ok(())
}

fn magic_packet(mac: &str) -> Option<Vec<u8>> {
    let bytes: Vec<u8> = mac.split(':').map(|h| u8::from_str_radix(h, 16).ok()).collect::<Option<_>>()?;
    if bytes.len() != 6 {
        return None;
    }
    let mut p = vec![0xFF; 6];
    for _ in 0..16 {
        p.extend(&bytes);
    }
    Some(p)
}

fn is_webos(resp: &[u8]) -> bool {
    String::from_utf8_lossy(resp).to_ascii_lowercase().contains("webos-second-screen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_magic_packet() {
        let p = magic_packet("02:00:5E:10:20:30").unwrap();
        assert_eq!(p.len(), 102);
        assert_eq!(&p[..6], &[0xFF; 6]);
        assert_eq!(&p[96..], &[0x02, 0x00, 0x5E, 0x10, 0x20, 0x30]);
        assert!(magic_packet("zz:00").is_none());
    }

    #[test]
    fn parses_webos_response() {
        assert!(is_webos(b"HTTP/1.1 200 OK\r\nST: urn:lge-com:service:webos-second-screen:1\r\n\r\n"));
        assert!(!is_webos(b"HTTP/1.1 200 OK\r\nST: upnp:rootdevice\r\n\r\n"));
    }
}
