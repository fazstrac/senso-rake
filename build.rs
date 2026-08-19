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
}
