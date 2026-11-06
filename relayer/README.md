# Watcher and relayer

Long-running process for one NEAR testnet contract and Solana devnet. It lists trusts, reads deposit balances, and drives the flows the contract already allows: `report_deposit`, capped sweeps, fill fee, launch, pro-rata distribution, timeout, refund, and `release_pending`.

It uses the named watcher account and a local Solana fee-payer key. It does not invent destinations. It does not ask the contract to sign a shape the contract rejects. It stays on `@solana/web3.js` 1.99.0.

## Run

```bash
pnpm install --frozen-lockfile
pnpm relayer
# same process:
pnpm --filter @keyless-trust/relayer start
```

Missing `NEAR_WATCHER_ACCOUNT_ID`, a watcher key, or `SOLANA_RELAYER_KEYPAIR` prints the missing names and exits 1. It does not poll.

`SOLANA_SUBMIT` unset is dry-run. The loop still reads NEAR and Solana, logs each planned action (`trustId`, `state`, `action`), and sends no transaction. `request_*` locks a pending row for about 150 seconds and spends 0.01 NEAR, so dry-run does not call those methods either.

`SOLANA_SUBMIT=1` sends. Each tick does at most one relay per trust (sweep, fill fee, launch, distribute, or refund). Deposit reports run first, and a trust with a new report waits until the next tick for a relay. `GET /health` returns 200. Stop with SIGINT or SIGTERM.

Do not point this at mainnet. The process refuses an RPC URL that contains `mainnet` and a contract id that does not end in `.testnet`.

## Environment

| Variable | Role |
| --- | --- |
| `NEAR_CONTRACT_ID` | Contract account. Default `m10.keyless-trust-dev-54ef71.testnet`. `CONTRACT_ID` is the fallback. |
| `NEAR_RPC_URL` | Default `https://rpc.testnet.near.org`. |
| `NEAR_WATCHER_ACCOUNT_ID` | Watcher named at `create_trust`. Required. |
| `NEAR_WATCHER_PRIVATE_KEY` | `ed25519:` secret. Or set `NEAR_WATCHER_KEY_PATH` to a file containing that string or a JSON `private_key`. |
| `SOLANA_RPC_URL` | Default `https://api.devnet.solana.com`. |
| `SOLANA_RELAYER_KEYPAIR` | JSON secret-key file, a JSON byte array, or a base58 64-byte secret. `SOLANA_KEYPAIR_PATH` is the same file. |
| `POLL_MS` | Default `12000`. Minimum `1000`. |
| `SOLANA_SUBMIT` | `1` sends NEAR and Solana transactions. Anything else is dry-run. |
| `TRUST_ID` | Optional numeric filter. The one-shot scripts use the same variable. Unset it to watch every trust. |
| `PORT` or `HEALTH_PORT` | Health HTTP port. Default `8080`. `PORT` wins. |
| `HEALTH_DISABLE` | `1` skips the health server. |

Copy the names from `.env.example`. Do not commit the file you fill in.

The loop only relays a trust whose `watcher` equals `NEAR_WATCHER_ACCOUNT_ID`. `try_timeout` and `release_pending` are permissionless, so those still run for other trusts. Refund and distribute requests are permissionless on the contract, but confirm is watcher-only, so this process does not request them on someone else's trust.

## What a tick does

1. `list_trusts`, then contributor keys and pending rows from contract state (see gaps below).
2. For each contributor, read the Solana balance of the stored deposit address. `report_deposit` gets the lamports above reported minus swept and refunded, and a recent signature that is not `last_solana_sig` and was not already used. A balance with no unused signature is logged and skipped.
3. After the deadline, or after the execution timeout, call `try_timeout` once per process. The view already projects `Refunding` before the write lands.
4. Release a pending row about 150 seconds after it expires, before requesting another signature.
5. Refunding: exact `request_refund` from the deposit address, then exact `request_raise_refund` from the raise vault. Raising, Filled, and Executing can still refund overflow when the view says the amount is the full balance.
6. Filled or Executing: capped `request_sweep` into the stored raise address. A partial sweep must leave the deposit account empty or rent-exempt.
7. When `swept_total` equals `credited_total`: one exact `request_fill_fee` if the fee is unpaid, then `request_launch` with the fee omitted. `confirm_launch_outcome` takes no token balance. The quote is the one stored in the contract. Launch runs only when the stored curve matches the demo curve the builder quotes.
8. Launched: up to five `request_distribute` legs, then `confirm_transfer`, until claimable contributors are done.

Sweep, refund, raise-refund, fill fee, and distribute wait for the MPC signature, assemble a legacy transaction with the relayer as fee payer, submit, and confirm in the same tick. Logs include the NEAR transaction id and the Solana signature.

## Contract gaps

The contract was not changed. The loop works around these views:

- There is no `list_contributions`. Contributor keys are the `cv` index, the same prefix the web app reads.
- There is no `list_pending`. Pending rows are the `p` prefix. Values are borsh `PendingTransfer`. Keys are the prefix plus the borsh string, so they can be decoded.
- There is no `quote_first_buy` view. `minimum_amount_out` is the SDK quote of the demo curve. A stored curve that differs is logged and not launched.
- `get_trust` projects `Refunding` before `try_timeout` writes it. The process calls `try_timeout` once when it sees that condition.
- `report_deposit` adds lamports and requires a fresh Solana signature of length 64 to 128. The contract does not check that the signature matches the amount.
- A crash after Solana submit and before `confirm_*` is not journaled. The signature is in the log. The one-shot confirm scripts can finish that row.
- A partial `fill_fee_paid` cannot be completed. The contract requires one transfer equal to the full fee while `fill_fee_paid` is still below that fee. The loop skips a raise that is already part-way paid.
