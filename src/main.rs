use postgresql_embedded::{PostgreSQL, Result, SettingsBuilder};
use colored::Colorize;

use crate::setup_queries::{assign_db_ownership, create_database, create_database_role, create_tracking_tables, is_schema_change_applied, mark_schema_change_applied, set_database_role_login};

pub mod setup_queries;

use serde::Deserialize;

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
    login: bool
}

#[derive(Deserialize)]
struct Database {
    name: String,
    owner: String,
    schema_dump_file: String,
    schemas: Vec<Schema>
}

#[derive(Deserialize)]
struct Schema {
    name: String,
    owner: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let init_db = "postgres";
    let config_file = "./config.yaml";
    let data_dir = "./data";
    let host = "127.0.0.1";
    let port = 5432;
    let is_temp_db = true;
    let apply_schema = true;
    let schema_dump_timestamp = 0;
    let schema_dump_description = "schema_dump";

    let yaml_str = std::fs::read_to_string(config_file)?;
    let config: Config = serde_saphyr::from_str(&yaml_str).unwrap(); // TODO: Remove unwrap

    let settings = SettingsBuilder::new()
        .host(host)
        .port(port)
        .username(&config.superuser.name)
        .password(&config.superuser.password)
        .data_dir(data_dir)
        .temporary(is_temp_db)
        .config("max_connections", "100")
        .build();

    let mut postgresql = PostgreSQL::new(settings);
    postgresql.setup().await?;
    postgresql.start().await?;

    let main_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgresql.settings().url(init_db))
        .await?;

    for role in &config.roles {
        create_database_role(&main_pool, &role.name, &role.password).await?;
        set_database_role_login(&main_pool, &role.name, role.login).await?;
    }

    for database in &config.databases {
        if !postgresql.database_exists(&database.name).await? {
            create_database(&main_pool, &database.name, &database.owner).await?;
        } else {
            assign_db_ownership(&main_pool, &database.name, &database.owner).await?;
        }

        let owner = config
            .roles
            .iter()
            .find(|role| role.name == database.owner)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Missing credentials for database owner: {}", database.owner),
                )
            })?;

        let options = sqlx::postgres::PgConnectOptions::new()
            .host(host)
            .port(port)
            .username(&owner.name)
            .password(&owner.password)
            .database(&database.name);


        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        

        let schema = if apply_schema {
            Some(
                std::fs::read_to_string(&database.schema_dump_file)?
                    .lines()
                    .filter(|line| {
                        !matches!(
                            line.split_whitespace().next(),
                            Some("\\restrict" | "\\unrestrict")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        } else {
            None
        };

        if let Some(schema) = &schema {
            if !is_schema_change_applied(&pool, schema_dump_timestamp).await? {
                println!("Applying schema for database {}", database.name.cyan());
                sqlx::raw_sql(schema).execute(&pool).await?;
                create_tracking_tables(&pool).await?;
                mark_schema_change_applied(&pool, schema_dump_timestamp, schema_dump_description).await?;
            } else {
                println!("Schema has already been applied for database {}; Skipping to avoid conflicts", database.name.cyan());
            }
        } else {
            create_tracking_tables(&pool).await?;
        }

        for schema in &database.schemas {
            println!("{} {}", schema.name, schema.owner)
        }

        pool.close().await;
    }

    tokio::signal::ctrl_c().await?;

    println!("Gracefully shutting down");
    main_pool.close().await;
    postgresql.stop().await
}
