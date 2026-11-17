use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use deeptls::Packet;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::{
    capture::{self, Shared, make_packet, report, unix_now},
    config::SniffConfig,
};

const IDLE: Duration = Duration::from_secs(30);
const SWEEP_EVERY: usize = 512;
const MAX_HELLO: usize = 16 * 1024;

type Key = ((IpAddr, u16), (IpAddr, u16));

struct Finalized {
    ip: IpAddr,
    chain_id: u64,
    host: String,
    observed_at: u64,
    packets: Vec<Packet>,
}

struct Flow {
    client: Option<(IpAddr, u16)>,
    host: Option<String>,
    chain: Option<u64>,
    hello: Vec<u8>,
    decided: bool,
    started: Instant,
    observed_at: u64,
    last: Instant,
    packets: Vec<Packet>,
}

impl Flow {
    fn new(now: Instant) -> Self {
        Self {
            client: None,
            host: None,
            chain: None,
            hello: Vec::new(),
            decided: false,
            started: now,
            observed_at: unix_now(),
            last: now,
            packets: Vec::new(),
        }
    }
}

pub async fn run(shared: Arc<Shared>, sniff: &SniffConfig) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Finalized>();
    let interface = sniff.interface.clone();
    let filter = sniff.filter.clone();
    let worker = shared.clone();
    let handle = std::thread::spawn(move || capture_loop(worker, &interface, &filter, tx));

    while let Some(finalized) = rx.recv().await {
        report(
            &shared,
            finalized.ip,
            finalized.chain_id,
            &finalized.host,
            finalized.observed_at,
            &finalized.packets,
        )
        .await;
    }

    match handle.join() {
        Ok(result) => result,
        Err(_) => anyhow::bail!("pcap worker panicked"),
    }
}

fn capture_loop(
    shared: Arc<Shared>,
    interface: &str,
    filter: &str,
    tx: UnboundedSender<Finalized>,
) -> anyhow::Result<()> {
    let device = if interface.is_empty() {
        pcap::Device::lookup()
            .context("pcap device lookup failed")?
            .context("no default capture device; set sniff.interface")?
    } else {
        pcap::Device::from(interface)
    };

    let mut capture = pcap::Capture::from_device(device)
        .context("cannot open the capture device")?
        .immediate_mode(true)
        .promisc(true)
        .snaplen(65535)
        .timeout(200)
        .open()
        .context("cannot activate the capture; needs CAP_NET_RAW or root")?;
    capture.filter(filter, true).context("invalid bpf filter")?;

    let offset = l2_offset(capture.get_datalink().0)
        .context("unsupported link-layer type on this interface")?;
    tracing::info!(
        "sniffing {interface} ({} rpc host patterns)",
        shared.matcher.len()
    );

    let mut flows: HashMap<Key, Flow> = HashMap::new();
    let mut since_sweep = 0usize;
    loop {
        match capture.next_packet() {
            Ok(packet) => {
                handle_frame(&shared, packet.data, offset, &mut flows, &tx);
                since_sweep += 1;
                if since_sweep >= SWEEP_EVERY {
                    sweep(&mut flows, &tx);
                    since_sweep = 0;
                }
            }
            Err(pcap::Error::TimeoutExpired) => sweep(&mut flows, &tx),
            Err(error) => return Err(error).context("capture failed"),
        }
    }
}

fn handle_frame(
    shared: &Shared,
    data: &[u8],
    offset: usize,
    flows: &mut HashMap<Key, Flow>,
    tx: &UnboundedSender<Finalized>,
) {
    let Some(packet) = data.get(offset..) else {
        return;
    };
    let Some((src, dst, l4, proto)) = parse_ip(packet) else {
        return;
    };
    if proto != 6 {
        return;
    }
    let Some((src_port, dst_port, fin, rst, payload)) = parse_tcp(l4) else {
        return;
    };

    let key = flow_key(src, src_port, dst, dst_port);
    let now = Instant::now();
    let flow = flows.entry(key).or_insert_with(|| Flow::new(now));

    if !payload.is_empty() {
        if flow.client.is_none() {
            flow.client = Some((src, src_port));
        }
        let up = flow.client == Some((src, src_port));
        flow.packets.push(make_packet(
            flow.started.elapsed().as_secs_f64(),
            up,
            payload.to_vec(),
        ));
        if up && !flow.decided {
            flow.hello.extend_from_slice(payload);
            if let Some(record) = crate::sni::record_from(&flow.hello) {
                flow.host = crate::sni::from_client_hello(record);
                flow.chain = flow
                    .host
                    .as_deref()
                    .and_then(|host| shared.matcher.find(host))
                    .map(|matched| matched.chain_id);
                flow.decided = true;
                match (flow.host.as_deref(), flow.chain) {
                    (Some(host), Some(chain)) => {
                        tracing::debug!("{src}: rpc flow to {host} (chain {chain})")
                    }
                    (Some(host), None) => {
                        tracing::debug!("{src}: tls flow to {host}, not an rpc host")
                    }
                    (None, _) => tracing::debug!("{src}: client hello without sni"),
                }
            } else if flow.hello.len() > MAX_HELLO {
                flow.decided = true;
                tracing::debug!("{src}: no sni after {} bytes, giving up", flow.hello.len());
            }
        }
        flow.last = now;
    }

    if fin || rst {
        finalize(key, flows, tx);
    }
}

