use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

const DEFAULT_ADDR: &str = "127.0.0.1:7878";

fn main() -> anyhow::Result<()> {
    let command = env::args()
        .nth(1)
        .unwrap_or_else(|| r#"{"command":"snapshot"}"#.to_string());
    let addr = env::var("GITSPARK_AUTOMATION_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());

    let mut stream = TcpStream::connect(&addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut response = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut response)?;

    let value: serde_json::Value = serde_json::from_str(response.trim())?;
    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
