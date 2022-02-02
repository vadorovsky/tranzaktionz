use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;

use crate::{
    error::Error,
    transaction::{Transaction, TransactionType},
};

/// Account balance of a client.
#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct Client {
    /// Client ID.
    client: u16,
    /// Available funds.
    available: Decimal,
    /// Funds held due to a dispute.
    held: Decimal,
    /// Total found (available and held).
    total: Decimal,
    /// If true, client cannot make any transactions.
    locked: bool,
    /// History of transactions (deposit, withdrawal, dispute).
    #[serde(skip)]
    transactions: BTreeMap<u32, Transaction>,
}

impl Client {
    /// Create a new client.
    pub(crate) fn new(id: u16) -> Client {
        Client {
            client: id,
            available: Decimal::new(0, 0),
            held: Decimal::new(0, 0),
            total: Decimal::new(0, 0),
            locked: false,
            transactions: BTreeMap::new(),
        }
    }

    /// Ensures that the client can make a transaction.
    ///
    /// When client's account is locked (which means they're not allowed to
    /// make a transaction)
    fn can_make_tx(&self) -> Result<(), Error> {
        if self.locked {
            return Err(Error::ClientLocked);
        }
        Ok(())
    }

    /// Saves a transaction to client's history.
    fn save_tx(&mut self, tx: Transaction) {
        self.transactions.insert(tx.tx, tx);
    }

    /// Credits the given amount to the client's account.
    fn deposit(&mut self, amount: Decimal) -> Result<(), Error> {
        self.can_make_tx()?;

        self.available += amount;
        self.total += amount;

        Ok(())
    }

    /// Debits the given amount from the client's account.
    fn withdraw(&mut self, amount: Decimal) -> Result<(), Error> {
        self.can_make_tx()?;

        let available = self.available - amount;
        if available < Decimal::new(0, 0) {
            return Err(Error::NoFunds {
                client: self.client,
                available: self.available,
                requested: amount,
            });
        }

        self.available = available;
        self.total -= amount;

        Ok(())
    }

    /// Gets the given (disputed) transaction.
    fn get_tx(&mut self, tx_id: u32) -> Result<&mut Transaction, Error> {
        let tx = self
            .transactions
            .get_mut(&tx_id)
            .ok_or(Error::TransactionNotFound(tx_id))?;
        Ok(tx)
    }

    /// Checks whether the given transaction can be referred by a dispute,
    /// resolve or chargeback type of transaction.
    ///
    /// That is allowed only if the referred transaction is a deposit or
    /// withdrawal.
    fn tx_is_referrable(&mut self, tx_id: u32) -> Result<(), Error> {
        let tx = self.get_tx(tx_id)?;
        match tx.tx_type {
            TransactionType::Deposit | TransactionType::Withdrawal => Ok(()),
            _ => Err(Error::InvalidTxType(tx.tx_type.clone())),
        }
    }

    /// Claim that the other transaction was erroneus and should be reversed.
    fn dispute(&mut self, tx_id: u32) -> Result<(), Error> {
        self.can_make_tx()?;
        self.tx_is_referrable(tx_id)?;

        let tx = self.get_tx(tx_id)?;
        tx.dispute();
        let amount = tx.get_amount_or_err()?;
        self.available -= amount;
        self.held += amount;

        Ok(())
    }

    /// Resolve a dispute, release the associated held funds.
    fn resolve(&mut self, tx_id: u32) -> Result<(), Error> {
        self.can_make_tx()?;
        self.tx_is_referrable(tx_id)?;

        let tx = self.get_tx(tx_id)?;
        if !tx.is_disputed() {
            return Err(Error::TxNotDisputed(tx_id));
        }
        let amount = self.get_tx(tx_id)?.get_amount_or_err()?;
        self.available += amount;
        self.held -= amount;

        Ok(())
    }

    /// Reverse a transaction and lock the client account. Final state of a
    /// dispute.
    fn chargeback(&mut self, tx_id: u32) -> Result<(), Error> {
        let tx = self.get_tx(tx_id)?;
        if !tx.is_disputed() {
            return Err(Error::TxNotDisputed(tx_id));
        }
        // NOTE: Not sure about the implementation here. In theory chargeback
        // should always just substract the held and total amounts, but not
        // sure if that should happen for charging back the withdrawaals as
        // well... For now, I'm leaving it as it is, always substracting.
        //
        // match tx.tx_type {
        //     // In case of deposit transactions, we need to simply substract the
        //     // disputed amount from held and total amount, since we are reverting
        //     // the credit.
        //     TransactionType::Deposit => {
        //         let amount = tx.get_amount_or_err()?;
        //         self.held -= amount;
        //         self.total -= amount;
        //     }
        //     // In case of withdrawal transactions, we need to add the disputed
        //     // amount to helf and total amount, since we are reverting the debit
        //     // (reverting the previous substractionnn) and we need to compensate
        //     // by giving the disputed amount back.
        //     TransactionType::Withdrawal => {
        //         let amount = tx.get_amount_or_err()?;
        //         self.held += amount;
        //         self.total += amount;
        //     }
        //     _ => {
        //         return Err(Error::InvalidTxType(tx.tx_type.clone()));
        //     }
        // }
        let amount = tx.get_amount_or_err()?;
        self.held -= amount;
        self.total -= amount;
        self.locked = true;

        Ok(())
    }

