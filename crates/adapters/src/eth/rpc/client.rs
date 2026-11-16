use std::{collections::HashMap, io, sync::Arc};

use crate::eth::rpc::{parse, pool::RpcPool};
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::{AnyNetwork, AnyRpcBlock, AnyTransactionReceipt, TransactionBuilder},
    providers::{Provider, RootProvider},
    rpc::{
        client::{BatchRequest, ClientBuilder},
        json_rpc::{RpcRecv, RpcSend},
        types::{TransactionInput, TransactionRequest},
    },
    serde::WithOtherFields,
};
use alloy_primitives::{Bytes, TxHash};
use application::{
    BoxStream,
    eth::ports::{EthRpcSource, EthTxSource},
};
use domain::eth::{BlockRef, EthAddress, EthReceipt, MinedTx};
use futures::{StreamExt, TryStreamExt};

pub const DEFAULT_RATE_LIMIT_RPS: u32 = 15;

pub const DEFAULT_BATCH_BLOCKS: usize = 10;
const CHUNKS_PER_NODE: usize = 2;
const MIN_CHUNKS_IN_FLIGHT: usize = 4;
const MAX_CHUNKS_IN_FLIGHT: usize = 32;

pub struct RpcTxSource {
    provider: RootProvider<AnyNetwork>,
    pool: Arc<RpcPool>,
    batch_blocks: usize,
}

impl RpcTxSource {
    pub fn new(pool: Arc<RpcPool>) -> Self {
        let client = ClientBuilder::default().transport(pool.transport(), false);

        Self {
            provider: RootProvider::new(client),
            pool,
            batch_blocks: DEFAULT_BATCH_BLOCKS,
        }
    }

    pub fn with_batch_blocks(self, batch_blocks: usize) -> Self {
        Self {
            batch_blocks: batch_blocks.max(1),
            ..self
        }
    }

    fn chunks_in_flight(&self) -> usize {
        (self.pool.len() * CHUNKS_PER_NODE).clamp(MIN_CHUNKS_IN_FLIGHT, MAX_CHUNKS_IN_FLIGHT)
    }

    async fn batch<Params, Resp>(
        &self,
        method: &'static str,
        from_block: u64,
        to_block: u64,
        params: impl Fn(u64) -> Params,
    ) -> Result<Vec<Resp>, io::Error>
    where
        Params: RpcSend,
        Resp: RpcRecv,
    {
        let blocks: Vec<u64> = (from_block..=to_block).collect();
        let mut chunks = Vec::new();
        for chunk in blocks.chunks(self.batch_blocks) {
            chunks.push(self.batch_chunk(method, chunk, &params));
        }

        let answers: Vec<Vec<Resp>> = futures::stream::iter(chunks)
            .buffered(self.chunks_in_flight())
            .try_collect()
            .await?;

        Ok(answers.into_iter().flatten().collect())
    }

    async fn batch_chunk<Params, Resp>(
        &self,
        method: &'static str,
        chunk: &[u64],
        params: &impl Fn(u64) -> Params,
    ) -> Result<Vec<Resp>, io::Error>
    where
        Params: RpcSend,
        Resp: RpcRecv,
    {
        let mut batch = BatchRequest::new(self.provider.client());
        let mut waiters = Vec::with_capacity(chunk.len());

        for &block in chunk {
            let waiter = batch
                .add_call::<Params, Option<Resp>>(method, &params(block))
                .map_err(|error| {
                    io::Error::other(format!("{method} for block {block}: {error}"))
                })?;

            waiters.push(waiter);
        }

        batch
            .send()
            .await
            .map_err(|error| io::Error::other(format!("{method} failed: {error}")))?;

        let mut answers = Vec::with_capacity(chunk.len());
        for (&block, waiter) in chunk.iter().zip(waiters) {
            match waiter.await {
                Ok(Some(answer)) => answers.push(answer),
                Ok(None) => {
                    return Err(io::Error::other(format!(
                        "{method} knows nothing about block {block}"
                    )));
                }
                Err(error) => {
                    return Err(io::Error::other(format!(
                        "{method} for block {block}: {error}"
                    )));
                }
            }
        }

        Ok(answers)
    }

    async fn mined_txs(&self, from_block: u64, to_block: u64) -> Result<Vec<MinedTx>, io::Error> {
        let blocks = self.batch::<_, AnyRpcBlock>(
            "eth_getBlockByNumber",
            from_block,
            to_block,
            |block_number| (BlockNumberOrTag::Number(block_number), true),
        );

        let receipts = self.batch::<_, Vec<AnyTransactionReceipt>>(
            "eth_getBlockReceipts",
            from_block,
            to_block,
            |block_number| (BlockNumberOrTag::Number(block_number),),
        );

        let (blocks, receipts) = futures::future::try_join(blocks, receipts).await?;

        let receipts: HashMap<TxHash, EthReceipt> = receipts
            .iter()
            .flat_map(|receipts| parse::block_receipts(receipts))
            .collect();

        let mut mined = Vec::new();
        for block in &blocks {
            for tx in parse::block_txs(block) {
                match receipts.get(tx.tx_hash()) {
                    Some(receipt) => mined.push(MinedTx::new(tx, receipt.clone())),
                    None => tracing::warn!("no receipt for {}, skipping it", tx.tx_hash()),
                }
            }
        }

        Ok(mined)
    }
}

fn block_id(block: BlockRef) -> BlockId {
    match block {
        BlockRef::Latest => BlockId::latest(),
        BlockRef::Number(number) => BlockId::number(number),
    }
}

#[async_trait::async_trait]
impl EthRpcSource for RpcTxSource {
    async fn head_block(&self) -> Result<u64, io::Error> {
        self.provider
            .get_block_number()
            .await
            .map_err(|error| io::Error::other(format!("eth_blockNumber failed: {error}")))
    }

    async fn call(
        &self,
        to: &EthAddress,
        data: &[u8],
        block: BlockRef,
    ) -> Result<Bytes, io::Error> {
        let request = TransactionRequest::default()
            .with_to(to.address())
            .input(TransactionInput::new(Bytes::copy_from_slice(data)));

        self.provider
            .call(WithOtherFields::new(request))
            .block(block_id(block))
            .await
            .map_err(|error| io::Error::other(format!("eth_call failed: {error}")))
    }

    async fn code(&self, address: &EthAddress, block: BlockRef) -> Result<Bytes, io::Error> {
        self.provider
            .get_code_at(address.address())
            .block_id(block_id(block))
            .await
            .map_err(|error| io::Error::other(format!("eth_getCode failed: {error}")))
    }

    async fn receipt(&self, tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error> {
        let found = self
            .provider
            .get_transaction_receipt(*tx_hash)
            .await
            .map_err(|error| {
                io::Error::other(format!("eth_getTransactionReceipt failed: {error}"))
            })?;

        Ok(found.as_ref().map(parse::receipt))
    }
}

#[async_trait::async_trait]
impl EthTxSource for RpcTxSource {
    fn parallelism(&self) -> Option<usize> {
        Some(self.pool.len())
    }

    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
        let result = self.mined_txs(lower_block, highest_block).await;

        Box::pin(async_stream::stream! {
            match result {
                Ok(mined) => {
                    for tx in mined {
                        yield Ok(tx);
                    }
                }
                Err(e) => {
                    yield Err(e);
                }
            }
        })
    }
}
