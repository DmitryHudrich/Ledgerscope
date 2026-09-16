use std::{collections::HashMap, io};

use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::TransactionBuilder,
    providers::{Provider, RootProvider},
    rpc::{
        client::{BatchRequest, ClientBuilder},
        json_rpc::{RpcRecv, RpcSend},
        types::{Block, TransactionInput, TransactionReceipt, TransactionRequest},
    },
};
use alloy_primitives::{Bytes, TxHash};
use application::{
    BoxStream,
    eth::ports::{EthRpcSource, EthTxSource},
};
use domain::eth::{BlockRef, EthAddress, EthReceipt, MinedTx};
use reqwest::{Client, Url};

use crate::eth::rpc::{client::rate_limit::RateLimitLayer, parse};

mod rate_limit;

pub const DEFAULT_RATE_LIMIT_RPS: u32 = 15;

pub struct RpcTxSource {
    provider: RootProvider,
    batch_size: usize,
}

impl RpcTxSource {
    pub fn new(http_client: Client, rpc_url: Url, rps: u32) -> Self {
        let rps = rps.max(1);
        let client = ClientBuilder::default()
            .layer(RateLimitLayer::new(rps))
            .http_with_client(http_client, rpc_url);

        Self {
            provider: RootProvider::new(client),
            batch_size: rps as usize,
        }
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
        let mut answers = Vec::with_capacity(blocks.len());

        for chunk in blocks.chunks(self.batch_size) {
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
        }

        Ok(answers)
    }

    async fn mined_txs(&self, from_block: u64, to_block: u64) -> Result<Vec<MinedTx>, io::Error> {
        let blocks = self.batch::<_, Block>(
            "eth_getBlockByNumber",
            from_block,
            to_block,
            |block_number| (BlockNumberOrTag::Number(block_number), true),
        );

        let receipts = self.batch::<_, Vec<TransactionReceipt>>(
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
            .call(request)
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
