# Allowed Solana shapes

The NEAR contract will only sign these messages (legacy, no ALTs, ≤ 1232 bytes).

| Shape | Signer path | Allowed in | Notes |
| --- | --- | --- | --- |
| A Refund | `trust/<id>/in/<pubkey>` | Raising (overflow only), Refunding | `System::Transfer` to that pubkey only |
| B Sweep | `trust/<id>/in/<pubkey>` | Filled, Executing | Transfer ≤ credited (already ≤ 1% cap) to raise vault |
| C DBC create-and-buy | raise + fee vaults | Executing after sweeps | SDK 2.0.0 `createPoolWithFirstBuy`; config pinned at `create_trust` |
| D Distribute | `trust/<id>/raise` | Launched | ATA create-idempotent + SPL transfer, pro-rata |
| E Fee split (stretch) | `trust/<id>/fees` | after D | 70 / 20 / 10 to addresses fixed at create |

The crate implements the parser plus Shapes A and B. Relayer-paid fee payer is
accepted (`allow_relayer_fee_payer`): account 0 is the relayer, the deposit
address still signs the System transfer. Vault-paid (fee payer = source) still
passes. Any other program id, extra ix, attacker destination, or amount above
the signed cap is rejected.

See `contracts/keyless-trust/src/solana` and `src/shapes/transfer.rs`.
