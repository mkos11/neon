use anyhow::*;
use clap::{App, Arg, ArgMatches};
use std::str::FromStr;
use wal_craft::*;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("wal_craft=info"))
        .init();
    let type_arg = &Arg::new("type")
        .takes_value(true)
        .help("Type of WAL to craft")
        .possible_values([
            Simple::NAME,
            LastWalRecordXlogSwitch::NAME,
            LastWalRecordXlogSwitchEndsOnPageBoundary::NAME,
            WalRecordCrossingSegmentFollowedBySmallOne::NAME,
            LastWalRecordCrossingSegment::NAME,
        ])
        .required(true);
    let arg_matches = App::new("Postgres WAL crafter")
        .about("Crafts Postgres databases with specific WAL properties")
        .subcommand(
            App::new("print-postgres-config")
                .about("Print the configuration required for PostgreSQL server before running this script")
        )
        .subcommand(
            App::new("with-initdb")
                .about("Craft WAL in a new data directory first initialized with initdb")
                .arg(type_arg)
                .arg(
                    Arg::new("datadir")
                        .takes_value(true)
                        .help("Data directory for the Postgres server")
                        .required(true)
                )
                .arg(
                    Arg::new("pg-distrib-dir")
                        .long("pg-distrib-dir")
                        .takes_value(true)
                        .help("Directory with Postgres distributions (bin and lib directories, e.g. pg_install containing subpath `v14/bin/postgresql`)")
                        .default_value("/usr/local")
                )
                .arg(
                    Arg::new("pg-version")
                    .long("pg-version")
                    .help("Postgres version to use for the initial tenant")
                    .required(true)
                    .takes_value(true)
                )
        )
        .subcommand(
            App::new("in-existing")
                .about("Craft WAL at an existing recently created Postgres database. Note that server may append new WAL entries on shutdown.")
                .arg(type_arg)
                .arg(
                    Arg::new("connection")
                        .takes_value(true)
                        .help("Connection string to the Postgres database to populate")
                        .required(true)
                )
        )
        .get_matches();

    let wal_craft = |arg_matches: &ArgMatches, client| {
        let (intermediate_lsns, end_of_wal_lsn) = match arg_matches.value_of("type").unwrap() {
            Simple::NAME => Simple::craft(client)?,
            LastWalRecordXlogSwitch::NAME => LastWalRecordXlogSwitch::craft(client)?,
            LastWalRecordXlogSwitchEndsOnPageBoundary::NAME => {
                LastWalRecordXlogSwitchEndsOnPageBoundary::craft(client)?
            }
            WalRecordCrossingSegmentFollowedBySmallOne::NAME => {
                WalRecordCrossingSegmentFollowedBySmallOne::craft(client)?
            }
            LastWalRecordCrossingSegment::NAME => LastWalRecordCrossingSegment::craft(client)?,
            a => panic!("Unknown --type argument: {}", a),
        };
        for lsn in intermediate_lsns {
            println!("intermediate_lsn = {}", lsn);
        }
        println!("end_of_wal = {}", end_of_wal_lsn);
        Ok(())
    };

    match arg_matches.subcommand() {
        None => panic!("No subcommand provided"),
        Some(("print-postgres-config", _)) => {
            for cfg in REQUIRED_POSTGRES_CONFIG.iter() {
                println!("{}", cfg);
            }
            Ok(())
        }

        Some(("with-initdb", arg_matches)) => {
            let cfg = Conf {
                pg_version: arg_matches
                    .value_of("pg-version")
                    .unwrap()
                    .parse::<u32>()
                    .context("Failed to parse postgres version from the argument string")?,
                pg_distrib_dir: arg_matches.value_of("pg-distrib-dir").unwrap().into(),
                datadir: arg_matches.value_of("datadir").unwrap().into(),
            };
            cfg.initdb()?;
            let srv = cfg.start_server()?;
            wal_craft(arg_matches, &mut srv.connect_with_timeout()?)?;
            srv.kill();
            Ok(())
        }
        Some(("in-existing", arg_matches)) => wal_craft(
            arg_matches,
            &mut postgres::Config::from_str(arg_matches.value_of("connection").unwrap())?
                .connect(postgres::NoTls)?,
        ),
        Some(_) => panic!("Unknown subcommand"),
    }
}
