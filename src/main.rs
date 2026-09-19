use postgresql_embedded::{PostgreSQL, Result, SettingsBuilder};

use crate::setup_queries::{assign_db_ownership, create_database, create_database_role, set_database_role_login};

pub mod setup_queries;

use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    roles: Vec<Role>,
    databases: Vec<Database>,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let superuser = "postgres";
    let superuser_password = "postgres";
    let init_db = "postgres";
    let config_file = "./config.yaml";
    let data_dir = "./data";
    let schema_file = "./schema_dump.sql";
    let apply_schema = false;
    let host = "127.0.0.1";
    let port = 5432;
    let is_temp_db = false;

    let yaml_str = std::fs::read_to_string(config_file)?;
    let config: Config = serde_saphyr::from_str(&yaml_str).unwrap(); // TODO: Remove unwrap

    let settings = SettingsBuilder::new()
        .host(host)
        .port(port)
        .username(superuser)
        .password(superuser_password)
        .data_dir(data_dir)
        .temporary(is_temp_db)
        .config("max_connections", "100")
        .build();

    //TODO: This needs to be per-db in the db vec
    let schema = if apply_schema {
        Some(
            std::fs::read_to_string(schema_file)?
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

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&postgresql.settings().url(&database.name))
            .await?;

        if let Some(schema) = &schema {
            sqlx::raw_sql(schema).execute(&pool).await?;
        }

        pool.close().await;
    }

    tokio::signal::ctrl_c().await?;

    main_pool.close().await;
    postgresql.stop().await
}
