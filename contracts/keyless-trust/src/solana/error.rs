//! Parse and shape-check errors for Solana messages.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof,
    CompactU16Overflow,
    VersionedMessage,
    EmptyMessage,
    TooManyAccounts,
    TrailingBytes,
}

impl ParseError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnexpectedEof => "unexpected end of Solana message",
            Self::CompactU16Overflow => "compact-u16 overflow",
            Self::VersionedMessage => "versioned Solana message (v0/v1) is not allowed",
            Self::EmptyMessage => "empty Solana message",
            Self::TooManyAccounts => "too many account keys",
            Self::TrailingBytes => "trailing bytes after legacy message",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeError {
    Parse(ParseError),
    NoInstructions,
    UnexpectedProgram,
    ExtraInstruction,
    NotSystemTransfer,
    WrongDestination,
    WrongSource,
    AmountAboveCap,
    MissingAccounts,
    FeePayerNotSource,
    SourceNotSigner,
    DestinationNotWritable,
    WrongConfig,
    WrongMint,
    WrongParams,
    WrongBuyAmount,
    WrongMinOut,
    WrongFillFee,
    MissingLaunchIx,
    WrongAccount,
    WrongWrap,
    WrongAmount,
    WrongDecimals,
    WrongOwner,
    WrongTokenProgram,
    NotTransferChecked,
    AtaPayerNotRelayer,
    FeePayerNotRelayer,
    ExactData,
    WrongAccountCount,
    DuplicateContributor,
    TooManyTransfers,
    MessageTooLarge,
}

impl ShapeError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Parse(e) => e.as_str(),
            Self::NoInstructions => "message has no instructions",
            Self::UnexpectedProgram => "instruction targets a program not on the allow-list",
            Self::ExtraInstruction => "extra instruction beyond the allowed transfer shape",
            Self::NotSystemTransfer => "instruction is not SystemProgram::Transfer",
            Self::WrongDestination => "transfer destination is not the fixed owner",
            Self::WrongSource => "transfer source is not the required vault",
            Self::AmountAboveCap => "transfer amount exceeds the signed cap",
            Self::MissingAccounts => "transfer is missing from/to accounts",
            Self::FeePayerNotSource => "fee payer must be the vault being spent",
            Self::SourceNotSigner => "transfer source is not a required signer",
            Self::DestinationNotWritable => "transfer destination is not writable",
            Self::WrongConfig => "DBC config is not the pubkey pinned at create_trust",
            Self::WrongMint => "base mint is not the derived trust/<id>/mint key",
            Self::WrongParams => "name/symbol/uri do not match the trust config",
            Self::WrongBuyAmount => "first-buy amount_in is not raise total minus fill fee",
            Self::WrongMinOut => "minimum_amount_out is not the quoted first-buy output",
            Self::WrongFillFee => "fill fee transfer is missing or not raise → fee vault",
            Self::MissingLaunchIx => "launch message needs initialize_virtual_pool + swap",
            Self::WrongAccount => "launch instruction account does not match the derived pin",
            Self::WrongWrap => {
                "wSOL wrap must be raise → raise WSOL ATA for the first-buy amount, then SyncNative"
            }
            Self::WrongAmount => "transfer amount is not the remaining share",
            Self::WrongDecimals => "TransferChecked decimals are not 6",
            Self::WrongOwner => "destination owner is not the registered contributor",
            Self::WrongTokenProgram => {
                "distribution uses classic SPL Token (Tokenkeg), not Token-2022"
            }
            Self::NotTransferChecked => "instruction is not SPL Token TransferChecked",
            Self::AtaPayerNotRelayer => "ATA payer must be the relayer fee payer",
            Self::FeePayerNotRelayer => "fee payer must be the relayer, not raise/mint/fee",
            Self::ExactData => "instruction data length is not exact",
            Self::WrongAccountCount => "instruction account list length is not exact",
            Self::DuplicateContributor => "duplicate contributor in distribution",
            Self::TooManyTransfers => "too many distribution transfers",
            Self::MessageTooLarge => "message exceeds 1232 bytes",
        }
    }
}

impl From<ParseError> for ShapeError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}
