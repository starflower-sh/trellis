use clap::Parser;
use postgresql_embedded::{PostgreSQL, Result, SettingsBuilder};
use serde::Deserialize;

use crate::create::handle_create;

pub mod setup_queries;
pub mod apply;
pub mod create;

#[derive(Deserialize)]
struct Config {
    superuser: Superuser,
    roles: Vec<Role>,
    databases: Vec<Database>,
}

#[derive(Deserialize)]
struct Superuser {
    name: String,
    password: String,
}

#[derive(Deserialize)]
struct Role {
    name: String,
    password: String,
    login: bool,
}

#[derive(Deserialize)]
struct Database {
    name: String,
    owner: String,
    schema_dump_file: String,
    migrations_dir: String,
    schemas: Vec<Schema>,
}

#[derive(Deserialize)]
struct Schema {
    name: String,
    owner: String,
}

//TODO: Make it only serve if serve is true - otherwise it should connect to an existing db
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'a', long, conflicts_with = "create")]
    apply: bool,

    #[arg(short = 'c', long, conflicts_with = "apply")]
    create: bool,

    #[arg(short = 'm', long)]
    migration: bool,

    #[arg(short = 'C', long)]
    config: bool,

    #[arg(short = 's', long)]
    serve: bool,

    #[arg(
        short = 't',
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        requires = "serve"
    )]
    temporary_db: bool,

    #[arg(short = 'S', long)]
    schema: bool,

    #[arg(short = 'd', long, default_value = "")]
    description: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.create {
        let _ = handle_create(&args)?;
        return Ok(());
    }

    let init_db = "postgres";
    let config_file = "./config.yaml";
    let data_dir = "./data";
    let host = "127.0.0.1";
    let port = 5432;

    let yaml_str = std::fs::read_to_string(config_file)?;
    let config: Config = serde_saphyr::from_str(&yaml_str).unwrap(); // TODO: Remove unwrap

    let settings = SettingsBuilder::new()
        .host(host)
        .port(port)
        .username(&config.superuser.name)
        .password(&config.superuser.password)
        .data_dir(data_dir)
        .temporary(args.temporary_db)
        .config("max_connections", "100")
        .build();

    let mut postgresql = PostgreSQL::new(settings);
    postgresql.setup().await?;
    postgresql.start().await?;

    let main_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgresql.settings().url(init_db))
        .await?;

    if args.apply {
        apply::handle_apply(
            &postgresql,
            &main_pool,
            &config,
            &args,
            host,
            port,
        )
        .await?;
    }

    tokio::signal::ctrl_c().await?;

    println!("Gracefully shutting down");
    main_pool.close().await;
    postgresql.stop().await
}
