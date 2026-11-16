use std::{
    collections::HashMap,
    io,
    ops::Range,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use alloy_primitives::{B256, TxHash, U256};
use application::{
    BoxStream,
    eth::{Transfer, ports::EthTxSource, report_transfer},
};
use arrow_array::{
    Array, BooleanArray, Float64Array, Int64Array, ListArray, RecordBatch, StringArray,
    TimestampNanosecondArray,
};
use arrow_schema::ArrowError;
use bytes::Bytes;
use domain::eth::{BlockRange, EthAddress, EthLog, EthReceipt, EthTx, MinedTx};
use futures::{FutureExt, StreamExt, future::BoxFuture};
use parquet::{
    arrow::{
        ParquetRecordBatchStreamBuilder, ProjectionMask,
        arrow_reader::{ArrowPredicateFn, ArrowReaderOptions, RowFilter},
        async_reader::AsyncFileReader,
    },
    errors::ParquetError,
    file::{
        metadata::{ParquetMetaData, ParquetMetaDataReader},
        statistics::Statistics,
    },
    schema::types::SchemaDescriptor,
};
use reqwest::{Client, StatusCode, Url, header::RANGE};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};

const TRANSACTIONS: &str = "transactions";
const LOGS: &str = "logs";
const DATES_TTL: Duration = Duration::from_secs(3600);
const FETCH_ATTEMPTS: usize = 3;
const DATASET_PARALLELISM: usize = 2;
const REPORT_EVERY_BYTES: u64 = 4 * 1024 * 1024;
const MISSING: [&str; 3] = ["", "None", "null"];

const TX_COLUMNS: [&str; 10] = [
    "hash",
    "block_number",
    "transaction_index",
    "from_address",
    "to_address",
    "value",
    "input",
    "receipt_status",
    "receipt_contract_address",
    "block_timestamp",
];
const LOG_COLUMNS: [&str; 6] = [
    "transaction_hash",
    "block_number",
    "log_index",
    "address",
    "topics",
    "data",
];

#[derive(Clone, Debug)]
pub struct AwsPublicConfig {
    pub base_url: Url,
    pub prefix: String,
    pub cache_dir: PathBuf,
    pub keep_days: usize,
    pub window_blocks: u64,
}

#[derive(Clone, Debug)]
struct Object {
    key: String,
    size: u64,
}

#[derive(Clone, Copy, Debug)]
struct DayBlocks {
    from: u64,
    to: u64,
}

#[derive(Default)]
struct Days {
    listed: Option<(Instant, Vec<String>)>,
    bounds: HashMap<String, DayBlocks>,
}

#[derive(Default)]
struct DiskCache {
    used: HashMap<String, Instant>,
    downloads: HashMap<String, Arc<Mutex<()>>>,
}

pub struct AwsPublicTxSource {
    http: Client,
    config: AwsPublicConfig,
    days: Mutex<Days>,
    disk: Mutex<DiskCache>,
}

impl AwsPublicTxSource {
    pub fn new(http: Client, config: AwsPublicConfig) -> Self {
        Self {
            http,
            config,
            days: Mutex::default(),
            disk: Mutex::default(),
        }
    }

    fn table_prefix(&self, table: &str) -> String {
        format!("{}/{table}/", self.config.prefix.trim_end_matches('/'))
    }

