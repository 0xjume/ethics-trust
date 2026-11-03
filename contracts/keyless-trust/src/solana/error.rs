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
    AmountAboveCap,
    MissingAccounts,
    FeePayerNotSource,
    SourceNotSigner,
    DestinationNotWritable,
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
            Self::AmountAboveCap => "transfer amount exceeds the signed cap",
            Self::MissingAccounts => "transfer is missing from/to accounts",
            Self::FeePayerNotSource => "fee payer must be the vault being spent",
            Self::SourceNotSigner => "transfer source is not a required signer",
            Self::DestinationNotWritable => "transfer destination is not writable",
        }
    }
}

impl From<ParseError> for ShapeError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}
