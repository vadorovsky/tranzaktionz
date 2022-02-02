use rust_decimal::Decimal;
use serde::Deserialize;

use crate::error::Error;

/// Type of transaction.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TransactionType {
    /// Credit to the client's account.
    Deposit,
    /// Debit to the client's account.
    Withdrawal,
    /// Claim that some other transaction was erroneus and should be reversed.
    Dispute,
    /// Resolution to a dispute, releasing the associated held funds.
    Resolve,
    /// Final state of a dispute, client reversing a transaction.
    Chargeback,
}

/// Deserialize Decimals from strings in CSV.
///
/// rust_decimal comes with a serde module, available through serde-with-str
/// feature, but it supports only fields of type `Decimal`, not
/// `Option<Decimal>`. Therefore we had to implement our own serializer for
/// `Option<Decimal>`.
mod rust_decimal_serde_str_option {
    use super::*;

    use rust_decimal::prelude::*;
    use serde::Deserializer;

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Option<Decimal>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        if s.trim().is_empty() {
            return Ok(None);
        }

        match Decimal::from_str(&s) {
            Ok(d) => Ok(Some(d)),
            Err(_) => Ok(None),
        }
    }
}

/// Off-chain transaction.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Transaction {
    #[serde(rename = "type")]
    pub(crate) tx_type: TransactionType,
    pub(crate) client: u16,
    pub(crate) tx: u32,
    #[serde(with = "rust_decimal_serde_str_option")]
    pub(crate) amount: Option<Decimal>,
    #[serde(skip)]
    disputed: bool,
}

impl Transaction {
    /// Create a new transaction.
    #[cfg(test)]
    pub(crate) fn new(
        tx_type: TransactionType,
        client: u16,
        tx: u32,
        amount: Option<Decimal>,
    ) -> Transaction {
        Transaction {
            tx_type: tx_type,
            client: client,
            tx: tx,
            amount: amount,
            disputed: false,
        }
    }

    /// Claim that the transaction was erroneus and should be reversed.
    pub(crate) fn dispute(&mut self) {
        self.disputed = true;
    }

    pub(crate) fn is_disputed(&self) -> bool {
        return self.disputed;
    }

    /// Gets an amount of the given transactionn or returns an error.
    pub(crate) fn get_amount_or_err(&self) -> Result<Decimal, Error> {
        let amount = self.amount.ok_or(Error::WithoutAmount)?;
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use csv::{ReaderBuilder, Trim};

    #[test]
    fn deserialize_tx_type() {
        let data = "\
type
withdrawal
deposit
resolve
chargeback
dispute
";
        let expected = vec![
            TransactionType::Withdrawal,
            TransactionType::Deposit,
            TransactionType::Resolve,
            TransactionType::Chargeback,
            TransactionType::Dispute,
        ];

        let rdr = ReaderBuilder::new()
            .delimiter(b',')
            .from_reader(data.as_bytes());
        let rdr_iter = rdr.into_deserialize();
        let testcase_iter = rdr_iter.zip(expected.iter());

        for (result, exp_record) in testcase_iter {
            let record: TransactionType =
                result.expect("Failed to retrieve a transaction type record");
            assert_eq!(record, *exp_record);
        }
    }

    #[test]
    fn deserialize_tx() {
        let data = "\
type,       client, tx, amount
deposit,         1,  1,    1.0
deposit,         2,  2,    2.0
deposit,         1,  3,    2.0
withdrawal,      1,  4,    1.5
withdrawal,      2,  5,    3.0
dispute,         1,  4,
resolve,         1,  4,
dispute,         2,  5,
chargeback,      2,  5,
";
        let expected = vec![
            Transaction::new(TransactionType::Deposit, 1, 1, Some(Decimal::new(1, 0))),
            Transaction::new(TransactionType::Deposit, 2, 2, Some(Decimal::new(2, 0))),
            Transaction::new(TransactionType::Deposit, 1, 3, Some(Decimal::new(2, 0))),
            Transaction::new(TransactionType::Withdrawal, 1, 4, Some(Decimal::new(15, 1))),
            Transaction::new(TransactionType::Withdrawal, 2, 5, Some(Decimal::new(3, 0))),
            Transaction::new(TransactionType::Dispute, 1, 4, None),
            Transaction::new(TransactionType::Resolve, 1, 4, None),
            Transaction::new(TransactionType::Dispute, 2, 5, None),
            Transaction::new(TransactionType::Chargeback, 2, 5, None),
        ];

        let rdr = ReaderBuilder::new()
            .delimiter(b',')
            .trim(Trim::All)
            .from_reader(data.as_bytes());
        let rdr_iter = rdr.into_deserialize();
        let testcase_iter = rdr_iter.zip(expected.iter());

        for (result, exp_record) in testcase_iter {
            let record: Transaction = result.expect("Failed to retrieve a transaction record");
            assert_eq!(record, *exp_record);
        }
    }
}
