use alloy::{
    consensus::Transaction,
    network::{AnyRpcBlock, AnyTransactionReceipt, ReceiptResponse, TransactionResponse},
    rpc::types::Log,
};
use alloy_primitives::TxHash;

use domain::eth::{EthAddress, EthLog, EthReceipt, EthTx};

pub fn block_txs(block: &AnyRpcBlock) -> Vec<EthTx> {
    block
        .transactions
        .txns()
        .map(|tx| {
            EthTx::builder()
                .tx_hash(tx.tx_hash())
                .block_number(tx.block_number.unwrap_or(block.header.number))
                .timestamp(block.header.timestamp)
                .amount(tx.value())
                .from(EthAddress::from(tx.from()))
                .maybe_to(tx.to().map(EthAddress::from))
                .data(tx.input().clone())
                .build()
        })
        .collect()
}

pub fn block_receipts(receipts: &[AnyTransactionReceipt]) -> Vec<(TxHash, EthReceipt)> {
    receipts
        .iter()
        .map(|found| (found.transaction_hash, receipt(found)))
        .collect()
}

pub fn receipt(receipt: &AnyTransactionReceipt) -> EthReceipt {
    EthReceipt::new(
        receipt.status(),
        receipt.contract_address().map(EthAddress::from),
        receipt.inner.inner.logs().iter().map(log).collect(),
    )
}

fn log(log: &Log) -> EthLog {
    EthLog::new(
        EthAddress::from(log.inner.address),
        log.inner.topics().to_vec(),
        log.inner.data.data.clone(),
    )
}
