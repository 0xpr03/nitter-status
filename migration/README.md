# Running Migrator CLI

`cargo run -p migration -- up`

New entities:

`sea-orm-cli migrate generate --migration-dir "migration" --database-url "./sqlite.db?mode=rwc" <name>`

Run the following command to re-generate the entities.
`sea-orm-cli generate entity -o entities/src --with-serde serialize`

Note that you have to check for unwanted changes such as `DateTime` & `Date` types. These are likely replaced with String by the sea-orm CLI. Same with additional modules like `web`.

May require setting the ENV `$Env:DATABASE_URL='sqlite:./sqlite.db?mode=rwc'`

- Generate a new migration file
    ```sh
    cargo run -- migrate generate MIGRATION_NAME
    ```
- Apply all pending migrations
    ```sh
    cargo run
    ```
    ```sh
    cargo run -- up
    ```
- Apply first 10 pending migrations
    ```sh
    cargo run -- up -n 10
    ```
- Rollback last applied migrations
    ```sh
    cargo run -- down
    ```
- Rollback last 10 applied migrations
    ```sh
    cargo run -- down -n 10
    ```
- Drop all tables from the database, then reapply all migrations
    ```sh
    cargo run -- fresh
    ```
- Rollback all applied migrations, then reapply all migrations
    ```sh
    cargo run -- refresh
    ```
- Rollback all applied migrations
    ```sh
    cargo run -- reset
    ```
- Check the status of all migrations
    ```sh
    cargo run -- status
    ```
