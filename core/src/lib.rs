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

fn is_webos(resp: &[u8]) -> bool {
    String::from_utf8_lossy(resp).to_ascii_lowercase().contains("webos-second-screen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_webos_response() {
        assert!(is_webos(b"HTTP/1.1 200 OK\r\nST: urn:lge-com:service:webos-second-screen:1\r\n\r\n"));
        assert!(!is_webos(b"HTTP/1.1 200 OK\r\nST: upnp:rootdevice\r\n\r\n"));
    }
}
