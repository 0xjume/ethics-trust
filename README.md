# Keyless Trust

Community token raises whose Solana vaults no person or server holds.

A NEAR contract owns every Trust vault (per-contributor deposit, raise vault,
fee vault) through NEAR Chain Signatures (Ed25519, `domain_id` 1,
`v1.signer-prod.testnet`) and will only sign allowed Solana transaction shapes:
refund to the contributor, a capped sweep (`cap_bps` required at create), a
Meteora DBC create-and-buy into the pinned config (relayer pays pool/ATA
rent; every launch account is derived and pinned), pro-rata token
distribution, and (stretch) a 70/20/10 fee split.

NEAR HACKATHON 2026 · track `memecoins` · **testnet and Solana devnet only**.

## Problem

Today a “community raise” still means someone — a server, a multisig, a founder
wallet — can move the SOL. Keyless Trust removes that holder. The mechanic lives
in the NEAR contract, not in a backend somebody could switch off.

## How to run it

Needs Rust 1.93+, `wasm32-unknown-unknown`, Node 22, pnpm 10. No paid RPC, no
funded accounts for the default path.

```bash
pnpm install
cargo test --workspace
pnpm test
pnpm derive-addresses          # read-only NEAR testnet view
pnpm deposit-address           # path trust/<id>/in/<pubkey> → Solana address
pnpm verify-dbc-sdk            # read-only Solana devnet
pnpm dbc-tx-size               # builds txs, does not send
pnpm dev:app                   # Keyless Trust room at http://localhost:3000
```

The app reads NEAR testnet and Solana devnet through public RPC. Optional env
(see `.env.example`): `NEXT_PUBLIC_CONTRACT_ID` (default
`m10.keyless-trust-dev-54ef71.testnet`), `NEXT_PUBLIC_NEAR_RPC_URL`,
`NEXT_PUBLIC_SOLANA_RPC_URL`. There is no mainnet switch and no secret in the
client. Trust lists, detail, and Verify work without a wallet.

### Deploy on Vercel

Do not deploy from CI. When you import the repo in Vercel:

1. Framework preset: Next.js.
2. Root directory: `app`.
3. Include source files outside of the root directory: on (the app depends on `packages/ui`).
4. Install command: `cd .. && pnpm install --frozen-lockfile`.
5. Build command: `cd .. && pnpm --filter @keyless-trust/app build`.
6. Environment: the three `NEXT_PUBLIC_*` variables above. No keys.

`app/vercel.json` records those install and build commands for a project whose
root directory is `app`. `next.config.mjs` traces files from the repository
root so the workspace package is included in the serverless bundle. Pages are
client-rendered against public RPC, including `/trusts/[id]`, so this is a
Next server deploy rather than `output: "export"`.

The MPC transfer script is `relayer/src/near-signed-solana-transfer.ts`. Do not
run it here unless you set `EXECUTE_NEAR_SIGNED_TRANSFER=1` (and `SOLANA_SUBMIT=1`
to send). Measured numbers: `docs/spikes.md`.

## Proof it works

A NEAR-owned Solana vault signed a 0.001 SOL transfer on **devnet**. The
relayer paid the fee; the vault balance dropped by exactly 1 000 000 lamports.

