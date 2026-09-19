use postgresql_embedded::{PostgreSQL, Result, SettingsBuilder};

use crate::setup_queries::{create_database, create_database_user};

pub mod setup_queries;

#[tokio::main]
async fn main() -> Result<()> {
    let superuser = "postgres";
    let superuser_password = "postgres";
    let init_db = "postgres";

    let users = vec!["admin"];
    let user_password = "password"; // TODO: Make users a map vec
    let databases = vec!["test"];

    let data_dir = "./data";
    let schema_file = "./schema_dump.sql";
    let apply_schema = false;
    let host = "127.0.0.1";
    let port = 5432;
    let is_temp_db = true;

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

    for user in &users {
        create_database_user(&main_pool, user, user_password).await?;
    }

    for database in &databases {
        if !postgresql.database_exists(database).await? {
            create_database(&main_pool, database, users[0]).await?;
        }

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&postgresql.settings().url(database))
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
