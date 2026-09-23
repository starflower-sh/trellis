#![forbid(unsafe_code)]

use clap::{Parser, ValueEnum};
use postgresql_embedded::{PostgreSQL, Result, SettingsBuilder, VersionReq};
use serde::Deserialize;
use colored::Colorize;

use crate::create::handle_create;

pub mod setup_queries;
pub mod apply;
pub mod test;
pub mod create;

#[derive(Deserialize)]
struct Config {
    version: String,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MigrationAction {
    Up,
    Rollback,
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'a', long, conflicts_with = "create", conflicts_with="test")]
    apply: bool,

    #[arg(short = 'c', long, conflicts_with = "apply", conflicts_with="test")]
    create: bool,

    #[arg(short = 't', long, conflicts_with = "apply", conflicts_with="create")]
    test: bool,

    #[arg(
        short = 'm',
        long,
        value_enum,
        num_args = 0..=1,
        default_missing_value = "up"
    )]
    migration: Option<MigrationAction>,

    #[arg(short = 'C', long)]
    config: bool,

    #[arg(short = 's', long)]
    serve: bool,

    #[arg(short = 'T', long, requires="serve")]
    temporary_db: bool,

    #[arg(short = 'S', long)]
    schema: bool,

    #[arg(short = 'd', long, default_value = "")]
    description: String,

    #[arg(long, default_value_t = 5432)]
    port: u16,

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value = "postgres")]
    init_db: String,

    #[arg(long, default_value = "./trellis.yaml")]
    config_file: String,

    #[arg(long, default_value = "./data")]
    pgdata: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let temp_db = args.temporary_db || args.test;
    let serve = args.serve || args.test;

    if args.create {
        let _ = handle_create(&args)?;
        return Ok(());
    }

    let yaml_str = match std::fs::read_to_string(&args.config_file) {
        Ok(yaml_str) => yaml_str,
        Err(err) => {
            eprintln!("Failed to read config file got error:\n{}", err);
            std::process::exit(1);
        }
    };
    let config: Config = match serde_saphyr::from_str(&yaml_str) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Failed to parse config file got error:\n{}", err);
            std::process::exit(1);
        }
    };

    let mut postgresql = if serve {
        println!(
            "Starting a {} Postgres server",
            if temp_db { "temporary" } else { "persistent" }.cyan(),
        );
        let pg_version = VersionReq::parse(&config.version)?;

        let settings = SettingsBuilder::new()
            .host(&args.host)
            .port(args.port)
            .username(&config.superuser.name)
            .password(&config.superuser.password)
            .data_dir(&args.pgdata)
            .temporary(temp_db)
            .version(pg_version)
            .config("max_connections", "100")
            .build();

        let mut postgresql = PostgreSQL::new(settings);
        postgresql.setup().await?;
        postgresql.start().await?;
        Some(postgresql)
    } else {
        None
    };

    //TODO: Needs to handle SSL
    let connection_options = sqlx::postgres::PgConnectOptions::new()
        .host(&args.host)
        .port(args.port)
        .username(&config.superuser.name)
        .password(&config.superuser.password)
        .database(&args.init_db);

    let main_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_with(connection_options)
        .await?;

    if args.apply {
        apply::handle_apply(
            postgresql.as_ref(),
            &main_pool,
            &config,
            &args,
            &args.host,
            args.port,
        )
        .await?;
    } else if args.test{
        test::handle_test(
            &main_pool,
            &args,
        )
        .await?;
    }

    if serve & !args.test {
        tokio::signal::ctrl_c().await?;
        println!("Gracefully shutting down postgres server");
    }

    main_pool.close().await;

    if let Some(postgresql) = postgresql.as_mut() {
        postgresql.stop().await?;
    }

    Ok(())
}
