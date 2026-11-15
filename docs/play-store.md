# Publicar en Google Play

Artefactos: `tools/build-aab.sh` genera `target/aab/out/app.aab` (subir a Play) y `universal.apk` (probar por adb).
Subir una versión nueva: `VERSION_CODE=2 VERSION_NAME=0.1.1 tools/build-aab.sh` (el versionCode siempre crece).

## Antes de subir
- Cuenta de desarrollador de Play (USD 25, una vez). Si es **personal** y se creó después de nov-2023: prueba cerrada con **12 testers durante 14 días** antes de pedir producción. Las cuentas de organización están exentas.
- Activar **Play App Signing** (default). La clave de `~/.local/share/lg-remote/upload.jks` es solo la de *subida*; su contraseña está en el keyring (`secret-tool lookup name lgdoom_upload_keystore`). Hacer copia del keystore; si se pierde, Play permite resetear la clave de subida.
- El package id `io.github.twint.lgdoom` es **permanente** una vez subida la primera versión.
- Marcas: la app **no** usa nombre ni logo de LG. En la ficha decir "compatible con TVs webOS" y agregar el descargo de abajo. No usar "LG" en el título ni en capturas.

## Ficha (es-AR)
- **Nombre:** Zap Remote
- **Descripción corta (≤80):** Control remoto rápido para TVs webOS por wifi. Sin nube, sin cuenta, sin anuncios.
- **Descripción completa:**
  Zap Remote controla tu TV con webOS desde el celular, directo por tu red wifi: sin nube, sin registro y sin anuncios.
  - Conexión en una fracción de segundo
  - Cruceta, OK, Volver, Inicio, volumen, canales y silencio
  - Reproducción: play, pausa, adelantar y retroceder
  - Cambio de entrada (HDMI, AV...) y lanzador de apps instaladas
  - Teclado numérico y botones de colores
  - Modo touchpad con scroll
  - Encendido de la TV con Wake-on-LAN
  Requiere que el celular y la TV estén en la misma red wifi, y tener activada en la TV la opción para controlarla desde el móvil ("LG Connect Apps" u opción equivalente). Para encender la TV hay que activar "Encender con Wi-Fi".
  *Aplicación independiente, no afiliada ni respaldada por LG Electronics. webOS y LG son marcas de sus respectivos dueños.*
- **Categoría:** Herramientas (o Estilo de vida). **Clasificación de contenido:** sin violencia, sin compras, sin anuncios.
- **Ícono:** `icon/play-store-512.png`. Falta el gráfico de funciones (1024x500) y 2+ capturas de pantalla del celular.

## Seguridad de los datos (formulario)
- No recopila ni comparte datos. Todo (IP de la TV, clave de emparejamiento, MAC) se guarda solo en el dispositivo.
- Permiso: `INTERNET` (comunicación con la TV en la red local).
- Tráfico sin cifrar (`ws://`) **solo dentro de la red local** hacia la TV (puerto 3000 de webOS).
- Política de privacidad: Play pide una URL pública (no sirve un repo privado; p. ej. GitHub Pages). Texto mínimo: "La app no recopila ni transmite datos personales; la configuración queda en el dispositivo."

## Requisitos técnicos ya cumplidos
targetSdk 36 (obligatorio para apps nuevas desde 31/8/2026), minSdk 26, AAB firmado, librería nativa con páginas de 16 KB, solo arm64-v8a.
