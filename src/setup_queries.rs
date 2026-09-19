use sqlx::{Pool, Postgres};

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

pub async fn create_database_user(
    pool: &Pool<Postgres>,
    user: &str,
    password: &str,
) -> Result<(), sqlx::Error> {
    let exists = check_role_exists(pool, user).await?;

    if !exists {
        println!("Creating database user: {user}");

        let mut conn = pool.acquire().await?;

        let statement: String = sqlx::query_scalar(
            r#"
            SELECT
                format(
                    'CREATE USER %I WITH PASSWORD %L LOGIN',
                    $1::text,
                    $2::text
                );
            "#,
        )
        .bind(user)
        .bind(password)
        .fetch_one(&mut *conn)
        .await?;

        sqlx::raw_sql(&statement)
            .execute(&mut *conn)
            .await?;
    }

    Ok(())
}

pub async fn create_database(
    pool: &Pool<Postgres>,
    db_name: &str,
    db_owner: &str,
) -> Result<(), sqlx::Error> {
    println!("Creating database: {db_name}");

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
