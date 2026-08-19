# Migration rules:

1. Migration files are immutable.
2. Never renumber migrations.
3. Never delete migrations.
4. Fix mistakes with a new migration.
5. Every migration must be idempotent only by virtue of being applied once, not by using IF NOT EXISTS.
6. Every migration must be wrapped in BEGIN...COMMIT transaction for entire migration