pub const HEADER_LEN: usize = 5;
pub const MAX_PLAINTEXT: usize = 1 << 14;
pub const MAX_CIPHERTEXT: usize = MAX_PLAINTEXT + 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
}

impl ContentType {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            20 => Self::ChangeCipherSpec,
            21 => Self::Alert,
            22 => Self::Handshake,
            23 => Self::ApplicationData,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub time: f64,
    pub dir: Direction,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Record {
    pub time: f64,
    pub dir: Direction,
    pub kind: ContentType,
    pub len: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnknownContentType(u8),
    BadVersion(u16),
    Oversized(usize),
}

#[derive(Default)]
struct StreamReader {
    buf: Vec<u8>,
}

impl StreamReader {
    fn next_record(&mut self) -> Result<Option<(ContentType, usize)>, ParseError> {
        let Some(header) = self.buf.get(..HEADER_LEN) else {
            return Ok(None);
        };
        let kind =
            ContentType::from_byte(header[0]).ok_or(ParseError::UnknownContentType(header[0]))?;
        let version = u16::from_be_bytes([header[1], header[2]]);
        if !(0x0301..=0x0304).contains(&version) {
            return Err(ParseError::BadVersion(version));
        }
        let len = u16::from_be_bytes([header[3], header[4]]) as usize;
        if len > MAX_CIPHERTEXT {
            return Err(ParseError::Oversized(len));
        }
        if self.buf.len() < HEADER_LEN + len {
            return Ok(None);
        }
        self.buf.drain(..HEADER_LEN + len);
        Ok(Some((kind, len)))
    }
}

pub fn parse_records(packets: &[Packet]) -> Result<Vec<Record>, ParseError> {
    let mut streams = [StreamReader::default(), StreamReader::default()];
    let mut records = Vec::new();
    for packet in packets {
        let stream = &mut streams[packet.dir as usize];
        stream.buf.extend_from_slice(&packet.payload);
        while let Some((kind, len)) = stream.next_record()? {
            records.push(Record {
                time: packet.time,
                dir: packet.dir,
                kind,
                len,
            });
        }
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: u8, len: u16) -> Vec<u8> {
        let mut bytes = vec![kind, 0x03, 0x03];
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.resize(HEADER_LEN + len as usize, 0xAB);
        bytes
    }

    fn packet(time: f64, dir: Direction, payload: &[u8]) -> Packet {
        Packet {
            time,
            dir,
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn reassembles_records_split_across_segments() {
        let mut stream = record(23, 3000);
        stream.extend(record(21, 19));
        let (a, b) = stream.split_at(1448);
        let (b, c) = b.split_at(2);
        let packets = [
            packet(0.0, Direction::Down, a),
            packet(0.1, Direction::Down, b),
            packet(0.2, Direction::Down, c),
        ];

        let records = parse_records(&packets).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(
            (records[0].kind, records[0].len, records[0].time),
            (ContentType::ApplicationData, 3000, 0.2)
        );
        assert_eq!((records[1].kind, records[1].len), (ContentType::Alert, 19));
    }

    #[test]
    fn keeps_directions_independent() {
        let up = record(23, 100);
        let down = record(22, 200);
        let packets = [
            packet(0.0, Direction::Up, &up[..50]),
            packet(0.1, Direction::Down, &down),
            packet(0.2, Direction::Up, &up[50..]),
        ];
        let records = parse_records(&packets).unwrap();
        assert_eq!(
            records.iter().map(|r| (r.dir, r.len)).collect::<Vec<_>>(),
            [(Direction::Down, 200), (Direction::Up, 100)]
        );
    }

    #[test]
    fn rejects_garbage() {
        let packets = [packet(0.0, Direction::Up, b"GET / HTTP/1.1\r\n")];
        assert_eq!(
            parse_records(&packets).unwrap_err(),
            ParseError::UnknownContentType(b'G')
        );
    }
}