    async fn list(&self, prefix: &str, delimited: bool) -> io::Result<(Vec<Object>, Vec<String>)> {
        let mut objects = Vec::new();
        let mut prefixes = Vec::new();
        let mut token: Option<String> = None;

        loop {
            let mut url = self.config.base_url.clone();
            {
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("list-type", "2")
                    .append_pair("prefix", prefix);
                if delimited {
                    query.append_pair("delimiter", "/");
                }
                if let Some(token) = &token {
                    query.append_pair("continuation-token", token);
                }
            }
            let body = self
                .http
                .get(url)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|error| io::Error::other(format!("listing {prefix} failed: {error}")))?
                .text()
                .await
                .map_err(io::Error::other)?;

            for contents in tagged(&body, "Contents") {
                let key = tagged(contents, "Key").next();
                let size = tagged(contents, "Size")
                    .next()
                    .and_then(|raw| raw.parse().ok());
                if let (Some(key), Some(size)) = (key, size) {
                    objects.push(Object {
                        key: unescape(key),
                        size,
                    });
                }
            }
            for common in tagged(&body, "CommonPrefixes") {
                prefixes.extend(tagged(common, "Prefix").map(unescape));
            }

            token = tagged(&body, "NextContinuationToken").next().map(unescape);
            if token.is_none() || tagged(&body, "IsTruncated").next() != Some("true") {
                return Ok((objects, prefixes));
            }
        }
    }

    async fn dates(&self) -> io::Result<Vec<String>> {
        if let Some((at, dates)) = &self.days.lock().await.listed
            && at.elapsed() < DATES_TTL
        {
            return Ok(dates.clone());
        }

        let prefix = self.table_prefix(TRANSACTIONS);
        let (_, prefixes) = self.list(&prefix, true).await?;
        let mut dates: Vec<String> = prefixes
            .iter()
            .filter_map(|found| found.strip_prefix(&prefix))
            .filter_map(|rest| rest.strip_prefix("date="))
            .map(|rest| rest.trim_end_matches('/').to_owned())
            .collect();
        dates.sort();

        self.days.lock().await.listed = Some((Instant::now(), dates.clone()));
        Ok(dates)
    }

    async fn objects(&self, table: &str, date: &str) -> io::Result<Vec<Object>> {
        let prefix = format!("{}date={date}/", self.table_prefix(table));
        let (objects, _) = self.list(&prefix, false).await?;
        Ok(objects
            .into_iter()
            .filter(|object| object.key.ends_with(".parquet"))
            .collect())
    }

    async fn day_blocks(&self, date: &str) -> io::Result<DayBlocks> {
        if let Some(bounds) = self.days.lock().await.bounds.get(date) {
            return Ok(*bounds);
        }

        let mut from = u64::MAX;
        let mut to = 0;
        for object in self.objects(TRANSACTIONS, date).await? {
            let mut remote = RemoteFile {
                http: self.http.clone(),
                url: self.object_url(&object.key)?,
                size: object.size,
            };
            let metadata = ParquetMetaDataReader::new()
                .load_and_finish(&mut remote, object.size)
                .await
                .map_err(io::Error::other)?;
            if let Some((low, high)) = block_span(&metadata) {
                from = from.min(low);
                to = to.max(high);
            }
        }
        if from > to {
            return Err(io::Error::other(format!(
                "the dataset holds no blocks for {date}"
            )));
        }

        let bounds = DayBlocks { from, to };
        self.days
            .lock()
            .await
            .bounds
            .insert(date.to_owned(), bounds);
        Ok(bounds)
    }

    async fn days_for(&self, span: BlockRange) -> io::Result<Vec<(String, DayBlocks)>> {
        let dates = self.dates().await?;
        let (mut low, mut high) = (0, dates.len());
        while low < high {
            let middle = (low + high) / 2;
            if self.day_blocks(&dates[middle]).await?.to < span.from_block() {
                low = middle + 1;
            } else {
                high = middle;
            }
        }

        let mut days = Vec::new();
        for date in &dates[low..] {
            let bounds = self.day_blocks(date).await?;
            if bounds.from > span.to_block() {
                break;
            }
            days.push((date.clone(), bounds));
        }
        Ok(days)
    }

    fn object_url(&self, key: &str) -> io::Result<Url> {
        self.config
            .base_url
            .join(key)
            .map_err(|error| io::Error::other(format!("{key} is not a valid object path: {error}")))
    }

    async fn local(&self, table: &str, date: &str) -> io::Result<Vec<PathBuf>> {
        let day_key = format!("{table}/{date}");
        let gate = {
            let mut disk = self.disk.lock().await;
            disk.used.insert(date.to_owned(), Instant::now());
            disk.downloads.entry(day_key).or_default().clone()
        };
        let _downloading = gate.lock().await;

        let dir = self.config.cache_dir.join(table).join(date);
        fs::create_dir_all(&dir).await?;
        let mut paths = Vec::new();
        for object in self.objects(table, date).await? {
            let name = object
                .key
                .rsplit('/')
                .next()
                .unwrap_or(&object.key)
                .to_owned();
            let path = dir.join(name);
            let complete = fs::metadata(&path)
                .await
                .is_ok_and(|found| found.len() == object.size);
            if !complete {
                self.download(&object, &path, &format!("{table} {date}"))
                    .await?;
            }
            paths.push(path);
        }
        drop(_downloading);

        self.evict(date).await;
        Ok(paths)
    }

    async fn download(&self, object: &Object, path: &Path, label: &str) -> io::Result<()> {
        tracing::info!(
            "downloading {} ({} MB) from the aws public blockchain dataset",
            object.key,
            object.size / 1_000_000
        );
        let partial = path.with_extension("part");
        let mut response = self
            .http
            .get(self.object_url(&object.key)?)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| {
                io::Error::other(format!("{} did not download: {error}", object.key))
            })?;
        let mut file = fs::File::create(&partial).await?;
        let progress = |done_bytes| {
            report_transfer(Some(Transfer {
                label: label.to_owned(),
                done_bytes,
                total_bytes: object.size,
            }))
        };
        progress(0);
        let mut done = 0;
        let mut reported = 0;
        while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            if done - reported >= REPORT_EVERY_BYTES {
                progress(done);
                reported = done;
            }
        }
        file.flush().await?;
        drop(file);
        report_transfer(None);
        fs::rename(&partial, path).await
    }

    async fn evict(&self, keep: &str) {
        let stale: Vec<String> = {
            let mut disk = self.disk.lock().await;
            let mut used: Vec<(String, Instant)> = disk
                .used
                .iter()
                .map(|(date, at)| (date.clone(), *at))
                .collect();
            used.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
            let stale: Vec<String> = used
                .into_iter()
                .skip(self.config.keep_days.max(1))
                .map(|(date, _)| date)
                .filter(|date| date != keep)
                .collect();
            for date in &stale {
                disk.used.remove(date);
            }
            stale
        };

        for date in stale {
            for table in [TRANSACTIONS, LOGS] {
                let dir = self.config.cache_dir.join(table).join(&date);
                if let Err(error) = fs::remove_dir_all(&dir).await
                    && error.kind() != io::ErrorKind::NotFound
                {
                    tracing::warn!("could not drop the cached {}: {error}", dir.display());
                }
            }
        }
    }

    async fn day_txs(&self, date: &str, span: BlockRange) -> io::Result<Vec<MinedTx>> {
        let mut logs: HashMap<TxHash, Vec<(i64, EthLog)>> = HashMap::new();
        for path in self.local(LOGS, date).await? {
            for batch in read(&path, &LOG_COLUMNS, span).await? {
                collect_logs(&batch, &mut logs)?;
            }
        }

        let mut txs: Vec<(u64, i64, MinedTx)> = Vec::new();
        for path in self.local(TRANSACTIONS, date).await? {
            for batch in read(&path, &TX_COLUMNS, span).await? {
                collect_txs(&batch, &mut logs, &mut txs)?;
            }
        }

        txs.sort_by_key(|(block, index, _)| (*block, *index));
        Ok(txs.into_iter().map(|(_, _, mined)| mined).collect())
    }
}

