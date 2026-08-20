# Migration rules:

1. Migration files are immutable.
2. Never renumber migrations.
3. Never delete migrations.
4. Fix mistakes with a new migration.
5. Every migration must be idempotent only by virtue of being applied once, not by using IF NOT EXISTS.
6. The runner wraps the complete pending migration sequence in one transaction. Migration SQL files must not contain transaction-control statements.
7. A matching migration ledger is authoritative. Manual schema changes are unsupported and are not detected or repaired.