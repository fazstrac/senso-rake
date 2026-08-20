#[allow(dead_code)]
#[path = "src/database/migrations/definition.rs"]
mod definition;

fn main() {
    println!("cargo::rerun-if-changed=src/database/migrations");

    for (index, migration) in definition::MIGRATIONS.iter().enumerate() {
        if migration.verify_hash().is_err() {
            panic!(
                "hash verification failed for migration at index {index}; migration scripts are immutable, so restore the script or deliberately update its recorded hash"
            );
        }
    }

    let mut migration_definitions = definition::MIGRATIONS.to_vec();

    migration_definitions.sort_by_key(|left| left.version);

    for pair in migration_definitions.windows(2) {
        if pair[1].version != pair[0].version + 1 {
            panic!("migration definitions must have consecutive numbers");
        }
    }

    match migration_definitions.first() {
        Some(first) if first.version == 1 => {}
        _ => panic!("migration definitions must start with 1"),
    };
}
