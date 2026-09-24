use colored::Colorize;
use postgresql_embedded::{PostgreSQL, Result};
use sqlx::PgPool;

use crate::setup_queries::{
    assign_db_ownership, create_database, create_database_role, create_extension, create_schema, create_tracking_tables, grant_schema_privilege, is_schema_change_applied, mark_schema_change_applied, set_database_role_login
};
use crate::{Args, Config, Database, MigrationAction};

pub(crate) async fn handle_apply(
    postgresql: Option<&PostgreSQL>,
    main_pool: &PgPool,
    config: &Config,
    args: &Args,
    host: &str,
    port: u16,
) -> Result<()> {
    if args.config {
        apply_config(postgresql, main_pool, config).await?;
    }

    for database in &config.databases {
        let owner = config
            .roles
            .iter()
            .find(|role| role.name == database.owner)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "Missing credentials for database owner: {}",
                        database.owner
                    ),
                )
            })?;

        let owner_password  = match subst::substitute(&owner.password, &subst::Env) {
            Ok(password) => password,
            Err(err) => {
                eprintln!("Failed to parse database owner password:\n{}", err);
                std::process::exit(1);
            },
        };

        let options = sqlx::postgres::PgConnectOptions::new()
            .host(host)
            .port(port)
            .username(&owner.name)
            .password(&owner_password)
            .database(&database.name);

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        if let Some(extensions) = &database.extensions {
            let superuser_password  = match subst::substitute(&config.superuser.password, &subst::Env) {
                Ok(password) => password,
                Err(err) => {
                    eprintln!("Failed to parse superuser password:\n{}", err);
                    std::process::exit(1);
                },
            };

            let privileged_options = sqlx::postgres::PgConnectOptions::new()
                .host(host)
                .port(port)
                .username(&config.superuser.name)
                .password(&superuser_password)
                .database(&database.name);

            let privileged_pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(5)
                .connect_with(privileged_options)
                .await?;

            for extension in extensions {
                create_extension(&privileged_pool, &database.name, &extension).await?;
            }

            privileged_pool.close().await;
        }


        if let Some(schemas) = &database.schemas {
            let superuser_password  = match subst::substitute(&config.superuser.password, &subst::Env) {
                Ok(password) => password,
                Err(err) => {
                    eprintln!("Failed to parse superuser password:\n{}", err);
                    std::process::exit(1);
                },
            };

            let privileged_options = sqlx::postgres::PgConnectOptions::new()
                .host(host)
                .port(port)
                .username(&config.superuser.name)
                .password(&superuser_password)
                .database(&database.name);

            let privileged_pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(5)
                .connect_with(privileged_options)
                .await?;

            for schema in schemas {
                create_schema(&privileged_pool, &schema.name, &schema.owner, &database.name).await?;
                if let Some(privileges) = &schema.privileges {
                    if let Some(usage) = &privileges.usage {
                        for role in usage {
                            grant_schema_privilege(&privileged_pool, &schema.name, role, &database.name, "USAGE").await?;
                        }
                    }
                    if let Some(create) = &privileges.create {
                        for role in create {
                            grant_schema_privilege(&privileged_pool, &schema.name, role, &database.name, "CREATE").await?;
                        }
                    }
                }
            }

            privileged_pool.close().await;
        }

        if args.schema {
            apply_schema_dump(&pool, database).await?;
        } else {
            create_tracking_tables(&pool).await?;
        }

        match args.migration {
            Some(MigrationAction::Up) => {
                apply_migrations(
                    &pool,
                    &database.name,
                    &database.migrations_dir,
                )
                .await?;
            }
            Some(MigrationAction::Rollback) => {
                rollback_migration(
                    &pool,
                    &database.name,
                    &database.migrations_dir,
                )
                .await?;
            }
            None => {}
        }

        pool.close().await;
    }

    Ok(())
}

pub(crate) async fn apply_config(
    postgresql: Option<&PostgreSQL>,
    main_pool: &PgPool,
    config: &Config,
) -> Result<()> {
    for role in &config.roles {
        let role_password  = match subst::substitute(&role.password, &subst::Env) {
            Ok(password) => password,
            Err(err) => {
                eprintln!("Failed to parse database owner password:\n{}", err);
                std::process::exit(1);
            },
        };

        create_database_role(main_pool, &role.name, &role_password).await?;
        set_database_role_login(main_pool, &role.name, role.login).await?;
    }

    for database in &config.databases {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)",
        )
        .bind(&database.name)
        .fetch_one(main_pool)
        .await?;

        if exists {
            assign_db_ownership(main_pool, &database.name, &database.owner).await?;
        } else if postgresql.is_some() {
            create_database(main_pool, &database.name, &database.owner).await?;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Database {} does not exist; database creation requires --serve",
                    database.name
                ),
            )
            .into());
        }
    }

    Ok(())
}