#[async_trait::async_trait]
impl EthTxSource for AwsPublicTxSource {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
        let span = BlockRange::new(lower_block, highest_block);

        Box::pin(async_stream::stream! {
            let days = match self.days_for(span).await {
                Ok(days) => days,
                Err(error) => {
                    yield Err(error);
                    return;
                }
            };
            let covered = days.last().map(|(_, bounds)| bounds.to);
            if covered.is_none_or(|to| to < highest_block) {
                yield Err(io::Error::other(format!(
                    "the aws public dataset does not reach block {highest_block} yet"
                )));
                return;
            }

            for (date, _) in days {
                match self.day_txs(&date, span).await {
                    Ok(txs) => {
                        for tx in txs {
                            yield Ok(tx);
                        }
                    }
                    Err(error) => {
                        yield Err(error);
                        return;
                    }
                }
            }
        })
    }

    fn parallelism(&self) -> Option<usize> {
        Some(DATASET_PARALLELISM)
    }

    async fn windows(&self, span: BlockRange) -> Option<Vec<BlockRange>> {
        let days = match self.days_for(span).await {
            Ok(days) => days,
            Err(error) => {
                tracing::warn!("could not map blocks to dataset days: {error}");
                return None;
            }
        };
        let size = self.config.window_blocks.max(1);
        let mut windows = Vec::new();
        for (_, bounds) in days {
            let mut from = bounds.from.max(span.from_block());
            let to = bounds.to.min(span.to_block());
            while from <= to {
                let end = from.saturating_add(size - 1).min(to);
                windows.push(BlockRange::new(from, end));
                from = end + 1;
            }
        }
        Some(windows)
    }

    async fn available_until(&self) -> Option<u64> {
        let dates = match self.dates().await {
            Ok(dates) => dates,
            Err(error) => {
                tracing::warn!("the aws public dataset did not list its days: {error}");
                return None;
            }
        };
        let last = dates.last()?;
        match self.day_blocks(last).await {
            Ok(bounds) => Some(bounds.to),
            Err(error) => {
                tracing::warn!("the aws public dataset day {last} did not open: {error}");
                None
            }
        }
    }
}

