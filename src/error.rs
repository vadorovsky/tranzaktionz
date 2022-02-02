use rust_decimal::Decimal;
use thiserror::Error;

use crate::transaction::TransactionType;

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error(transparent)]
    CSV(#[from] csv::Error),

    #[error("client `{0}` not found")]
    ClientNotFound(u16),

    #[error("no funds available (requested {requested:?} from client {client:?} with {available:} available)")]
    NoFunds {
        client: u16,
        available: Decimal,
        requested: Decimal,
    },

    #[error("deposit/withdrawal transaction has to specify amount")]
    WithoutAmount,

    #[error("dispute/resolve transaction must not specify amount")]
    WithAmount,

    #[error("client's account locked")]
    ClientLocked,

    #[error("transaction not found")]
    TransactionNotFound(u32),

    #[error("invalid transaction type `{0:?}`, only deposit/withdrawal can be referred")]
    InvalidTxType(TransactionType),

    #[error("transaction is not dissputed, cannot resolve/chargeback")]
    TxNotDisputed(u32),
}
