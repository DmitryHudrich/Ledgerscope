use adapters::eth::RpcTxSource;
use application::eth::ports::{EthRpcSource, EthTxSource};
use domain::eth::BlockRef;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn serve(answers: Vec<Value>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    let handle = tokio::spawn(async move {
        let mut seen = Vec::new();

        for answer in answers {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = Vec::new();

            let body = loop {
                let mut buffer = [0u8; 4096];
                let read = socket.read(&mut buffer).await.unwrap();
                raw.extend_from_slice(&buffer[..read]);

                let text = String::from_utf8_lossy(&raw).into_owned();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length: usize = head
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap())
                        })
                        .unwrap();

                    if body.len() >= length {
                        break body.to_owned();
                    }
                }
            };

            let request: Value = serde_json::from_str(&body).unwrap();
            let answer = answer_for(&request, answer);
            let payload = answer.to_string();
            seen.push(request);

            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                        payload.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            socket.flush().await.unwrap();
        }

        seen
    });

    (url, handle)
}

fn answer_for(request: &Value, results: Value) -> Value {
    match request.as_array() {
        Some(requests) => Value::from(
            requests
                .iter()
                .zip(results.as_array().unwrap())
                .map(|(request, result)| single(&request["id"], result.clone()))
                .collect::<Vec<_>>(),
        ),
        None => single(&request["id"], results),
    }
}

fn single(id: &Value, result: Value) -> Value {
    match result.get("__error") {
        Some(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        None => json!({"jsonrpc": "2.0", "id": id, "result": result}),
    }
}

fn source(url: &str) -> RpcTxSource {
    RpcTxSource::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        reqwest::Url::parse(url).unwrap(),
        50,
    )
}

fn block(number: u64, tx_hash: &str) -> Value {
    json!({
        "number": format!("0x{number:x}"),
        "hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
        "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
        "logsBloom": format!("0x{}", "0".repeat(512)),
        "transactionsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "stateRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "miner": "0x0000000000000000000000000000000000000000",
        "difficulty": "0x0",
        "extraData": "0x",
        "gasLimit": "0x1c9c380",
        "gasUsed": "0x5208",
        "timestamp": "0x64000000",
        "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "nonce": "0x0000000000000000",
        "baseFeePerGas": "0x7",
        "size": "0x220",
        "uncles": [],
        "transactions": [{
            "hash": tx_hash,
            "nonce": "0x1",
            "blockHash": "0x1111111111111111111111111111111111111111111111111111111111111111",
            "blockNumber": format!("0x{number:x}"),
            "transactionIndex": "0x0",
            "from": "0x00000000000000000000000000000000000000aa",
            "to": "0x00000000000000000000000000000000000000bb",
            "value": "0x2a",
            "gasPrice": "0x9",
            "gas": "0x5208",
            "input": "0xdeadbeef",
            "type": "0x0",
            "chainId": "0x1",
            "v": "0x25",
            "r": "0x1",
            "s": "0x2"
        }]
    })
}

fn receipt(number: u64, tx_hash: &str) -> Value {
    json!({
        "transactionHash": tx_hash,
        "transactionIndex": "0x0",
        "blockHash": "0x1111111111111111111111111111111111111111111111111111111111111111",
        "blockNumber": format!("0x{number:x}"),
        "from": "0x00000000000000000000000000000000000000aa",
        "to": "0x00000000000000000000000000000000000000bb",
        "cumulativeGasUsed": "0x5208",
        "gasUsed": "0x5208",
        "contractAddress": null,
        "effectiveGasPrice": "0x9",
        "type": "0x0",
        "status": "0x1",
        "logsBloom": format!("0x{}", "0".repeat(512)),
        "logs": [{
            "address": "0x00000000000000000000000000000000000000cc",
            "topics": ["0x2222222222222222222222222222222222222222222222222222222222222222"],
            "data": "0xc0ffee",
            "blockHash": "0x1111111111111111111111111111111111111111111111111111111111111111",
            "blockNumber": format!("0x{number:x}"),
            "transactionHash": tx_hash,
            "transactionIndex": "0x0",
            "logIndex": "0x0",
            "removed": false
        }]
    })
}