struct RemoteFile {
    http: Client,
    url: Url,
    size: u64,
}

impl RemoteFile {
    async fn fetch(&self, range: Range<u64>) -> Result<Bytes, ParquetError> {
        let mut last = None;
        for _ in 0..FETCH_ATTEMPTS {
            let answer = self
                .http
                .get(self.url.clone())
                .header(
                    RANGE,
                    format!("bytes={}-{}", range.start, range.end.saturating_sub(1)),
                )
                .send()
                .await;
            match answer {
                Ok(response) if response.status() == StatusCode::PARTIAL_CONTENT => {
                    return response
                        .bytes()
                        .await
                        .map_err(|error| ParquetError::External(Box::new(error)));
                }
                Ok(response) => last = Some(format!("status {}", response.status())),
                Err(error) => last = Some(error.to_string()),
            }
        }
        Err(ParquetError::General(format!(
            "{} bytes {range:?} did not arrive: {}",
            self.url,
            last.unwrap_or_default()
        )))
    }
}

impl AsyncFileReader for RemoteFile {
    fn get_bytes(&mut self, range: Range<u64>) -> BoxFuture<'_, Result<Bytes, ParquetError>> {
        async move { self.fetch(range).await }.boxed()
    }

    fn get_metadata<'a>(
        &'a mut self,
        options: Option<&'a ArrowReaderOptions>,
    ) -> BoxFuture<'a, Result<Arc<ParquetMetaData>, ParquetError>> {
        async move {
            let size = self.size;
            let metadata = ParquetMetaDataReader::new()
                .with_arrow_reader_options(options)
                .load_and_finish(self, size)
                .await?;
            Ok(Arc::new(metadata))
        }
        .boxed()
    }
}

fn column_index(schema: &SchemaDescriptor, name: &str) -> Option<usize> {
    schema
        .root_schema()
        .get_fields()
        .iter()
        .position(|field| field.name() == name)
}

fn block_span(metadata: &ParquetMetaData) -> Option<(u64, u64)> {
    let schema = metadata.file_metadata().schema_descr();
    let column = schema
        .columns()
        .iter()
        .position(|column| column.name() == "block_number")?;
    let mut span: Option<(u64, u64)> = None;
    for group in metadata.row_groups() {
        let Some(Statistics::Int64(stats)) = group.column(column).statistics() else {
            return None;
        };
        let (Some(low), Some(high)) = (stats.min_opt(), stats.max_opt()) else {
            return None;
        };
        let (low, high) = (u64::try_from(*low).ok()?, u64::try_from(*high).ok()?);
        span = Some(match span {
            Some((from, to)) => (from.min(low), to.max(high)),
            None => (low, high),
        });
    }
    span
}