fn finalize(key: Key, flows: &mut HashMap<Key, Flow>, tx: &UnboundedSender<Finalized>) {
    let Some(flow) = flows.remove(&key) else {
        return;
    };
    let Flow {
        host,
        chain,
        client,
        observed_at,
        packets,
        ..
    } = flow;
    let app = capture::app_records(&packets).unwrap_or(0);
    match (host, chain) {
        (Some(host), Some(chain_id)) if app >= capture::MIN_APP_RECORDS => {
            let ip = client.map(|(ip, _)| ip).unwrap_or(key.0.0);
            let _ = tx.send(Finalized {
                ip,
                chain_id,
                host,
                observed_at,
                packets,
            });
        }
        (Some(host), Some(_)) => tracing::debug!("{host}: too few app records ({app}), skipped"),
        (Some(host), None) => tracing::debug!("{host}: not an rpc host, skipped"),
        (None, _) => tracing::debug!("flow finalized without sni (mid-connection?), skipped"),
    }
}

fn sweep(flows: &mut HashMap<Key, Flow>, tx: &UnboundedSender<Finalized>) {
    let stale: Vec<Key> = flows
        .iter()
        .filter(|(_, flow)| flow.last.elapsed() > IDLE)
        .map(|(key, _)| *key)
        .collect();
    for key in stale {
        finalize(key, flows, tx);
    }
}

fn flow_key(a_ip: IpAddr, a_port: u16, b_ip: IpAddr, b_port: u16) -> Key {
    let a = (a_ip, a_port);
    let b = (b_ip, b_port);
    if a <= b { (a, b) } else { (b, a) }
}

fn parse_ip(packet: &[u8]) -> Option<(IpAddr, IpAddr, &[u8], u8)> {
    match packet.first()? >> 4 {
        4 => {
            if packet.len() < 20 {
                return None;
            }
            let ihl = (packet[0] & 0x0f) as usize * 4;
            let proto = packet[9];
            let src = IpAddr::V4(Ipv4Addr::new(
                packet[12], packet[13], packet[14], packet[15],
            ));
            let dst = IpAddr::V4(Ipv4Addr::new(
                packet[16], packet[17], packet[18], packet[19],
            ));
            Some((src, dst, packet.get(ihl..)?, proto))
        }
        6 => {
            if packet.len() < 40 {
                return None;
            }
            let proto = packet[6];
            let mut src = [0u8; 16];
            let mut dst = [0u8; 16];
            src.copy_from_slice(&packet[8..24]);
            dst.copy_from_slice(&packet[24..40]);
            Some((
                IpAddr::V6(Ipv6Addr::from(src)),
                IpAddr::V6(Ipv6Addr::from(dst)),
                packet.get(40..)?,
                proto,
            ))
        }
        _ => None,
    }
}

fn parse_tcp(l4: &[u8]) -> Option<(u16, u16, bool, bool, &[u8])> {
    if l4.len() < 20 {
        return None;
    }
    let src_port = u16::from_be_bytes([l4[0], l4[1]]);
    let dst_port = u16::from_be_bytes([l4[2], l4[3]]);
    let data_offset = (l4[12] >> 4) as usize * 4;
    let flags = l4[13];
    let fin = flags & 0x01 != 0;
    let rst = flags & 0x04 != 0;
    Some((src_port, dst_port, fin, rst, l4.get(data_offset..)?))
}

fn l2_offset(linktype: i32) -> Option<usize> {
    match linktype {
        0 => Some(4),    // NULL / loopback
        1 => Some(14),   // EN10MB (ethernet)
        12 => Some(0),   // RAW ip
        113 => Some(16), // LINUX_SLL
        _ => None,
    }
}