#[tokio::test]
async fn mined_txs_come_back_from_a_batch() {
    let first = "0x3333333333333333333333333333333333333333333333333333333333333333";
    let second = "0x4444444444444444444444444444444444444444444444444444444444444444";

    let (url, server) = serve(vec![
        json!([block(1, first), block(2, second)]),
        json!([[receipt(1, first)], [receipt(2, second)]]),
    ])
    .await;

    let source = source(&url);
    let mined: Vec<_> = source.txs(1, 2).await.collect().await;
    let seen = server.await.unwrap();

    let mined: Vec<_> = mined.into_iter().map(Result::unwrap).collect();
    assert_eq!(mined.len(), 2);

    let tx = mined[0].tx();
    assert_eq!(tx.tx_hash().to_string(), first);
    assert_eq!(tx.block_number(), 1);
    assert_eq!(tx.timestamp(), 0x64000000);
    assert_eq!(tx.amount().to::<u64>(), 42);
    assert_eq!(
        tx.from().to_string(),
        "0x00000000000000000000000000000000000000aa"
    );
    assert_eq!(
        tx.to().unwrap().to_string(),
        "0x00000000000000000000000000000000000000bb"
    );
    assert_eq!(tx.data().to_string(), "0xdeadbeef");

    let receipt = mined[0].receipt();
    assert!(receipt.succeeded());
    assert_eq!(receipt.logs().len(), 1);
    assert_eq!(receipt.logs()[0].data().to_string(), "0xc0ffee");

    let methods: Vec<&str> = seen
        .iter()
        .map(|request| request[0]["method"].as_str().unwrap())
        .collect();
    assert!(methods.contains(&"eth_getBlockByNumber"));
    assert!(methods.contains(&"eth_getBlockReceipts"));

    let blocks = seen
        .iter()
        .find(|request| request[0]["method"] == "eth_getBlockByNumber")
        .unwrap();
    assert_eq!(blocks[0]["params"], json!(["0x1", true]));
    assert_eq!(blocks[1]["params"], json!(["0x2", true]));

    let receipts = seen
        .iter()
        .find(|request| request[0]["method"] == "eth_getBlockReceipts")
        .unwrap();
    assert_eq!(receipts[0]["params"], json!(["0x1"]));
}

#[tokio::test]
async fn call_and_code_ask_for_the_right_block() {
    let (url, server) = serve(vec![json!("0xbeef"), json!("0xfeed")]).await;
    let source = source(&url);

    let returned = source
        .call(
            &"0x00000000000000000000000000000000000000bb"
                .parse()
                .unwrap(),
            &[0x95, 0xd8, 0x9b, 0x41],
            BlockRef::Number(7),
        )
        .await
        .unwrap();
    assert_eq!(returned.to_string(), "0xbeef");

    let code = source
        .code(
            &"0x00000000000000000000000000000000000000bb"
                .parse()
                .unwrap(),
            BlockRef::Latest,
        )
        .await
        .unwrap();
    assert_eq!(code.to_string(), "0xfeed");

    let seen = server.await.unwrap();
    assert_eq!(seen[0]["method"], "eth_call");
    assert_eq!(
        seen[0]["params"][0]["to"],
        "0x00000000000000000000000000000000000000bb"
    );
    assert_eq!(seen[0]["params"][0]["input"], "0x95d89b41");
    assert_eq!(seen[0]["params"][1], "0x7");
    assert_eq!(seen[1]["method"], "eth_getCode");
    assert_eq!(seen[1]["params"][1], "latest");
}

#[tokio::test]
async fn a_rate_limited_answer_is_retried() {
    let tx_hash = "0x3333333333333333333333333333333333333333333333333333333333333333";
    let (url, server) = serve(vec![
        json!({"__error": {"code": -32005, "message": "rate limit exceeded"}}),
        json!(receipt(1, tx_hash)),
    ])
    .await;

    let source = source(&url);
    let found = source.receipt(&tx_hash.parse().unwrap()).await.unwrap();
    let seen = server.await.unwrap();

    assert_eq!(seen.len(), 2);
    assert!(found.unwrap().succeeded());
}

#[tokio::test]
async fn a_plain_error_is_not_retried() {
    let tx_hash = "0x3333333333333333333333333333333333333333333333333333333333333333";
    let (url, server) = serve(vec![
        json!({"__error": {"code": -32000, "message": "execution reverted"}}),
    ])
    .await;

    let source = source(&url);
    let failed = source.receipt(&tx_hash.parse().unwrap()).await;
    let seen = server.await.unwrap();

    assert_eq!(seen.len(), 1);
    assert!(
        failed
            .unwrap_err()
            .to_string()
            .contains("execution reverted")
    );
}
