use serde_json::{Value, json};

use crate::sim::node::{TRANSFER_TOPIC, TestNode, hex, hex_raw, quantity, tron_address};
use crate::sim::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transaction {
    EthTransfer,
    Erc20Transfer,
    EthPortfolio,
    EthLogScan,
    TrxTransfer,
    Trc20Transfer,
    TronAccount,
}

impl Transaction {
    pub const ALL: [Self; 7] = [
        Self::EthTransfer,
        Self::Erc20Transfer,
        Self::EthPortfolio,
        Self::EthLogScan,
        Self::TrxTransfer,
        Self::Trc20Transfer,
        Self::TronAccount,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::EthTransfer => "eth-send",
            Self::Erc20Transfer => "erc20-send",
            Self::EthPortfolio => "eth-portf",
            Self::EthLogScan => "eth-logs",
            Self::TrxTransfer => "trx-send",
            Self::Trc20Transfer => "trc20-send",
            Self::TronAccount => "tron-acct",
        }
    }

    fn is_tron(self) -> bool {
        matches!(
            self,
            Self::TrxTransfer | Self::Trc20Transfer | Self::TronAccount
        )
    }
}

#[derive(Clone, Debug)]
pub struct Exchange {
    pub call: String,
    pub pause: f64,
    pub request: usize,
    pub response: usize,
    pub server_time: f64,
}

pub fn run(tx: Transaction, rng: &mut Rng) -> Vec<Exchange> {
    let node = TestNode::new(rng);
    let mut wallet = Wallet {
        node,
        rng,
        next_id: 0,
        exchanges: Vec::new(),
    };
    let heartbeat = |w: &mut Wallet| {
        if tx.is_tron() {
            w.tron("/wallet/getnowblock", json!({}));
        } else {
            w.rpc("eth_blockNumber", json!([]));
        }
    };

    if wallet.rng.chance(0.4) {
        heartbeat(&mut wallet);
    }
    match tx {
        Transaction::EthTransfer => wallet.eth_send(false),
        Transaction::Erc20Transfer => wallet.eth_send(true),
        Transaction::EthPortfolio => wallet.eth_portfolio(),
        Transaction::EthLogScan => wallet.eth_log_scan(),
        Transaction::TrxTransfer => wallet.tron_send(false),
        Transaction::Trc20Transfer => wallet.tron_send(true),
        Transaction::TronAccount => wallet.tron_account(),
    }
    if wallet.rng.chance(0.3) {
        heartbeat(&mut wallet);
    }
    wallet.exchanges
}

struct Wallet<'a> {
    node: TestNode,
    rng: &'a mut Rng,
    next_id: u64,
    exchanges: Vec<Exchange>,
}

