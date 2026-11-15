# Zap Remote

Control remoto para TVs con webOS, rápido y sin nube: habla directo con la TV por la red local (WebSocket), sin cuentas ni anuncios. Escrito en Rust.

- `core/`: cliente de la TV (descubrimiento SSDP, emparejamiento, botones, Wake-on-LAN). No depende de Android.
- `app/`: la app de Android (UI con [Slint](https://slint.dev)).
- `tools/build-aab.sh`: arma el APK/AAB firmado. `tools/make_icon.py`: genera los íconos.

Descargas: [Releases](https://github.com/TwinT/lg-remote/releases). Requiere Android 8+ (arm64), el celular y la TV en la misma red, y activar en la TV la opción de control desde el móvil ("LG Connect Apps" o equivalente). Para encender la TV hay que activar "Encender con Wi-Fi".

Prueba rápida desde la PC: `cargo run -p lg-core --example cli -- discover`.

## Licencia

[GPL-3.0-or-later](LICENSE): podés usar, modificar y redistribuir el código; los derivados deben seguir siendo GPL. La app usa [Slint](https://slint.dev) bajo su opción GPLv3 (Slint también ofrece una licencia "Royalty-Free" y una comercial, que no se usan acá). El resto de las dependencias son MIT/Apache-2.0/BSD/Zlib, compatibles con GPLv3.

Aplicación independiente, no afiliada ni respaldada por LG Electronics. LG y webOS son marcas de sus respectivos dueños.
