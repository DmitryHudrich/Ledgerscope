use deeptls::classifier::{Classifier, Intent, TrainConfig};
use deeptls::{Direction, Packet};

fn header(kind: u8, len: u16) -> Vec<u8> {
    let mut bytes = vec![kind, 0x03, 0x03];
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.resize(5 + len as usize, 0xAB);
    bytes
}

#[test]
fn trains_and_classifies_parsed_segments() {
    let classifier = Classifier::train(TrainConfig::default());

    let mut up = header(23, 1500);
    up.extend(header(23, 400));
    let packets = vec![
        Packet { time: 0.0, dir: Direction::Up, payload: up },
        Packet { time: 0.1, dir: Direction::Down, payload: header(23, 250) },
    ];

    let verdict = classifier.classify_packets(&packets).unwrap();
    assert!(verdict.confidence >= 0.0 && verdict.confidence <= 1.0);
    assert!(matches!(verdict.intent, Intent::Broadcast | Intent::Query));
}
