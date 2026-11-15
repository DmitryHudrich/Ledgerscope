use crate::tls::{ContentType, Direction, Record};

const PREFIX: usize = 16;

pub fn extract(records: &[Record]) -> Vec<f64> {
    let data: Vec<&Record> = records.iter().filter(|r| r.kind == ContentType::ApplicationData).collect();
    let (up, down): (Vec<&Record>, Vec<&Record>) = data.iter().partition(|r| r.dir == Direction::Up);
    let bytes = |rs: &[&Record]| rs.iter().map(|r| r.len).sum::<usize>();
    let duration = match (data.first(), data.last()) {
        (Some(first), Some(last)) => last.time - first.time,
        _ => 0.0,
    };
    let bursts = data.windows(2).filter(|w| w[0].dir != w[1].dir).count() + 1;
    let max_down = down.iter().map(|r| r.len).max().unwrap_or(0);

    let mut features = vec![
        log(up.len()),
        log(down.len()),
        log(bytes(&up)),
        log(bytes(&down)),
        log(max_down),
        log(bursts),
        (duration * 1000.0).ln_1p(),
    ];
    features.extend((0..PREFIX).map(|i| data.get(i).map_or(0.0, |r| signed_size(r))));
    features
}

fn log(x: usize) -> f64 {
    (x as f64).ln_1p()
}

fn signed_size(record: &Record) -> f64 {
    match record.dir {
        Direction::Up => log(record.len),
        Direction::Down => -log(record.len),
    }
}
