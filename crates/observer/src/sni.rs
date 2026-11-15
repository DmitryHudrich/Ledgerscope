const HANDSHAKE: u8 = 22;
const CLIENT_HELLO: u8 = 1;
const EXT_SERVER_NAME: u16 = 0x0000;
const NAME_TYPE_HOST: u8 = 0;

pub fn from_client_hello(record: &[u8]) -> Option<String> {
    let mut at = 0usize;
    if *record.get(at)? != CLIENT_HELLO {
        return None;
    }
    at += 4;
    at += 2;
    at += 32;

    let session_len = *record.get(at)? as usize;
    at += 1 + session_len;

    let cipher_len = be16(record, at)? as usize;
    at += 2 + cipher_len;

    let compression_len = *record.get(at)? as usize;
    at += 1 + compression_len;

    let extensions_len = be16(record, at)? as usize;
    at += 2;
    let end = (at + extensions_len).min(record.len());

    while at + 4 <= end {
        let ext_type = be16(record, at)?;
        let ext_len = be16(record, at + 2)? as usize;
        at += 4;
        if at + ext_len > record.len() {
            break;
        }
        if ext_type == EXT_SERVER_NAME {
            return host_name(&record[at..at + ext_len]);
        }
        at += ext_len;
    }
    None
}

fn host_name(extension: &[u8]) -> Option<String> {
    if extension.len() < 2 {
        return None;
    }
    let list_len = u16::from_be_bytes([extension[0], extension[1]]) as usize;
    let end = (2 + list_len).min(extension.len());
    let mut at = 2;
    while at + 3 <= end {
        let name_type = extension[at];
        let name_len = u16::from_be_bytes([extension[at + 1], extension[at + 2]]) as usize;
        at += 3;
        if at + name_len > extension.len() {
            break;
        }
        if name_type == NAME_TYPE_HOST {
            return std::str::from_utf8(&extension[at..at + name_len])
                .ok()
                .map(|host| host.to_ascii_lowercase());
        }
        at += name_len;
    }
    None
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
    ]))
}

pub fn record_from(buffer: &[u8]) -> Option<&[u8]> {
    if buffer.len() < 5 || buffer[0] != HANDSHAKE {
        return None;
    }
    let length = u16::from_be_bytes([buffer[3], buffer[4]]) as usize;
    buffer.get(5..5 + length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_hello(host: &str) -> Vec<u8> {
        let mut server_name = Vec::new();
        server_name.push(NAME_TYPE_HOST);
        server_name.extend_from_slice(&(host.len() as u16).to_be_bytes());
        server_name.extend_from_slice(host.as_bytes());

        let mut server_name_list = Vec::new();
        server_name_list.extend_from_slice(&(server_name.len() as u16).to_be_bytes());
        server_name_list.extend_from_slice(&server_name);

        let mut extension = Vec::new();
        extension.extend_from_slice(&EXT_SERVER_NAME.to_be_bytes());
        extension.extend_from_slice(&(server_name_list.len() as u16).to_be_bytes());
        extension.extend_from_slice(&server_name_list);

        let mut body = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]);
        body.extend_from_slice(&[0u8; 32]);
        body.push(0);
        body.extend_from_slice(&2u16.to_be_bytes());
        body.extend_from_slice(&[0x13, 0x01]);
        body.push(1);
        body.push(0);
        body.extend_from_slice(&(extension.len() as u16).to_be_bytes());
        body.extend_from_slice(&extension);

        let mut handshake = Vec::new();
        handshake.push(CLIENT_HELLO);
        handshake.extend_from_slice(&[0, 0, 0]);
        let len = body.len();
        handshake[1] = (len >> 16) as u8;
        handshake[2] = (len >> 8) as u8;
        handshake[3] = len as u8;
        handshake.extend_from_slice(&body);

        let mut record = Vec::new();
        record.push(HANDSHAKE);
        record.extend_from_slice(&[0x03, 0x01]);
        record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
        record.extend_from_slice(&handshake);
        record
    }

    #[test]
    fn reads_the_sni_out_of_a_client_hello() {
        let record = client_hello("api.nile.trongrid.io");
        let inner = record_from(&record).unwrap();
        assert_eq!(
            from_client_hello(inner).as_deref(),
            Some("api.nile.trongrid.io")
        );
    }

    #[test]
    fn a_non_handshake_buffer_has_no_record() {
        assert!(record_from(b"GET / HTTP/1.1\r\n").is_none());
    }
}
