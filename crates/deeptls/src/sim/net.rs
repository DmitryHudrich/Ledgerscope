
use crate::sim::rng::Rng;
use crate::tls::{ContentType, Direction, Packet, MAX_PLAINTEXT};
use crate::sim::wallet::Exchange;

const MSS: usize = 1448;
const AEAD_TAG: usize = 16;

#[derive(Clone, Copy, Debug)]
pub enum Defense {
    None,
    Padding(usize),
}

impl Defense {
    fn block(self) -> usize {
        match self {
            Defense::None => 1,
            Defense::Padding(block) => block,
        }
    }
}

pub fn capture(exchanges: &[Exchange], defense: Defense, rng: &mut Rng) -> Vec<Packet> {
    let rtt = rng.range(0.02, 0.15);
    let bandwidth = rng.range(1e6, 8e6);
    let mut packets = Vec::new();
    let mut t = 0.0;
    let mut send = |t: f64, dir: Direction, size: usize, rng: &mut Rng| -> f64 {
        let bytes = encrypt(size, defense.block(), rng);
        let mut t = t;
        for segment in bytes.chunks(MSS) {
            packets.push(Packet { time: t, dir, payload: segment.to_vec() });
            t += segment.len() as f64 / bandwidth;
        }
        t
    };

    for e in exchanges {
        t += e.pause;
        let sent = send(t, Direction::Up, e.request, rng);
        let processed = sent + rtt / 2.0 + e.server_time * rng.range(0.7, 1.5);
        t = send(processed + rtt / 2.0, Direction::Down, e.response, rng);
    }
    packets
}

fn encrypt(plaintext: usize, block: usize, rng: &mut Rng) -> Vec<u8> {
    let max_chunk = MAX_PLAINTEXT / block * block - 1;
    let mut out = Vec::with_capacity(plaintext * 2);
    let mut left = plaintext;
    while left > 0 {
        let chunk = left.min(max_chunk);
        left -= chunk;
        let len = (chunk + 1).div_ceil(block) * block + AEAD_TAG;
        out.push(ContentType::ApplicationData as u8);
        out.extend_from_slice(&[0x03, 0x03]);
        out.extend_from_slice(&(len as u16).to_be_bytes());
        let start = out.len();
        out.resize(start + len, 0);
        rng.fill(&mut out[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::parse_records;
    use crate::sim::wallet::{run, Transaction};

    #[test]
    fn captured_traffic_parses_back_into_records() {
        let mut rng = Rng::new(7);
        for tx in Transaction::ALL {
            let exchanges = run(tx, &mut rng);
            for defense in [Defense::None, Defense::Padding(4096)] {
                let records = parse_records(&capture(&exchanges, defense, &mut rng)).unwrap();
                let up: usize = records.iter().filter(|r| r.dir == Direction::Up).map(|r| r.len).sum();
                let expected: usize = exchanges.iter().map(|e| e.request).sum();
                assert!(up > expected, "{tx:?}: ciphertext must exceed plaintext");
                assert!(records.iter().all(|r| r.kind == ContentType::ApplicationData));
            }
        }
    }
}
