use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::{Context, bail};
use deeptls::Packet;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        TcpListener, TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
};

use crate::{
    capture::{Shared, make_packet, report, unix_now},
    sni,
};

const READ_BUFFER: usize = 16 * 1024;
const MAX_HELLO: usize = 16 * 1024;
const MAX_HANDSHAKE: usize = 8 * 1024;

#[derive(Clone, Copy)]
pub enum Front {
    Sni,
    Proxy,
}

pub async fn serve(shared: Arc<Shared>, listen: &str, front: Front) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("cannot listen on {listen}"))?;
    let kind = match front {
        Front::Sni => "sni passthrough",
        Front::Proxy => "http/socks proxy",
    };
    tracing::info!(
        "observer on {listen} ({kind}), {} rpc host patterns",
        shared.matcher.len()
    );

    loop {
        let (client, peer) = listener.accept().await.context("accept failed")?;
        let shared = shared.clone();
        tokio::spawn(async move {
            let result = match front {
                Front::Sni => handle_sni(&shared, client, peer.ip()).await,
                Front::Proxy => handle_proxy(&shared, client, peer.ip()).await,
            };
            if let Err(error) = result {
                tracing::debug!("connection from {peer} ended: {error}");
            }
        });
    }
}

async fn handle_sni(shared: &Shared, mut client: TcpStream, ip: IpAddr) -> anyhow::Result<()> {
    let (buffered, host) = read_hello(&mut client).await?;
    let Some(host) = host else {
        tracing::debug!("{ip}: no sni, not routed");
        return Ok(());
    };
    let port = shared.default_port;
    tunnel(shared, client, &host, port, buffered, ip).await
}

async fn handle_proxy(shared: &Shared, mut client: TcpStream, ip: IpAddr) -> anyhow::Result<()> {
    let mut first = vec![0u8; READ_BUFFER];
    let read = client.read(&mut first).await?;
    if read == 0 {
        return Ok(());
    }
    first.truncate(read);

    let (host, port, leftover) = if first[0] == 0x05 {
        socks5(&mut client, first).await?
    } else if first.starts_with(b"CONNECT ") {
        http_connect(&mut client, first).await?
    } else {
        bail!("expected socks5 or an http connect");
    };

    tunnel(shared, client, &host, port, leftover, ip).await
}

async fn socks5(
    client: &mut TcpStream,
    first: Vec<u8>,
) -> anyhow::Result<(String, u16, Vec<u8>)> {
    let mut buf = first;
    while buf.len() < 2 || buf.len() < 2 + buf[1] as usize {
        read_more(client, &mut buf).await?;
    }
    let methods = buf[1] as usize;
    let mut request = buf.split_off(2 + methods);
    client.write_all(&[0x05, 0x00]).await?;

    loop {
        if request.len() >= 4 {
            let atyp = request[3];
            let total = match atyp {
                0x01 => Some(10),
                0x04 => Some(22),
                0x03 if request.len() >= 5 => Some(5 + request[4] as usize + 2),
                0x03 => None,
                _ => {
                    client
                        .write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                        .await?;
                    bail!("unsupported socks address type {atyp}");
                }
            };
            if let Some(total) = total
                && request.len() >= total
            {
                if request[1] != 0x01 {
                    client
                        .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                        .await?;
                    bail!("only socks connect is supported");
                }
                let (host, port) = match atyp {
                    0x01 => (
                        Ipv4Addr::new(request[4], request[5], request[6], request[7]).to_string(),
                        u16::from_be_bytes([request[8], request[9]]),
                    ),
                    0x04 => {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(&request[4..20]);
                        (
                            Ipv6Addr::from(octets).to_string(),
                            u16::from_be_bytes([request[20], request[21]]),
                        )
                    }
                    _ => {
                        let len = request[4] as usize;
                        let host = String::from_utf8_lossy(&request[5..5 + len]).into_owned();
                        let port = u16::from_be_bytes([request[5 + len], request[6 + len]]);
                        (host, port)
                    }
                };
                let leftover = request.split_off(total);
                client
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await?;
                return Ok((host, port, leftover));
            }
        }
        if request.len() > MAX_HANDSHAKE {
            bail!("socks request too large");
        }
        read_more(client, &mut request).await?;
    }
}

