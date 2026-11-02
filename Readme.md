# p2p-files

Transferencia de archivos entre dispositivos de una red local, sin nube,
sin cuentas y sin dependencias externas. Escrito en Rust usando solo la
librería estándar.

![demo](demo.gif)

## ¿Por qué?

Pasar un archivo del celular a la PC (o entre dos PCs) suele implicar
subirlo a un servicio en la nube y volver a bajarlo. Este proyecto lo
manda directo por la red local.

## Uso

```bash
# En la computadora que recibe
cargo run --release -- recibir

# En la computadora que envía
cargo run --release -- enviar 192.168.1.25:7878 foto.jpg
```

Los archivos recibidos se guardan en `recibidos/`. Si el nombre ya
existe, se guarda como `1_foto.jpg`, `2_foto.jpg`, etc.

## Características

- Transferencia directa por TCP, sin servidores intermedios
- Archivos de cualquier tamaño (memoria constante: usa un buffer de 8 KB)
- Barra de progreso en el emisor
- Varias transferencias simultáneas (un hilo por conexión)
- Protección contra nombres maliciosos (`../../archivo`)
- Errores claros en vez de pánicos
- Tests automáticos (`cargo test`)

## Protocolo

TCP entrega un flujo continuo de bytes sin separadores, así que el
emisor y el receptor acuerdan un formato:

```
┌──────────────┬──────────────┬─────────────────┬──────────────┐
│ 1 byte       │ N bytes      │ 8 bytes         │ resto        │
│ largo nombre │ nombre UTF-8 │ tamaño (u64 BE) │ contenido    │
└──────────────┴──────────────┴─────────────────┴──────────────┘
```

Cualquier programa que hable este formato es compatible, sin importar
el lenguaje en que esté escrito.

## Limitaciones

- Funciona dentro de la misma red local. Para redes distintas se puede
  usar una VPN como Tailscale o ZeroTier, sin cambiar el código.
- La transferencia **no está cifrada** ni autenticada: cualquiera en la
  red puede enviar archivos al receptor. Usar solo en redes de confianza.
- Límite de 255 bytes en el nombre del archivo.

## Ideas futuras

- Confirmación antes de aceptar un archivo
- Cifrado (TLS)
- Descubrimiento automático de dispositivos (mDNS)
- App móvil en React Native y conexión por código QR

## Pruebas

```bash
cargo test
```
