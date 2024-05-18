use anyhow::{bail, Result};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

#[skyline::main(name = "astra-cobalt-plugin")]
fn main() {
    println!("Starting Astra file server.");

    std::panic::set_hook(Box::new(|info| {
        let location = info.location().unwrap();

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<Any>",
            },
        };

        let err_msg = format!(
            "Custom plugin has panicked at '{}' with the following message:\n{}\0",
            location, msg
        );
        skyline::error::show_error(
            1,
            "Custom plugin has panicked! Please open the details and send a screenshot to the developer, then close the game.\n\0",
            err_msg.as_str(),
        );
    }));

    std::thread::spawn(|| {
        let mut logger = Logger::new();

        let server = TcpListener::bind("0.0.0.0:7878").unwrap();
        logger.log(&format!(
            "Started server on address {:?}",
            server.local_addr()
        ));

        for result in server.incoming() {
            logger.log(&format!("Received incoming {:?}", result));

            match result {
                Ok(mut connection) => match process_request(&mut connection, &mut logger) {
                    Ok(_) => {}
                    Err(err) => {
                        logger.log_error(&err);
                        write_error_to_stream(&mut connection, err);
                        let _ = connection.flush();
                    }
                },
                Err(err) => logger.log_error(&err),
            }
        }

        logger.log("Shutting down server...");
    });
}

fn process_request(mut connection: &mut TcpStream, logger: &mut Logger) -> Result<()> {
    logger.log(&format!(
        "Handling connection {:?}",
        connection.local_addr()
    ));

    let mut buf = [0u8; 1];
    connection.read_exact(&mut buf)?;
    let operation = buf[0];

    let mut reader = BufReader::new(&mut connection);
    let mut path = String::new();
    reader.read_line(&mut path)?;
    let path = format!("rom:/Data/{}", path.trim().replace('\\', "/"));

    logger.log(&format!(
        "Received request for file {} operation {}",
        path, operation
    ));

    match operation {
        0 => connection.write_all(&[if Path::new(&path).exists() { 1 } else { 0 }])?,
        1 => {
            let buffer = std::fs::read(&path)?;
            logger.log(&format!(
                "Got file of size {} from path {}",
                buffer.len(),
                path
            ));
            connection.write_all(&[0])?;
            connection.write_all(&buffer.len().to_be_bytes())?;
            connection.write_all(&buffer)?;
        }
        2 => {
            let mut glob = String::new();
            reader.read_line(&mut glob)?;
            let glob = format!("{}/{}", path, glob);

            logger.log(&format!(
                "Ignoring glob for now as filtering is unsupported: {}",
                glob
            ));

            let mut paths = HashSet::new();
            list_files(&path, &mut paths)?;

            logger.log(&format!("Listed {} paths from dir {}", paths.len(), path));

            connection.write_all(&[0])?;
            connection.write_all(&paths.len().to_be_bytes())?;
            for path in paths {
                writeln!(connection, "{}", path.display())?;
            }
        }
        _ => bail!("Unknown operation {}", operation),
    }

    logger.log(&format!("Successfully processed request for file {}", path));
    Ok(())
}

fn write_error_to_stream<E>(connection: &mut TcpStream, err: E)
where
    E: std::fmt::Debug,
{
    let message = format!("{:?}", err);
    let _ = connection.write_all(&[1]);
    let _ = connection.write_all(&message.as_bytes().len().to_be_bytes());
    let _ = connection.write_all(message.as_bytes());
}

fn list_files<P: AsRef<Path>>(dir: P, output: &mut HashSet<PathBuf>) -> Result<()> {
    let dir = dir.as_ref();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                list_files(path, output)?;
            } else {
                let entry_relative_to_root: PathBuf = path.iter().skip(2).collect();
                output.insert(entry_relative_to_root);
            }
        }
    }
    Ok(())
}

struct Logger {
    file: Option<File>,
}

impl Logger {
    pub fn new() -> Self {
        println!("Attempting to create log file...");
        Self {
            file: match File::create(r"sd:/engage/mods/astra-cobalt-plugin/log.txt") {
                Ok(file) => Some(file),
                Err(err) => {
                    println!("Error creating log file: {:?}", err);
                    None
                }
            },
        }
    }

    pub fn log(&mut self, message: &str) {
        println!("{}", message);
        if let Some(file) = &mut self.file {
            let mut writer = BufWriter::new(file);
            let _ = writeln!(writer, "{}", message);
            let _ = writer.flush();
        }
    }

    pub fn log_error<E>(&mut self, error: E)
    where
        E: std::fmt::Debug,
    {
        self.log(&format!("ERROR: {:?}", error));
    }
}
