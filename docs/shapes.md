# Allowed Solana shapes

The NEAR contract will only sign these messages (legacy, no ALTs, ≤ 1232 bytes).

| Shape | Signer path | Allowed in | Notes |
| --- | --- | --- | --- |
| A Refund | `trust/<id>/in/<pubkey>` | Open, Refunding | `System::Transfer` to that pubkey |
| B Sweep | `trust/<id>/in/<pubkey>` | Executing | Transfer ≤ remaining 1% cap to raise vault; remainder to owner |
| C DBC create-and-buy | raise + fee vaults | Executing after sweeps | SDK 2.0.0 `createPoolWithFirstBuy`; config pinned at `create_trust` |
| D Distribute | `trust/<id>/raise` | Distributing | ATA create-idempotent + SPL transfer, pro-rata |
| E Fee split (stretch) | `trust/<id>/fees` | Distributed | 70 / 20 / 10 to addresses fixed at create |

Day-1 crate implements the parser and Shape A (fixed destination + amount cap).
See `contracts/keyless-trust/src/solana` and `src/shapes/transfer.rs`.
