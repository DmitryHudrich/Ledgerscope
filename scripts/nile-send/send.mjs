import { TronWeb } from 'tronweb';

const env = (name, fallback) => process.env[name] ?? fallback;

const fullHost = env('NILE_FULLHOST', 'https://api.nile.trongrid.io');
const privateKey = env('PRIVATE_KEY');
const amountSun = Number(env('AMOUNT_SUN', '1000000'));
const count = Number(env('COUNT', '3'));
const delayMs = Number(env('DELAY_MS', '4000'));
const poll = env('POLL', 'true') !== 'false';

const proxy =
  process.env.HTTPS_PROXY ??
  process.env.https_proxy ??
  process.env.ALL_PROXY ??
  process.env.all_proxy ??
  null;

if (!privateKey) {
  console.error('PRIVATE_KEY is required: a funded Nile account (hex).');
  console.error('Fund one at https://nileex.io/join/getJoinPage');
  process.exit(1);
}

const tronWeb = new TronWeb({ fullHost, privateKey });
const to = env('TO', tronWeb.defaultAddress.base58);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

console.log(`nile-send -> ${fullHost}`);
console.log(`sender ${tronWeb.defaultAddress.base58}`);
console.log(`sending ${count} transfer(s) of ${amountSun} sun to ${to}`);
if (proxy) {
  console.log(`routing through proxy ${proxy}`);
} else {
  console.warn('no HTTPS_PROXY/ALL_PROXY set: traffic will NOT pass the observer.');
  console.warn('run as: HTTPS_PROXY=http://127.0.0.1:8443 node send.mjs   (or via proxychains)');
}

for (let i = 1; i <= count; i += 1) {
  try {
    const receipt = await tronWeb.trx.sendTransaction(to, amountSun);
    const txid = receipt?.txid ?? receipt?.transaction?.txID ?? null;
    console.log(`[${i}/${count}] broadcast ${txid ?? '(no txid)'}`);
    if (poll && txid) {
      for (let attempt = 0; attempt < 5; attempt += 1) {
        await sleep(2000);
        const info = await tronWeb.trx.getTransactionInfo(txid);
        if (info && Object.keys(info).length > 0) {
          console.log(`         mined in block ${info.blockNumber}`);
          break;
        }
      }
    }
  } catch (error) {
    console.error(`[${i}/${count}] failed: ${error?.message ?? error}`);
  }
  if (i < count) {
    await sleep(delayMs);
  }
}