impl Wallet<'_> {
    fn post(&mut self, path: &'static str, body: Value, pause: f64) -> Value {
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: node.testnet.local\r\nUser-Agent: demo-wallet/0.1\r\n\
             Accept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.to_string().len()
        );
        let (reply, server_time) = self.node.handle(path, &body, self.rng);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nDate: Fri, 13 Nov 2026 12:00:00 GMT\r\n\
             Vary: Origin\r\nContent-Length: {}\r\n\r\n{reply}",
            reply.to_string().len()
        );
        let call = match &body {
            Value::Array(batch) => format!("batch of {}", batch.len()),
            single => single["method"].as_str().unwrap_or(path).to_owned(),
        };
        self.exchanges.push(Exchange {
            call,
            pause,
            request: request.len(),
            response: response.len(),
            server_time,
        });
        reply
    }

    fn quick_pause(&mut self) -> f64 {
        self.rng.range(0.002, 0.03)
    }

    fn rpc_body(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
    }

    fn rpc_after(&mut self, pause: f64, method: &str, params: Value) -> Value {
        let body = self.rpc_body(method, params);
        self.post("/", body, pause)["result"].take()
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let pause = self.quick_pause();
        self.rpc_after(pause, method, params)
    }

    fn tron(&mut self, path: &'static str, body: Value) -> Value {
        let pause = self.quick_pause();
        self.post(path, body, pause)
    }

    fn eth_send(&mut self, token: bool) {
        let from = hex(self.rng, 20);
        self.rpc("eth_chainId", json!([]));
        self.rpc("eth_getTransactionCount", json!([from, "pending"]));
        self.rpc("eth_getBlockByNumber", json!(["latest", false]));
        self.rpc("eth_maxPriorityFeePerGas", json!([]));

        let mut call = json!({"from": from, "to": hex(self.rng, 20)});
        if token {
            let calldata = format!(
                "0xa9059cbb{:0>64}{}",
                hex_raw(self.rng, 20),
                hex_raw(self.rng, 32)
            );
            self.rpc("eth_call", json!([{"to": call["to"], "data": format!("0x70a08231{:0>64}", &from[2..])}, "latest"]));
            call["data"] = json!(calldata);
        } else {
            call["value"] = json!(quantity(self.rng.next_u64() >> 8));
        }
        self.rpc("eth_estimateGas", json!([call]));

        let raw = hex(self.rng, if token { 177 } else { 114 });
        let hash = self.rpc("eth_sendRawTransaction", json!([raw]));
        for _ in 0..10 {
            let pause = self.rng.range(0.5, 2.0);
            if !self
                .rpc_after(pause, "eth_getTransactionReceipt", json!([hash]))
                .is_null()
            {
                break;
            }
        }
    }

    fn eth_portfolio(&mut self) {
        let owner = hex(self.rng, 20);
        let mut batch = vec![self.rpc_body("eth_getBalance", json!([owner, "latest"]))];
        for _ in 0..2 + self.rng.below(8) {
            let call =
                json!({"to": hex(self.rng, 20), "data": format!("0x70a08231{:0>64}", &owner[2..])});
            batch.push(self.rpc_body("eth_call", json!([call, "latest"])));
        }
        let pause = self.quick_pause();
        self.post("/", Value::Array(batch), pause);
    }

    fn eth_log_scan(&mut self) {
        let head = self.rpc("eth_blockNumber", json!([]));
        let head = u64::from_str_radix(head.as_str().unwrap_or("0x0").trim_start_matches("0x"), 16)
            .unwrap_or(0);
        let filter = json!({
            "fromBlock": quantity(head.saturating_sub(5000)), "toBlock": quantity(head),
            "address": hex(self.rng, 20), "topics": [TRANSFER_TOPIC, null, format!("0x{:0>64}", hex_raw(self.rng, 20))],
        });
        self.rpc("eth_getLogs", json!([filter]));
    }

    fn tron_send(&mut self, token: bool) {
        let owner = tron_address(self.rng);
        let mut tx = if token {
            let request = json!({
                "owner_address": owner, "contract_address": tron_address(self.rng),
                "function_selector": "transfer(address,uint256)", "parameter": hex_raw(self.rng, 64),
                "fee_limit": 100_000_000, "call_value": 0, "visible": true,
            });
            self.tron("/wallet/triggersmartcontract", request)["transaction"].take()
        } else {
            let request = json!({"owner_address": owner, "to_address": tron_address(self.rng),
                "amount": self.rng.below(1_000_000_000), "visible": true});
            self.tron("/wallet/createtransaction", request)
        };
        tx["signature"] = json!([hex_raw(self.rng, 65)]);
        let txid = tx["txID"].clone();
        self.tron("/wallet/broadcasttransaction", tx);

        for _ in 0..10 {
            let pause = self.rng.range(1.0, 3.0);
            let info = self.post(
                "/wallet/gettransactioninfobyid",
                json!({"value": txid, "visible": true}),
                pause,
            );
            if info.as_object().is_some_and(|o| !o.is_empty()) {
                break;
            }
        }
    }

    fn tron_account(&mut self) {
        let address = tron_address(self.rng);
        self.tron(
            "/wallet/getaccount",
            json!({"address": address, "visible": true}),
        );
        self.tron(
            "/wallet/getaccountresource",
            json!({"address": address, "visible": true}),
        );
        let balance_of = json!({
            "owner_address": address, "contract_address": tron_address(self.rng),
            "function_selector": "balanceOf(address)", "parameter": format!("{:0>64}", hex_raw(self.rng, 21)), "visible": true,
        });
        self.tron("/wallet/triggerconstantcontract", balance_of);
    }
}