async fn read(path: &Path, columns: &[&str], span: BlockRange) -> io::Result<Vec<RecordBatch>> {
    let file = fs::File::open(path).await?;
    let builder = ParquetRecordBatchStreamBuilder::new(file)
        .await
        .map_err(io::Error::other)?;
    let metadata = builder.metadata().clone();
    let schema = metadata.file_metadata().schema_descr();

    let wanted: Vec<usize> = columns
        .iter()
        .map(|name| {
            column_index(schema, name)
                .ok_or_else(|| io::Error::other(format!("{} has no {name} column", path.display())))
        })
        .collect::<io::Result<_>>()?;
    let block_root = column_index(schema, "block_number")
        .ok_or_else(|| io::Error::other(format!("{} has no block_number", path.display())))?;
    let block_leaf = schema
        .columns()
        .iter()
        .position(|column| column.name() == "block_number")
        .unwrap_or(block_root);

    let groups: Vec<usize> = metadata
        .row_groups()
        .iter()
        .enumerate()
        .filter(|(_, group)| match group.column(block_leaf).statistics() {
            Some(Statistics::Int64(stats)) => match (stats.min_opt(), stats.max_opt()) {
                (Some(low), Some(high)) => {
                    *high >= span.from_block() as i64 && *low <= span.to_block() as i64
                }
                _ => true,
            },
            _ => true,
        })
        .map(|(index, _)| index)
        .collect();
    if groups.is_empty() {
        return Ok(Vec::new());
    }

    let (from, to) = (span.from_block() as i64, span.to_block() as i64);
    let predicate = ArrowPredicateFn::new(
        ProjectionMask::roots(schema, [block_root]),
        move |batch: RecordBatch| {
            let blocks = batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| ArrowError::SchemaError("block_number is not int64".to_owned()))?;
            Ok(BooleanArray::from_iter(blocks.iter().map(|block| {
                Some(block.is_some_and(|block| block >= from && block <= to))
            })))
        },
    );

    let mut stream = builder
        .with_projection(ProjectionMask::roots(schema, wanted))
        .with_row_groups(groups)
        .with_row_filter(RowFilter::new(vec![Box::new(predicate)]))
        .build()
        .map_err(io::Error::other)?;

    let mut batches = Vec::new();
    while let Some(batch) = stream.next().await {
        batches.push(batch.map_err(io::Error::other)?);
    }
    Ok(batches)
}

fn strings<'a>(batch: &'a RecordBatch, name: &str) -> io::Result<&'a StringArray> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| io::Error::other(format!("the dataset column {name} is not text")))
}

fn integers<'a>(batch: &'a RecordBatch, name: &str) -> io::Result<&'a Int64Array> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
        .ok_or_else(|| io::Error::other(format!("the dataset column {name} is not an integer")))
}

fn text_at(array: &StringArray, row: usize) -> Option<&str> {
    (!array.is_null(row))
        .then(|| array.value(row))
        .filter(|text| !MISSING.contains(text))
}

fn address(raw: &str) -> io::Result<EthAddress> {
    EthAddress::from_str(raw)
        .map_err(|error| io::Error::other(format!("{raw} is not an address: {error}")))
}

fn hash(raw: &str) -> io::Result<B256> {
    B256::from_str(raw).map_err(|error| io::Error::other(format!("{raw} is not a hash: {error}")))
}

fn hex_bytes(raw: &str) -> io::Result<alloy_primitives::Bytes> {
    alloy_primitives::Bytes::from_str(raw)
        .map_err(|error| io::Error::other(format!("{raw} is not hex: {error}")))
}

fn wei(value: f64) -> U256 {
    if !value.is_finite() || value <= 0.0 {
        return U256::ZERO;
    }
    U256::from_str_radix(&format!("{value:.0}"), 10).unwrap_or_default()
}

fn collect_logs(
    batch: &RecordBatch,
    logs: &mut HashMap<TxHash, Vec<(i64, EthLog)>>,
) -> io::Result<()> {
    let hashes = strings(batch, "transaction_hash")?;
    let indexes = integers(batch, "log_index")?;
    let addresses = strings(batch, "address")?;
    let datas = strings(batch, "data")?;
    let topics = batch
        .column_by_name("topics")
        .and_then(|column| column.as_any().downcast_ref::<ListArray>())
        .ok_or_else(|| io::Error::other("the dataset column topics is not a list"))?;

    for row in 0..batch.num_rows() {
        let (Some(tx), Some(emitter)) = (text_at(hashes, row), text_at(addresses, row)) else {
            continue;
        };
        let mut words = Vec::new();
        if !topics.is_null(row) {
            let values = topics.value(row);
            let values = values
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| io::Error::other("the dataset topics are not text"))?;
            for topic in values.iter().flatten() {
                words.push(hash(topic)?);
            }
        }
        let data = match text_at(datas, row) {
            Some(raw) => hex_bytes(raw)?,
            None => alloy_primitives::Bytes::new(),
        };
        logs.entry(hash(tx)?).or_default().push((
            indexes.value(row),
            EthLog::new(address(emitter)?, words, data),
        ));
    }
    Ok(())
}