- NEAR `sign`: [BnwGYCZTcZ41ntCUKEeRMmpwhXeuqxzRfyunSvJfDa4R](https://testnet.nearblocks.io/txns/BnwGYCZTcZ41ntCUKEeRMmpwhXeuqxzRfyunSvJfDa4R)
- Solana transfer: [32cEnng3NKyCv2VLQ47M2uXnVDp1LZH7B7rtWZtzHDP2QHz6yrEbFJw7kyWerMs1CYZ2vMoa3cgrcKmKkGNgbd9G](https://explorer.solana.com/tx/32cEnng3NKyCv2VLQ47M2uXnVDp1LZH7B7rtWZtzHDP2QHz6yrEbFJw7kyWerMs1CYZ2vMoa3cgrcKmKkGNgbd9G?cluster=devnet)

## Do X, see Y (judge steps)

1. **Do** `pnpm derive-addresses`. **See** `trust/1/raise` →
   `ed25519:B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj` (plan check) plus
   fees and contributor paths from `v1.signer-prod.testnet`.
2. **Do** `pnpm verify-dbc-sdk` then `pnpm dbc-tx-size`. **See**
   `sdkMatchesDevnet: true` for `@meteora-ag/dynamic-bonding-curve-sdk@2.0.0`,
   the DBC program executable on devnet, and legacy createConfig2 / create-and-buy
   messages under 1232 bytes (measured 533 and 811).
3. **Do** `cargo test -p keyless-trust`. **See** the parser reject an attacker
   destination or a fee-payer-as-source, plus cap overflow and pending settle.
4. **Do** open the Proof links above. **See** a NEAR `sign` and a Solana
   **devnet** transfer from the keyless vault, with the relayer as fee payer.
5. **Do** `pnpm deposit-address`. **See** `trust/1/in/<pubkey>` plus the
   `v1.signer-prod.testnet` derived Solana address (domain 1).
6. **Do** (human, testnet + devnet, new subaccount **m10**) the recipe in
   `docs/spikes.md`. **See** a DBC pool, then a pro-rata `TransferChecked`
   from the raise token ATA to each contributor's ATA. `claimable` drops
   only after the watcher `confirm_transfer`, and the trust moves to
   `Distributed` when every floor share is booked. Rounding dust stays in
   the raise ATA. The share basis is the contract's first-buy quote, not a
   watcher balance.

## NEAR tech used

- **Chain Signatures** — `derived_public_key` and `sign` on
  `v1.signer-prod.testnet`, `payload_v2: { Eddsa }`, `domain_id: 1`.
- **near-sdk 5.x** contract: Trust registry, state machine, Solana message
  parser, shape checkers, MPC promises + `#[private]` callbacks.
- **near-api-js v7** for the TypeScript relayer and scripts.

Public libraries only (listed here as baseline / allowed, Q-005): `near-sdk`,
`near-api-js`, `@solana/web3.js` v1, `@meteora-ag/dynamic-bonding-curve-sdk@2.0.0`,
`@meteora-ag/cp-amm-sdk@1.5.1`. Nothing from private `0xjume/ethics` or Sana Guard.

Ethics Launch is the existing product (Q-002). This entry is the NEAR-owned
vault mechanic. Patterns only; no copied modules.

## Trust assumptions

- **MPC network.** The signer set could collectively sign anything. That is the
  base assumption of Chain Signatures.
- **Watcher / relayer.** NEAR cannot read Solana. The watcher reports balances,
  landed sweeps, and expiry. A lying or offline watcher can **delay** the
  raise and can still mis-report **who credited how much** among registered
  contributors. It cannot set the share basis, and it cannot send funds to
  any address the contract did not fix (owner, raise vault, DBC pool,
  contributors, fixed fee destinations). Anyone can run a watcher.
  `confirm_transfer`, `confirm_launch_outcome`, `request_launch`,
  `request_sweep`, and `request_fill_fee` are watcher-only. Refunds stay
  permissionless (full amount only) so a contributor can exit if the watcher
  is offline; confirm still needs the named watcher. Distribution claims are
  permissionless. One paid claim locks only the contributors in that message
  (at most 5) for 150 s. `tokens_bought` is the contract's exact-in quote of
  `credited − fill fee` on the curve stored at `create_trust`, pinned as the
  swap's `minimum_amount_out` at `request_launch`. `confirm_launch_outcome`
  takes no balance. Sum of floor shares is required to be ≤ that basis, so
  claim order cannot strand a later contributor. Tokens the swap delivers
  above the quote stay in the raise ATA as dust. Destinations stay the
  register pubkey.
- **Sybil.** The 1% cap is per *wallet*. One person can split into many wallets.
  Good enough for a demo, not a fairness guarantee.
- **DBC config.** Created once; the Trust pins its pubkey. `feeClaimer` is
  the fee vault. `leftoverReceiver` is the raise vault. Distribution uses
  classic Tokenkeg (Shape C rejects Token-2022).

## Regulatory disclaimer

Devnet and testnet only. No real value. No returns promised. This is not an
offer of securities, a crowdfunding, or a collective investment scheme. Language
is escrow rules and refunds guaranteed by code. Not investment advice. Before
any mainnet use: legal advice in the operating jurisdictions.

## Related project by the same builder

The same lead also entered a separate, independent project, **Sana Guard**
(NEAR HACKATHON Q-015). The two repositories do not share source.

## Layout

See `AGENTS.md` and `docs/PLAN.md`. Design tokens: `docs/DESIGN.md`.
SDK pins: `docs/DECISIONS.md`. Spike numbers: `docs/spikes.md`.
