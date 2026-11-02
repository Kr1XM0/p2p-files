use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

const TAM_BUFFER: usize = 8192;
const CARPETA_DESTINO: &str = "recibidos";

fn mostrar_progreso(actual: u64, total: u64) {
    let porcentaje = if total == 0 {
        100
    } else {
        actual * 100 / total
    };
    let llenos = (porcentaje / 5) as usize;
    let barra = format!("{}{}", "#".repeat(llenos), "-".repeat(20 - llenos));
    print!("\r[{}] {}%", barra, porcentaje);
    let _ = io::stdout().flush();
}

fn error(tipo: io::ErrorKind, mensaje: &str) -> io::Error {
    io::Error::new(tipo, mensaje)
}

fn crear_archivo_unico(carpeta: &Path, nombre: &str) -> io::Result<(File, PathBuf)> {
    let mut intento = 0;
    loop {
        let ruta = if intento == 0 {
            carpeta.join(nombre)
        } else {
            carpeta.join(format!("{}_{}", intento, nombre))
        };
        match OpenOptions::new().write(true).create_new(true).open(&ruta) {
            Ok(archivo) => return Ok((archivo, ruta)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => intento += 1,
            Err(e) => return Err(e),
        }
    }
}

fn manejar_conexion(mut stream: TcpStream, carpeta: &Path) -> io::Result<()> {
    // 1. Largo del nombre
    let mut largo = [0u8; 1];
    stream.read_exact(&mut largo)?;

    // 2. Nombre
    let mut nombre_bytes = vec![0u8; largo[0] as usize];
    stream.read_exact(&mut nombre_bytes)?;
    let nombre = String::from_utf8(nombre_bytes)
        .map_err(|_| error(io::ErrorKind::InvalidData, "el nombre no es UTF-8 válido"))?;

    let nombre = Path::new(&nombre)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| error(io::ErrorKind::InvalidData, "nombre de archivo inválido"))?
        .to_string();

    // 3. Tamaño del archivo (8 bytes)
    let mut tam = [0u8; 8];
    stream.read_exact(&mut tam)?;
    let total = u64::from_be_bytes(tam);

    // 4. Contenido, por trozos
    let (mut archivo, ruta) = crear_archivo_unico(carpeta, &nombre)?;
    let mut buffer = [0u8; TAM_BUFFER];
    let mut recibido: u64 = 0;

    while recibido < total {
        let falta = (total - recibido).min(TAM_BUFFER as u64) as usize;
        let n = stream.read(&mut buffer[..falta])?;
        if n == 0 {
            return Err(error(
                io::ErrorKind::UnexpectedEof,
                "el emisor cerró la conexión antes de terminar",
            ));
        }
        archivo.write_all(&buffer[..n])?;
        recibido += n as u64;
    }

    println!("Recibido {} ({} bytes)", ruta.display(), recibido);
    Ok(())
}

fn recibir() -> io::Result<()> {
    fs::create_dir_all(CARPETA_DESTINO)?;
    let listener = TcpListener::bind("0.0.0.0:7878")?;
    println!(
        "Escuchando en el puerto 7878. Los archivos se guardan en '{}/'",
        CARPETA_DESTINO
    );

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let origen = s
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "desconocido".to_string());
                println!("Conexión de: {}", origen);

                // Un hilo nuevo por cada conexión
                thread::spawn(move || {
                    if let Err(e) = manejar_conexion(s, Path::new(CARPETA_DESTINO)) {
                        eprintln!("Error en la transferencia con {}: {}", origen, e);
                    }
                });
            }
            Err(e) => eprintln!("Error aceptando conexión: {}", e),
        }
    }
    Ok(())
}