fn collect_txs(
    batch: &RecordBatch,
    logs: &mut HashMap<TxHash, Vec<(i64, EthLog)>>,
    txs: &mut Vec<(u64, i64, MinedTx)>,
) -> io::Result<()> {
    let hashes = strings(batch, "hash")?;
    let blocks = integers(batch, "block_number")?;
    let positions = integers(batch, "transaction_index")?;
    let senders = strings(batch, "from_address")?;
    let receivers = strings(batch, "to_address")?;
    let inputs = strings(batch, "input")?;
    let statuses = integers(batch, "receipt_status")?;
    let created = strings(batch, "receipt_contract_address")?;
    let values = batch
        .column_by_name("value")
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
        .ok_or_else(|| io::Error::other("the dataset column value is not a double"))?;
    let stamps = batch
        .column_by_name("block_timestamp")
        .and_then(|column| column.as_any().downcast_ref::<TimestampNanosecondArray>())
        .ok_or_else(|| io::Error::other("the dataset column block_timestamp is not a timestamp"))?;

    for row in 0..batch.num_rows() {
        let (Some(tx_hash), Some(from)) = (text_at(hashes, row), text_at(senders, row)) else {
            continue;
        };
        let tx_hash = hash(tx_hash)?;
        let block = u64::try_from(blocks.value(row)).unwrap_or_default();
        let to = text_at(receivers, row).map(address).transpose()?;
        let data = match text_at(inputs, row) {
            Some(raw) => hex_bytes(raw)?,
            None => alloy_primitives::Bytes::new(),
        };
        let tx = EthTx::builder()
            .tx_hash(tx_hash)
            .block_number(block)
            .timestamp(u64::try_from(stamps.value(row) / 1_000_000_000).unwrap_or_default())
            .amount(wei(values.value(row)))
            .from(address(from)?)
            .maybe_to(to)
            .data(data)
            .build();

        let mut own = logs.remove(&tx_hash).unwrap_or_default();
        own.sort_by_key(|(index, _)| *index);
        let receipt = EthReceipt::new(
            statuses.value(row) == 1,
            text_at(created, row).map(address).transpose()?,
            own.into_iter().map(|(_, log)| log).collect(),
        );
        txs.push((block, positions.value(row), MinedTx::new(tx, receipt)));
    }
    Ok(())
}

fn tagged<'a>(body: &'a str, tag: &str) -> impl Iterator<Item = &'a str> + 'a {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut rest = body;
    std::iter::from_fn(move || {
        let start = rest.find(&open)? + open.len();
        let end = rest[start..].find(&close)? + start;
        let found = &rest[start..end];
        rest = &rest[end + close.len()..];
        Some(found)
    })
}

fn unescape(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_yields_its_objects_prefixes_and_token() {
        let body = "<ListBucketResult><IsTruncated>true</IsTruncated>\
            <Contents><Key>v1.0/eth/logs/date=2025-11-10/part-0.parquet</Key><Size>42</Size></Contents>\
            <CommonPrefixes><Prefix>v1.0/eth/logs/date=2025-11-11/</Prefix></CommonPrefixes>\
            <NextContinuationToken>a&amp;b</NextContinuationToken></ListBucketResult>";

        let keys: Vec<&str> = tagged(body, "Contents")
            .flat_map(|contents| tagged(contents, "Key"))
            .collect();
        assert_eq!(keys, ["v1.0/eth/logs/date=2025-11-10/part-0.parquet"]);
        assert_eq!(
            tagged(body, "Prefix").collect::<Vec<_>>(),
            ["v1.0/eth/logs/date=2025-11-11/"]
        );
        assert_eq!(
            unescape(tagged(body, "NextContinuationToken").next().unwrap()),
            "a&b"
        );
    }

    #[test]
    fn a_double_value_becomes_whole_wei() {
        assert_eq!(wei(1.5e18), U256::from(1_500_000_000_000_000_000_u128));
        assert_eq!(wei(0.0), U256::ZERO);
        assert_eq!(wei(f64::NAN), U256::ZERO);
        assert_eq!(wei(-3.0), U256::ZERO);
    }
}
