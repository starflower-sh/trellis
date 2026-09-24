use sqlx::{Pool, Postgres};
use colored::Colorize;

pub async fn create_tracking_tables(
    pool: &Pool<Postgres>,
) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(
        r#"
        CREATE SCHEMA IF NOT EXISTS starflower_trellis;

        CREATE TABLE IF NOT EXISTS starflower_trellis.schema_migrations (
            version BIGINT NOT NULL PRIMARY KEY,
            description TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_schema_change_applied(
    pool: &Pool<Postgres>,
    version: i64,
    description: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO
            starflower_trellis.schema_migrations (version, description)
        VALUES
            ($1, $2);
        "#,
    )
    .bind(version)
    .bind(description)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn is_schema_change_applied(
    pool: &Pool<Postgres>,
    version: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM starflower_trellis.schema_migrations
            WHERE version = $1
        );
        "#,
    )
    .bind(version)
    .fetch_one(pool)
    .await;

    match result {
        Err(sqlx::Error::Database(error))
            if error.code().as_deref() == Some("42P01") =>
        {
            Ok(false)
        }
        other => other,
    }
}

pub async fn check_role_exists(
    pool: &Pool<Postgres>,
    user: &str,
) -> Result<bool, sqlx::Error> {
    let mut conn = pool.acquire().await?;

    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT
            EXISTS (
                SELECT
                    1
                FROM
                    pg_roles
                WHERE
                    rolname = $1
            );
        "#,
    )
    .bind(user)
    .fetch_one(&mut *conn)
    .await?;

    return Ok(exists);
}

pub async fn create_database_role(
    pool: &Pool<Postgres>,
    role: &str,
    password: &str,
) -> Result<(), sqlx::Error> {
    let exists = check_role_exists(pool, role).await?;

    if !exists {
        println!("Creating database role {}", role.cyan());

        let mut conn = pool.acquire().await?;

        let statement: String = sqlx::query_scalar(
            r#"
            SELECT format(
                'CREATE ROLE %I WITH PASSWORD %L',
                $1::text,
                $2::text
            );
            "#,
        )
        .bind(role)
        .bind(password)
        .fetch_one(&mut *conn)
        .await?;

        sqlx::raw_sql(&statement)
            .execute(&mut *conn)
            .await?;
    } else {
        println!("Database role {} already exists; skipping creation", role.cyan());
    }

    Ok(())
}

pub async fn set_database_role_login(
    pool: &Pool<Postgres>,
    user: &str,
    grant_login: bool,
) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;

    println!(
        "{} login for database role {}",
        if grant_login { "Enabling" } else { "Disabling" }.cyan(),
        user.cyan()
    );

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT format(
            'ALTER ROLE %I %s',
            $1::text,
            CASE WHEN $2::boolean THEN 'LOGIN' ELSE 'NOLOGIN' END
        );
        "#,
    )
    .bind(user)
    .bind(grant_login)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn create_database(
    pool: &Pool<Postgres>,
    db_name: &str,
    db_owner: &str,
) -> Result<(), sqlx::Error> {
    println!("Creating database {} with owner {}", db_name.cyan(), db_owner.cyan());

    let mut conn = pool.acquire().await?;

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT
            format(
                'CREATE DATABASE %I OWNER %I',
                $1::text,
                $2::text
            );
        "#,
    )
    .bind(db_name)
    .bind(db_owner)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn assign_db_ownership(
    pool: &Pool<Postgres>,
    db_name: &str,
    db_owner: &str,
) -> Result<(), sqlx::Error> {
    println!("Setting owner of database {} as {}", db_name.cyan(), db_owner.cyan());

    let mut conn = pool.acquire().await?;

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT format(
            'ALTER DATABASE %I OWNER TO %I',
            $1::text,
            $2::text
        );
        "#,
    )
    .bind(db_name)
    .bind(db_owner)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn create_extension(
    pool: &Pool<Postgres>,
    db_name: &str,
    extension_name: &str,
) -> Result<(), sqlx::Error> {
    println!("Setting up exension {} on database {}", extension_name.cyan(), db_name.cyan());

    let mut conn = pool.acquire().await?;

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT format(
            'CREATE EXTENSION IF NOT EXISTS %I',
            $1::text
        );
        "#,
    )
    .bind(extension_name)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn create_schema(
    pool: &Pool<Postgres>,
    schema_name: &str,
    schema_owner: &str,
    db_name: &str,
) -> Result<(), sqlx::Error> {
    println!("Creating schema {} if it doesn't exist in database {} with owner {}", schema_name.cyan(), db_name.cyan(), schema_owner.cyan());

    let mut conn = pool.acquire().await?;

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT
            format(
                'CREATE SCHEMA IF NOT EXISTS %I AUTHORIZATION %I',
                $1::text,
                $2::text
            );
        "#,
    )
    .bind(schema_name)
    .bind(schema_owner)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn grant_schema_privilege(
    pool: &Pool<Postgres>,
    schema_name: &str,
    role_name: &str,
    db_name: &str,
    privilege: &str,
) -> Result<(), sqlx::Error> {
    println!("Granting {} for schema {} in database {} to {}", privilege.cyan(), schema_name.cyan(), db_name.cyan(), role_name.cyan());

    let mut conn = pool.acquire().await?;

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT
            format(
                'GRANT %s ON SCHEMA %I TO %I',
                $1::text,
                $2::text,
                $3::text
            );
        "#,
    )
    .bind(privilege)
    .bind(schema_name)
    .bind(role_name)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}