fn enviar(direccion: &str, ruta: &str) -> io::Result<()> {
    let mut archivo = File::open(ruta)?;
    let total = archivo.metadata()?.len();

    let nombre = Path::new(ruta)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            error(
                io::ErrorKind::InvalidInput,
                "la ruta no tiene un nombre de archivo válido",
            )
        })?;
    let nombre_bytes = nombre.as_bytes();
    if nombre_bytes.len() > 255 {
        return Err(error(
            io::ErrorKind::InvalidInput,
            "el nombre del archivo supera los 255 bytes",
        ));
    }

    let mut stream = TcpStream::connect(direccion)?;

    stream.write_all(&[nombre_bytes.len() as u8])?;
    stream.write_all(nombre_bytes)?;
    stream.write_all(&total.to_be_bytes())?;

    let mut buffer = [0u8; TAM_BUFFER];
    let mut enviado: u64 = 0;
    loop {
        let n = archivo.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        stream.write_all(&buffer[..n])?;
        enviado += n as u64;
        mostrar_progreso(enviado, total);
    }

    println!("\nEnviado {} ({} bytes) a {}", nombre, enviado, direccion);
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let resultado = match args.get(1).map(|s| s.as_str()) {
        Some("recibir") => recibir(),
        Some("enviar") => {
            if args.len() < 4 {
                println!("Uso: cargo run -- enviar <direccion:puerto> <archivo>");
                return;
            }
            enviar(&args[2], &args[3])
        }
        _ => {
            println!("Uso:");
            println!("  cargo run -- recibir");
            println!("  cargo run -- enviar <direccion:puerto> <archivo>");
            return;
        }
    };

    if let Err(e) = resultado {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::JoinHandle;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Crea una carpeta temporal única para que los tests no se pisen entre sí
    fn carpeta_temporal(etiqueta: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ruta =
            env::temp_dir().join(format!("p2p_{}_{}_{}", etiqueta, std::process::id(), nanos));
        fs::create_dir_all(&ruta).unwrap();
        ruta
    }

    /// Levanta un receptor que atiende UNA conexión y devuelve su dirección
    fn iniciar_receptor(destino: PathBuf) -> (String, JoinHandle<io::Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let direccion = listener.local_addr().unwrap().to_string();
        let hilo = thread::spawn(move || {
            let (stream, _) = listener.accept()?;
            manejar_conexion(stream, &destino)
        });
        (direccion, hilo)
    }

    /// Envía `contenido` como archivo `nombre` y devuelve lo que llegó
    fn transferir(contenido: &[u8], nombre: &str) -> Vec<u8> {
        let origen = carpeta_temporal("origen");
        let destino = carpeta_temporal("destino");

        let ruta_origen = origen.join(nombre);
        fs::write(&ruta_origen, contenido).unwrap();

        let (direccion, hilo) = iniciar_receptor(destino.clone());
        enviar(&direccion, ruta_origen.to_str().unwrap()).unwrap();
        hilo.join().unwrap().unwrap();

        fs::read(destino.join(nombre)).unwrap()
    }

    #[test]
    fn archivo_llega_identico() {
        let contenido: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let recibido = transferir(&contenido, "datos.bin");
        assert_eq!(contenido, recibido);
    }

    #[test]
    fn archivo_vacio_funciona() {
        let recibido = transferir(&[], "vacio.txt");
        assert!(recibido.is_empty());
    }

    #[test]
    fn nombres_repetidos_no_se_pisan() {
        let carpeta = carpeta_temporal("repetidos");
        let (_, primera) = crear_archivo_unico(&carpeta, "a.txt").unwrap();
        let (_, segunda) = crear_archivo_unico(&carpeta, "a.txt").unwrap();

        assert_eq!(primera.file_name().unwrap(), "a.txt");
        assert_eq!(segunda.file_name().unwrap(), "1_a.txt");
    }

    #[test]
    fn conexion_cortada_da_error() {
        let destino = carpeta_temporal("cortada");
        let (direccion, hilo) = iniciar_receptor(destino);

        let mut stream = TcpStream::connect(&direccion).unwrap();
        let nombre = b"roto.bin";
        stream.write_all(&[nombre.len() as u8]).unwrap();
        stream.write_all(nombre).unwrap();
        stream.write_all(&100u64.to_be_bytes()).unwrap();
        stream.write_all(&[7u8; 10]).unwrap();
        drop(stream);

        let resultado = hilo.join().unwrap();
        let error = resultado.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }
}