    /// Makes a transaction on the given client account.
    pub(crate) fn make_tx(&mut self, tx: Transaction) -> Result<(), Error> {
        self.can_make_tx()?;

        match tx.tx_type {
            TransactionType::Deposit => match tx.amount {
                Some(a) => {
                    self.deposit(a)?;
                    self.save_tx(tx);
                }
                None => return Err(Error::WithoutAmount),
            },
            TransactionType::Withdrawal => match tx.amount {
                Some(a) => {
                    self.withdraw(a)?;
                    self.save_tx(tx);
                }
                None => return Err(Error::WithoutAmount),
            },
            TransactionType::Dispute => match tx.amount {
                Some(_) => return Err(Error::WithAmount),
                None => self.dispute(tx.tx)?,
            },
            TransactionType::Resolve => match tx.amount {
                Some(_) => return Err(Error::WithAmount),
                None => self.resolve(tx.tx)?,
            },
            TransactionType::Chargeback => match tx.amount {
                Some(_) => return Err(Error::WithAmount),
                None => self.chargeback(tx.tx)?,
            },
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use csv::WriterBuilder;

    #[test]
    fn serialize_client() {
        let clients = vec![
            Client {
                client: 1,
                available: Decimal::new(15, 1),
                held: Decimal::new(0, 0),
                total: Decimal::new(15, 1),
                locked: false,
                transactions: BTreeMap::new(),
            },
            Client {
                client: 2,
                available: Decimal::new(2, 0),
                held: Decimal::new(0, 0),
                total: Decimal::new(2, 0),
                locked: false,
                transactions: BTreeMap::new(),
            },
        ];

        let mut wtr = WriterBuilder::new().from_writer(vec![]);
        for client in clients.iter() {
            wtr.serialize(client).expect("Failed to serialize client");
        }

        let data = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
        assert_eq!(
            data,
            "\
client,available,held,total,locked
1,1.5,0,1.5,false
2,2,0,2,false
"
        )
    }

    #[test]
    fn test_can_make_tx() {
        let mut c = Client::new(1);

        c.can_make_tx()
            .expect("Expected client account to not be locked");

        c.locked = true;

        c.can_make_tx()
            .expect_err("Expected client account to be locked");
    }

    #[test]
    fn test_save_tx() {
        let mut c = Client::new(1);

        let tx1 = Transaction::new(TransactionType::Deposit, 1, 1, Some(Decimal::new(1, 0)));
        let tx2 = Transaction::new(TransactionType::Withdrawal, 1, 2, Some(Decimal::new(5, 1)));

        c.save_tx(tx1.clone());
        c.save_tx(tx2.clone());

        assert_eq!(
            *c.transactions.get(&1).expect("Failed to get a transaction"),
            tx1
        );
        assert_eq!(
            *c.transactions.get(&2).expect("Failed to get a transaction"),
            tx2
        );
    }

    #[test]
    fn test_deposit() {
        let mut c = Client::new(1);

        // Deposit 2.5
        c.deposit(Decimal::new(25, 1)).expect("Failed to deposit");
        assert_eq!(c.available, Decimal::new(25, 1));
        assert_eq!(c.held, Decimal::new(0, 0));
        assert_eq!(c.total, Decimal::new(25, 1));

        // Deposit 1.94 (2.5 + 1.94 = 4.44)
        c.deposit(Decimal::new(194, 2)).expect("Failed to deposit");
        assert_eq!(c.available, Decimal::new(444, 2));
        assert_eq!(c.held, Decimal::new(0, 0));
        assert_eq!(c.total, Decimal::new(444, 2));

        // Deposit 5.8432 (= 10.2832)
        c.deposit(Decimal::new(58432, 4))
            .expect("Failed to deposit");
        assert_eq!(c.available, Decimal::new(102832, 4));
        assert_eq!(c.held, Decimal::new(0, 0));
        assert_eq!(c.total, Decimal::new(102832, 4));
    }

    #[test]
    fn test_withdraw() {
        let mut c = Client::new(1);

        // Try to withdraw without funds available.
        c.withdraw(Decimal::new(42069, 2))
            .expect_err("Expected client account not to have funds");

        // Deposit before withdrawing.
        c.deposit(Decimal::new(420, 0)).expect("Failed to deposit");
        c.withdraw(Decimal::new(69, 0)).expect("Failed to deposit");

        // Try to withdraw more than available.
        c.withdraw(Decimal::new(9001, 0))
            .expect_err("Expected client account to have insufficient funds");

        assert_eq!(c.available, Decimal::new(351, 0));
    }

    #[test]
    fn test_get_tx() {
        let mut c = Client::new(1);

        c.save_tx(Transaction::new(
            TransactionType::Deposit,
            1,
            1,
            Some(Decimal::new(69, 0)),
        ));

        let tx = c
            .transactions
            .get(&1)
            .expect("Failed to geet a transaction");

        assert_eq!(tx.tx_type, TransactionType::Deposit);
        assert_eq!(tx.client, 1);
        assert_eq!(tx.tx, 1);
        assert_eq!(tx.amount, Some(Decimal::new(69, 0)));
    }

    #[test]
    fn test_tx_is_referrable() {
        let mut c = Client::new(1);

        c.save_tx(Transaction::new(
            TransactionType::Deposit,
            1,
            1,
            Some(Decimal::new(15, 1)),
        ));
        c.save_tx(Transaction::new(
            TransactionType::Withdrawal,
            1,
            2,
            Some(Decimal::new(25, 1)),
        ));
        c.save_tx(Transaction::new(TransactionType::Dispute, 1, 3, None));
        c.save_tx(Transaction::new(TransactionType::Resolve, 1, 4, None));
        c.save_tx(Transaction::new(TransactionType::Chargeback, 1, 5, None));

        c.tx_is_referrable(1).expect("Expected tx to be referrable");
        c.tx_is_referrable(2).expect("Expected tx to be referrable");

        c.tx_is_referrable(3)
            .expect_err("Expected tx to be not referrable");
        c.tx_is_referrable(4)
            .expect_err("Expected tx to be not referrable");
        c.tx_is_referrable(5)
            .expect_err("Expected tx to be not referrable");
    }

    #[test]
    fn test_dispute_resolve() {
        // Dispute and resolve the only first deposit.
        {
            let mut c = Client::new(1);

            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                1,
                1,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");

            c.resolve(1)
                .expect_err("Expected resolving a transaction not under dispute to fail");

            c.dispute(1).expect("Failed to dispute transaction");

            assert_eq!(c.available, Decimal::new(0, 0));
            assert_eq!(c.held, Decimal::new(25, 1));
            assert_eq!(c.total, Decimal::new(25, 1));

            c.resolve(1).expect("Failed to resolve transaction");

            assert_eq!(c.available, Decimal::new(25, 1));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(25, 1));
        }
        // Dispute and resolve the 2nd deposit.
        {
            let mut c = Client::new(2);

            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                2,
                1,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");
            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                2,
                2,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");

            assert_eq!(c.available, Decimal::new(5, 0));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(5, 0));

            c.dispute(2).expect("Failed to dispute transaction");

            assert_eq!(c.available, Decimal::new(25, 1));
            assert_eq!(c.held, Decimal::new(25, 1));
            assert_eq!(c.total, Decimal::new(5, 0));

            c.resolve(2).expect("Failed to resolve transaction");

            assert_eq!(c.available, Decimal::new(5, 0));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(5, 0));
        }
        // Dispute and resolve the withdrawal.
        {
            let mut c = Client::new(3);

            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                3,
                1,
                Some(Decimal::new(5, 0)),
            ))
            .expect("Failed to make a transaction");
            c.make_tx(Transaction::new(
                TransactionType::Withdrawal,
                3,
                2,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");

            assert_eq!(c.available, Decimal::new(25, 1));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(25, 1));

            c.dispute(2).expect("Failed to dispute transaction");

            assert_eq!(c.available, Decimal::new(0, 0));
            assert_eq!(c.held, Decimal::new(25, 1));
            assert_eq!(c.total, Decimal::new(25, 1));

            c.resolve(2).expect("Failed to resolve transaction");

            assert_eq!(c.available, Decimal::new(25, 1));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(25, 1));
        }
    }

    #[test]
    fn test_dispute_chargeback() {
        // Dispute and chargeback the only first deposit.
        {
            let mut c = Client::new(1);

            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                1,
                1,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");

            c.chargeback(1)
                .expect_err("Expected chargeback of a transaction not under dispute to fail");

            c.dispute(1).expect("Failed to dispute transaction");

            assert_eq!(c.available, Decimal::new(0, 0));
            assert_eq!(c.held, Decimal::new(25, 1));
            assert_eq!(c.total, Decimal::new(25, 1));

            c.chargeback(1).expect("Failed to resolve transaction");

            assert_eq!(c.available, Decimal::new(0, 0));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(0, 0));
        }
        // Dispute and chargeback the 2nd deposit.
        {
            let mut c = Client::new(2);

            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                2,
                1,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");
            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                2,
                2,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");

            assert_eq!(c.available, Decimal::new(5, 0));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(5, 0));

            c.dispute(2).expect("Failed to dispute transaction");

            assert_eq!(c.available, Decimal::new(25, 1));
            assert_eq!(c.held, Decimal::new(25, 1));
            assert_eq!(c.total, Decimal::new(5, 0));

            c.chargeback(2).expect("Failed to resolve transaction");

            assert_eq!(c.available, Decimal::new(25, 1));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(25, 1));
        }
        // Dispute and chargeback the withdrawal.
        {
            let mut c = Client::new(3);

            c.make_tx(Transaction::new(
                TransactionType::Deposit,
                3,
                1,
                Some(Decimal::new(5, 0)),
            ))
            .expect("Failed to make a transaction");
            c.make_tx(Transaction::new(
                TransactionType::Withdrawal,
                3,
                2,
                Some(Decimal::new(25, 1)),
            ))
            .expect("Failed to make a transaction");

            assert_eq!(c.available, Decimal::new(25, 1));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(25, 1));

            c.dispute(2).expect("Failed to dispute transaction");

            assert_eq!(c.available, Decimal::new(0, 0));
            assert_eq!(c.held, Decimal::new(25, 1));
            assert_eq!(c.total, Decimal::new(25, 1));

            c.chargeback(2).expect("Failed to resolve transaction");

            // assert_eq!(c.available, Decimal::new(0, 0));
            // assert_eq!(c.held, Decimal::new(5, 0));
            // assert_eq!(c.total, Decimal::new(5, 0));

            assert_eq!(c.available, Decimal::new(0, 0));
            assert_eq!(c.held, Decimal::new(0, 0));
            assert_eq!(c.total, Decimal::new(0, 0));
        }
    }

    #[test]
    fn test_make_tx() {
        let mut c = Client::new(1);

        // Make some deposits.
        c.make_tx(Transaction::new(
            TransactionType::Deposit,
            1,
            1,
            Some(Decimal::new(26, 1)),
        ))
        .expect("Failed to make a transaction");
        c.make_tx(Transaction::new(
            TransactionType::Deposit,
            1,
            2,
            Some(Decimal::new(53, 1)),
        ))
        .expect("Failed to make a transaction");
        c.make_tx(Transaction::new(
            TransactionType::Deposit,
            1,
            3,
            Some(Decimal::new(41, 1)),
        ))
        .expect("Failed to make a transaction");

        // Try to make a faulty deposit without amount.
        c.make_tx(Transaction::new(TransactionType::Deposit, 1, 4, None))
            .expect_err("Expected deposit without amount to fail");

        // Make a withdrawal.
        c.make_tx(Transaction::new(
            TransactionType::Withdrawal,
            1,
            5,
            Some(Decimal::new(13, 1)),
        ))
        .expect("Failed to make a transaction");
        // Try to make faulty withdrawals.
        c.make_tx(Transaction::new(TransactionType::Withdrawal, 1, 6, None))
            .expect_err("Expected withdrawal without amount to fail");
        c.make_tx(Transaction::new(
            TransactionType::Withdrawal,
            1,
            7,
            Some(Decimal::new(9001, 0)),
        ))
        .expect_err("Expected withdrawal to fail due to insufficient funds");

        // Try to make a faulty dispute.
        c.make_tx(Transaction::new(
            TransactionType::Dispute,
            1,
            1,
            Some(Decimal::new(1, 0)),
        ))
        .expect_err("Expected dispute with provided amount to fail");
        // Make correct disputes.
        c.make_tx(Transaction::new(TransactionType::Dispute, 1, 1, None))
            .expect("Failed to make a transaction");
        c.make_tx(Transaction::new(TransactionType::Dispute, 1, 2, None))
            .expect("Failed to make a transaction");

        // Try to make a faulty resolve transaction.
        c.make_tx(Transaction::new(
            TransactionType::Resolve,
            1,
            1,
            Some(Decimal::new(26, 1)),
        ))
        .expect_err("Expected resolve transaction with provided amounnt to fail");
        // Make a correct resolve transaction.
        c.make_tx(Transaction::new(TransactionType::Resolve, 1, 1, None))
            .expect("Failed to make a transaction");

        // Try to make a faulty chargeback transaction.
        c.make_tx(Transaction::new(
            TransactionType::Chargeback,
            1,
            2,
            Some(Decimal::new(26, 1)),
        ))
        .expect_err("Expected chargeback with provided amount to fail");
        // Make a correct chargeback transaction.
        c.make_tx(Transaction::new(TransactionType::Chargeback, 1, 2, None))
            .expect("Failed to make a transaction");
    }
}
