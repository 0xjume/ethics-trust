# Allowed Solana shapes

The NEAR contract will only sign these messages (legacy, no ALTs, ≤ 1232 bytes).

| Shape | Signer path | Allowed in | Notes |
| --- | --- | --- | --- |
| A Refund | `trust/<id>/in/<pubkey>` | Raising / Filled / Executing (overflow); Refunding (unswept) | Source = stored deposit address; dest = contributor pubkey; **full refundable only** (or dust close-out). Permissionless for liveness. |
| A2 Raise refund | `trust/<id>/raise` | Refunding | Source = raise vault; dest = contributor; **full** raise-refundable only |
| B Sweep | `trust/<id>/in/<pubkey>` | Filled, Executing | Watcher-only. Source = stored deposit; dest = stored raise vault; ≤ credited |
| C DBC create-and-buy | `trust/<id>/raise` + `trust/<id>/mint` | Executing after sweeps settled | Watcher-only sign. Swap is 15 IDL accounts (referral `None` = DBC program at 12) + optional instructions sysvar; ExactIn only; relayer is the only init/ATA payer |
| D Distribute | `trust/<id>/raise` | Launched | ATA create-idempotent + SPL transfer, pro-rata |
| E Fee split (stretch) | `trust/<id>/fees` | after D | 70 / 20 / 10 to addresses fixed at create |

The crate implements the parser plus Shapes A, A2, B, and C. Relayer-paid fee
payer is accepted (`allow_relayer_fee_payer`): account 0 is the relayer, the
vault still signs. Vault-paid (fee payer = source) still passes. Shape C
rejects a tampered config/mint/amount, extra raise transfer, redirected swap
output ATA, short account lists, unknown programs, or a versioned/ALT
message. `request_distribute` (Shape D) is a Milestone 4 stub.

See `contracts/keyless-trust/src/solana`, `src/shapes/transfer.rs`, and
`src/shapes/dbc.rs`.
