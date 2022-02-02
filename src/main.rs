use std::{collections::BTreeMap, io, path::Path};

use clap::Parser;
use csv::{ReaderBuilder, Trim, WriterBuilder};

mod client;
mod error;
mod transaction;

use client::Client;
use error::Error;
use transaction::Transaction;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    /// File with CSV series of transactions
    #[clap()]
    file: String,
}

fn process_transactions<P: AsRef<Path>>(file: P) -> Result<(), Error> {
    let mut clients_map: BTreeMap<u16, Client> = BTreeMap::new();

    let rdr = ReaderBuilder::new()
        .delimiter(b',')
        .trim(Trim::All)
        .from_path(file)?;
    for result in rdr.into_deserialize() {
        let tx: Transaction = result?;

        clients_map
            .entry(tx.client)
            .or_insert(Client::new(tx.client));

        let client = clients_map
            .get_mut(&tx.client)
            .ok_or(error::Error::ClientNotFound(tx.client))?;

        if let Err(e) = client.make_tx(tx) {
            match e {
                // Those errors can be ignored. We can proceed with next
                // transactions.
                Error::NoFunds { .. } | Error::TransactionNotFound(_) | Error::TxNotDisputed(_) => {
                }
                _ => return Err(e),
            }
        }
    }

    let mut wtr = WriterBuilder::new().from_writer(io::stdout());
    for (_, client) in clients_map.iter() {
        wtr.serialize(client)?;
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    process_transactions(args.file)?;

    Ok(())
}
