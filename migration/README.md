# Running Migrator CLI

Run these commands from `migration/`. The `cli` feature enables the standalone
migration tool; application startup runs migrations without this feature.

- Generate a new migration file
    ```sh
    cargo run --features cli -- generate MIGRATION_NAME
    ```
- Apply all pending migrations
    ```sh
    cargo run --features cli
    ```
    ```sh
    cargo run --features cli -- up
    ```
- Apply first 10 pending migrations
    ```sh
    cargo run --features cli -- up -n 10
    ```
- Rollback last applied migrations
    ```sh
    cargo run --features cli -- down
    ```
- Rollback last 10 applied migrations
    ```sh
    cargo run --features cli -- down -n 10
    ```
- Drop all tables from the database, then reapply all migrations
    ```sh
    cargo run --features cli -- fresh
    ```
- Rollback all applied migrations, then reapply all migrations
    ```sh
    cargo run --features cli -- refresh
    ```
- Rollback all applied migrations
    ```sh
    cargo run --features cli -- reset
    ```
- Check the status of all migrations
    ```sh
    cargo run --features cli -- status
    ```