async fn apply_schema_dump(
    pool: &PgPool,
    database: &Database,
) -> Result<()> {
    let schema_dump_timestamp = 0;
    let schema_dump_description = "schema_dump";

    let schema = std::fs::read_to_string(&database.schema_dump_file)?
        .lines()
        .filter(|line| {
            !matches!(
                line.split_whitespace().next(),
                Some("\\restrict" | "\\unrestrict")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !is_schema_change_applied(pool, schema_dump_timestamp).await? {
        println!("Database {}: Applying schema", database.name.cyan());
        sqlx::raw_sql(&schema).execute(pool).await?;
        create_tracking_tables(pool).await?;
        mark_schema_change_applied(
            pool,
            schema_dump_timestamp,
            schema_dump_description,
        )
        .await?;
    } else {
        println!(
            "Schema has already been applied for database {}; Skipping to avoid conflicts",
            database.name.cyan()
        );
    }

    Ok(())
}

pub async fn apply_migrations(
    pool: &PgPool,
    database_name: &str,
    migrations_dir: &str,
) -> Result<()> {
    let invalid_input = |message: String| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
    };

    let mut migrations = std::collections::BTreeMap::new();

    for entry in std::fs::read_dir(migrations_dir)? {
        let entry = entry?;

        if !entry.file_type()?.is_file() {
            continue;
        }

        let path = entry.path();
        let filename = entry.file_name();
        let filename = filename.to_str().ok_or_else(|| {
            invalid_input(format!("Invalid UTF-8 filename: {}", path.display()))
        })?;

        let Some(stem) = filename.strip_suffix(".up.sql") else {
            continue;
        };

        let (timestamp, description) = stem.split_once('_').ok_or_else(|| {
            invalid_input(format!(
                "Expected YYYYMMDDHHMMSS_description.up.sql: {filename}"
            ))
        })?;

        if timestamp.len() != 14
            || !timestamp.bytes().all(|byte| byte.is_ascii_digit())
            || description.is_empty()
        {
            return Err(invalid_input(format!(
                "Invalid migration filename: {filename}"
            ))
            .into());
        }

        let version = timestamp.parse::<i64>().map_err(|error| {
            invalid_input(format!("Invalid migration version in {filename}: {error}"))
        })?;

        if migrations.contains_key(&version) {
            return Err(invalid_input(format!(
                "Duplicate migration version: {version}"
            ))
            .into());
        }

        migrations.insert(version, (description.to_owned(), path));
    }

    for (version, (description, path)) in migrations {
        let mut transaction = pool.begin().await?;

        sqlx::query(
            "LOCK TABLE starflower_trellis.schema_migrations IN SHARE ROW EXCLUSIVE MODE",
        )
        .execute(&mut *transaction)
        .await?;

        let applied: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM starflower_trellis.schema_migrations
                WHERE version = $1
            )",
        )
        .bind(version)
        .fetch_one(&mut *transaction)
        .await?;

        if applied {
            transaction.commit().await?;
            continue;
        }

        let sql = std::fs::read_to_string(&path)?;

        println!(
            "Database {}: Applying migration {}_{}",
            database_name.cyan(),
            version,
            description.cyan(),
        );

        sqlx::raw_sql(&sql)
            .execute(&mut *transaction)
            .await?;

        //TODO: This is here instead of using the query function cause it needs the
        // transaction - adapt the query function.
        sqlx::query(
            "INSERT INTO starflower_trellis.schema_migrations (version, description)
             VALUES ($1, $2)",
        )
        .bind(version)
        .bind(&description)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
    }

    Ok(())
}

pub async fn rollback_migration(
    pool: &PgPool,
    database_name: &str,
    migrations_dir: &str,
) -> Result<u16> {
    let mut transaction = pool.begin().await?;

    sqlx::query(
        "LOCK TABLE starflower_trellis.schema_migrations IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *transaction)
    .await?;

    let migration: Option<(i64, String)> = sqlx::query_as(
        "SELECT version, description
         FROM starflower_trellis.schema_migrations
         WHERE version > 0
         ORDER BY version DESC
         LIMIT 1",
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let Some((version, description)) = migration else {
        transaction.commit().await?;
        println!(
            "Database {}: No migrations to roll back",
            database_name.cyan(),
        );
        return Ok(404);
    };

    let path = std::path::Path::new(migrations_dir)
        .join(format!("{version:014}_{description}.down.sql"));

    let sql = std::fs::read_to_string(&path).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("Failed to read rollback migration {}: {error}", path.display()),
        )
    })?;

    println!(
        "Database {}: Rolling back migration {:014}_{}",
        database_name.cyan(),
        version,
        description.cyan(),
    );

    sqlx::raw_sql(&sql)
        .execute(&mut *transaction)
        .await?;

    sqlx::query(
        "DELETE FROM starflower_trellis.schema_migrations WHERE version = $1",
    )
    .bind(version)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;

    Ok(200)
}