async fn http_connect(
    client: &mut TcpStream,
    first: Vec<u8>,
) -> anyhow::Result<(String, u16, Vec<u8>)> {
    let mut buf = first;
    let head_end = loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > MAX_HANDSHAKE {
            bail!("http connect header too large");
        }
        read_more(client, &mut buf).await?;
    };

    let leftover = buf[head_end + 4..].to_vec();
    let line_end = find(&buf, b"\r\n").unwrap_or(head_end);
    let line = String::from_utf8_lossy(&buf[..line_end]).into_owned();
    let target = line
        .split_whitespace()
        .nth(1)
        .context("malformed CONNECT line")?;
    let (host, port) = target
        .rsplit_once(':')
        .context("CONNECT target has no port")?;
    let port: u16 = port.parse().context("CONNECT port is not a number")?;

    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;
    Ok((host.to_owned(), port, leftover))
}

async fn read_more(client: &mut TcpStream, buf: &mut Vec<u8>) -> anyhow::Result<()> {
    let mut chunk = [0u8; READ_BUFFER];
    let read = client.read(&mut chunk).await?;
    if read == 0 {
        bail!("peer closed during handshake");
    }
    buf.extend_from_slice(&chunk[..read]);
    Ok(())
}

async fn read_hello(client: &mut TcpStream) -> std::io::Result<(Vec<u8>, Option<String>)> {
    let mut buffer = Vec::with_capacity(READ_BUFFER);
    let mut chunk = vec![0u8; READ_BUFFER];
    loop {
        if let Some(record) = sni::record_from(&buffer) {
            return Ok((buffer.clone(), sni::from_client_hello(record)));
        }
        if buffer.len() > MAX_HELLO {
            return Ok((buffer, None));
        }
        let read = client.read(&mut chunk).await?;
        if read == 0 {
            return Ok((buffer, None));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

async fn tunnel(
    shared: &Shared,
    client: TcpStream,
    host: &str,
    port: u16,
    initial_up: Vec<u8>,
    ip: IpAddr,
) -> anyhow::Result<()> {
    let upstream = TcpStream::connect((host, port))
        .await
        .with_context(|| format!("cannot reach {host}:{port}"))?;

    let started = Instant::now();
    let observed_at = unix_now();
    let packets = Arc::new(Mutex::new(Vec::<Packet>::new()));

    let (client_read, client_write) = client.into_split();
    let (upstream_read, mut upstream_write) = upstream.into_split();

    if !initial_up.is_empty() {
        packets
            .lock()
            .unwrap()
            .push(make_packet(0.0, true, initial_up.clone()));
        upstream_write.write_all(&initial_up).await?;
    }

    let up = relay(client_read, upstream_write, true, started, packets.clone());
    let down = relay(upstream_read, client_write, false, started, packets.clone());
    let _ = tokio::join!(up, down);

    match shared.matcher.find(host) {
        Some(matched) => {
            let captured = packets.lock().unwrap().clone();
            report(shared, ip, matched.chain_id, host, observed_at, &captured).await;
        }
        None => tracing::debug!("{ip} -> {host}: not an rpc host, forwarded only"),
    }
    Ok(())
}

async fn relay(
    mut reader: OwnedReadHalf,
    mut writer: OwnedWriteHalf,
    up: bool,
    started: Instant,
    packets: Arc<Mutex<Vec<Packet>>>,
) -> std::io::Result<()> {
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        let read = reader.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        packets
            .lock()
            .unwrap()
            .push(make_packet(started.elapsed().as_secs_f64(), up, buf[..read].to_vec()));
        writer.write_all(&buf[..read]).await?;
    }
    let _ = writer.shutdown().await;
    Ok(())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
