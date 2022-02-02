# tranzaktionz

A simple CLI tool which analyzes series of off-chain transactions from a CSV and
output the state of clients accounts as a CSV.

## Usage

tranzaktionz takes a file with transactions as a positional argument and prints
the result to stdout. Therefore it can be used in the following way:

```bash
cargo run -- transactions.csv > accounts.csv
```

This repository contains example CSV input files in *test/* directory. Using
one of them is also possible:

```bash
cargo run -- tests/example1.csv
```

## Format

### Input

Input consists of the following columns:

* type (string)
* client (u16)
* tx (u32)
* amount (decimal)

Example:

```
type,       client, tx, amount
deposit,         1,  1,    1.0
deposit,         2,  2,    2.0
deposit,         1,  3,    2.0
withdrawal,      1,  4,    1.5
withdrawal,      2,  5,    3.0
```

### Output

Output consists of the following columns:

* client (u16)
* available (decimal) - funds that are available for trading, staking, withdrawal etc.
* held (decimal) - funds that are held for dispute
* total (decimal) - total funds that are available or held
* locked (bool) - information whether the account is locked (due to a chargeback)

Example:

```
client,available,held,total,locked
1,1.5,0,1.5,false
2,2.0,0,2.0,false
```

## Types of transaction

* **Deposit** - credit to the client's account
* **Withdrawal** - debit to the client's asset account
* **Dispute** - claim that a transactionn should be reversed; it's not getting
  reversed yet, but disputed amount is substracted from available funds and
  held
* **Resolve** - resolution of a dispute, releasing the held funds and addinng
  them to available funds
* **Chargeback** - final state of a dispute, reversing a transation; held an
  total funds decrease bby amount previously disputed

## Testing

tranzaktionz comes with unit and integration tests which can be executed with:

```bash
cargo test
```

Integration tests from *test/* directory make sure that tranzaktionz is able to
parse the input correctly and process given transactions. That directory includes
also sample CSV files.

However, unit tests included in *client.rs* and *transaction.rs* are more
focused on finding errors for each type of transaction.

## Serialization

Serialization and deserialization is done with [csv](https://crates.io/crates/csv)
and [serde](https://crates.io/crates/serde) crates. Input and output series are
represented by `Transaction` and `Client` structs.

## Errors

Errors are handled with [anyhow](https://crates.io/crates/anyhow) and
[thiserror](https://crates.io/crates/thiserror) crates. Anyhow is used in the
`main` function, thiserror is used to define concrete errors in `error.rs`,
which are used everywhere else - especially in `client.rs` and `transaction.rs`.

The program shouldn't panic, `unwrap()` is used only in tests - every error
should be handled gracefully with `?`.
