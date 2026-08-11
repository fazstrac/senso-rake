use crate::application::{
    CreateLegacyAssignment, CreatedLegacyAssignment, DiscoveredDeviceAssignment,
    LegacyAssignmentRepository, LegacyAssignmentRepositoryError,
};
use crate::database::DbHandle;
use async_trait::async_trait;
use chrono::DateTime;
use serde::Deserialize;
use serde_json;

use log::error;

enum LegacyAssignmentQuery<'a> {
    ListDiscoveredAssignments,
    CreateAssignment(&'a CreateLegacyAssignment),
    SoftDeleteAssignment(i64),
    RestoreAssignment(i64),
}

#[derive(Deserialize)]
struct CreatedDuckDBLegacyAssignment {
    mapping_id: i64,
    model: String,
    reported_id: String,
    description: String,
    validity_start_us: i64,
}

impl TryFrom<CreatedDuckDBLegacyAssignment> for CreatedLegacyAssignment {
    type Error = LegacyAssignmentRepositoryError;

    fn try_from(value: CreatedDuckDBLegacyAssignment) -> Result<Self, Self::Error> {
        let validity_start =
            DateTime::from_timestamp_micros(value.validity_start_us).ok_or_else(|| {
                error!(
                    "Invalid validity_start timestamp {:?}",
                    value.validity_start_us
                );
                LegacyAssignmentRepositoryError::General
            })?;

        Ok(CreatedLegacyAssignment {
            mapping_id: value.mapping_id,
            model: value.model,
            reported_id: value.reported_id,
            description: value.description,
            validity_start,
        })
    }
}

#[derive(Deserialize)]
struct DiscoveredDuckDBDeviceAssignment {
    mapping_id: Option<i64>,
    model: String,
    reported_id: String,
    last_seen_us: i64,
    latest_ulid: String,
    description: Option<String>,
    validity_start_us: Option<i64>,
    deleted: Option<bool>,
}

impl TryFrom<DiscoveredDuckDBDeviceAssignment> for DiscoveredDeviceAssignment {
    type Error = LegacyAssignmentRepositoryError;

    fn try_from(value: DiscoveredDuckDBDeviceAssignment) -> Result<Self, Self::Error> {
        let last_seen = DateTime::from_timestamp_micros(value.last_seen_us).ok_or_else(|| {
            error!("Invalid last_seen timestamp {}", value.last_seen_us);
            LegacyAssignmentRepositoryError::Serialization
        })?;

        let validity_start = value
            .validity_start_us
            .map(|ts| {
                DateTime::from_timestamp_micros(ts).ok_or_else(|| {
                    error!(
                        "Invalid validity_start timestamp {:?}",
                        value.validity_start_us
                    );
                    LegacyAssignmentRepositoryError::Serialization
                })
            })
            .transpose()?;

        Ok(DiscoveredDeviceAssignment {
            mapping_id: value.mapping_id,
            model: value.model,
            reported_id: value.reported_id,
            last_seen,
            latest_ulid: value.latest_ulid,
            description: value.description,
            validity_start,
            deleted: value.deleted,
        })
    }
}

struct LegacyDBQuery {
    sql: String,
    params: Vec<String>,
}

impl LegacyAssignmentQuery<'_> {
    fn into_query(self) -> LegacyDBQuery {
        match self {
            Self::ListDiscoveredAssignments => LegacyDBQuery{sql: "SELECT mapping_id, model, id AS reported_id, epoch_us(last_seen) AS last_seen_us, latest_ulid, description, epoch_us(validity_start) AS validity_start_us, deleted FROM all_sensors".into(), params: vec![]},
            Self::CreateAssignment(payload) => LegacyDBQuery{sql: "INSERT INTO mappings (model, id, validity_start, description) VALUES (?, ?, ?, ?) RETURNING mapping_id, model, id as reported_id, description, epoch_us(validity_start) AS validity_start_us".into(), params: vec![
                payload.model.clone(),
                payload.reported_id.clone(),
                payload.validity_start.to_rfc3339(),
                payload.description.clone(),
            ]},
            Self::SoftDeleteAssignment(id) => LegacyDBQuery{sql: "UPDATE mappings SET deleted = true WHERE mapping_id = ?".into(), params: vec![id.to_string()]},
            Self::RestoreAssignment(id) => LegacyDBQuery{sql: "UPDATE mappings SET deleted = false WHERE mapping_id = ?".into(), params: vec![id.to_string()]},
        }
    }
}

pub struct DuckDBLegacyAssignmentRepository {
    db_handle: DbHandle,
}

impl DuckDBLegacyAssignmentRepository {
    pub fn new(db_handle: DbHandle) -> Self {
        Self { db_handle }
    }
}

#[async_trait]
impl LegacyAssignmentRepository for DuckDBLegacyAssignmentRepository {
    async fn list_discovered_assignments(
        &self,
    ) -> Result<Vec<DiscoveredDeviceAssignment>, LegacyAssignmentRepositoryError> {
        let query = LegacyAssignmentQuery::ListDiscoveredAssignments.into_query();
        let json = self
            .db_handle
            .query(query.sql)
            .await
            .map_err(map_duckdb_error)?;

        let rows: Vec<DiscoveredDuckDBDeviceAssignment> =
            serde_json::from_str(&json).map_err(map_serde_error)?;

        rows.into_iter()
            .map(DiscoveredDeviceAssignment::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn create_assignment(
        &self,
        command: CreateLegacyAssignment,
    ) -> Result<CreatedLegacyAssignment, LegacyAssignmentRepositoryError> {
        let query = LegacyAssignmentQuery::CreateAssignment(&command).into_query();
        let json = self
            .db_handle
            .query_with_params(query.sql, query.params)
            .await
            .map_err(map_duckdb_error)?;

        let rows: Vec<CreatedDuckDBLegacyAssignment> =
            serde_json::from_str(&json).map_err(map_serde_error)?;

        // Force the result into an array of size 1
        let [row]: [CreatedDuckDBLegacyAssignment; 1] = rows
            .try_into()
            .map_err(|_e| LegacyAssignmentRepositoryError::Serialization)?;

        CreatedLegacyAssignment::try_from(row)
    }

    async fn soft_delete_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentRepositoryError> {
        let query = LegacyAssignmentQuery::SoftDeleteAssignment(mapping_id).into_query();
        let json = self
            .db_handle
            .query_with_params(query.sql, query.params)
            .await
            .map_err(map_duckdb_error)?;

        let data: () = serde_json::from_str(&json).map_err(map_serde_error)?;

        Ok(data)
    }

    async fn restore_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentRepositoryError> {
        let query = LegacyAssignmentQuery::RestoreAssignment(mapping_id).into_query();
        let json = self
            .db_handle
            .query_with_params(query.sql, query.params)
            .await
            .map_err(map_duckdb_error)?;

        let data: () = serde_json::from_str(&json).map_err(map_serde_error)?;

        Ok(data)
    }
}

fn map_duckdb_error(e: anyhow::Error) -> LegacyAssignmentRepositoryError {
    let msg = e.to_string();
    error!("Repository repository response {msg}");

    if msg.starts_with("Constraint Error: Duplicate key") {
        LegacyAssignmentRepositoryError::AssignmentAlreadyExists
    } else {
        LegacyAssignmentRepositoryError::Persistence
    }
}

fn map_serde_error(e: serde_json::Error) -> LegacyAssignmentRepositoryError {
    error!("Failed to deserialize repository response {e}");

    LegacyAssignmentRepositoryError::Serialization
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};

    // Pull real schema SQL and Arrow batch creation from the crate
    use crate::database::DbService;
    use crate::database::schema::SCHEMA_SQL;
    use crate::service::Service;

    use crossbeam_channel::{Sender, unbounded};
    use tokio::task::JoinHandle;

    fn test_helper_time() -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(2026, 8, 2)
            .unwrap()
            .and_hms_opt(19, 30, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap()
    }

    fn test_helper_assignment() -> CreateLegacyAssignment {
        let ts1 = test_helper_time();

        CreateLegacyAssignment {
            model: "Mock".into(),
            reported_id: "254".into(),
            description: "MockDescription".into(),
            validity_start: ts1,
        }
    }
    fn test_helper_create_expected_assignment() -> CreatedLegacyAssignment {
        let assignment = test_helper_assignment();

        CreatedLegacyAssignment {
            mapping_id: 1,
            model: assignment.model,
            reported_id: assignment.reported_id,
            description: assignment.description,
            validity_start: assignment.validity_start,
        }
    }

    struct RepositoryFixture {
        repository: DuckDBLegacyAssignmentRepository,
        shutdown_tx: Sender<()>,
        join_handle: JoinHandle<()>,
    }

    impl RepositoryFixture {
        async fn new() -> Self {
            let (db_shutdown_tx, db_shutdown_rx) = unbounded();
            let db_svc = DbService::new(None, db_shutdown_rx).unwrap();
            let db_handle = db_svc.get_handle();
            let join_handle = db_svc.start().await.unwrap();
            let _res = db_handle.query(SCHEMA_SQL.to_string()).await;

            let repo = DuckDBLegacyAssignmentRepository::new(db_handle);

            Self {
                repository: repo,
                shutdown_tx: db_shutdown_tx,
                join_handle,
            }
        }

        async fn shutdown(self) {
            self.shutdown_tx.send(()).unwrap();
            self.join_handle.await.unwrap();
        }
    }

    //
    // TESTS
    //

    #[test]
    fn legacy_assignment_list_discovered_assignments_correct_sql() {
        let query = LegacyAssignmentQuery::ListDiscoveredAssignments.into_query();

        assert_eq!(query.sql, "SELECT mapping_id, model, id AS reported_id, epoch_us(last_seen) AS last_seen_us, latest_ulid, description, epoch_us(validity_start) AS validity_start_us, deleted FROM all_sensors".to_string());
        assert_eq!(query.params.len(), 0);
    }

    #[test]
    fn legacy_assignment_create_assignment_correct_sql_and_parameter_order() {
        let command = test_helper_assignment();
        let query = LegacyAssignmentQuery::CreateAssignment(&command).into_query();

        assert_eq!(query.sql, "INSERT INTO mappings (model, id, validity_start, description) VALUES (?, ?, ?, ?) RETURNING mapping_id, model, id as reported_id, description, epoch_us(validity_start) AS validity_start_us".to_string());

        let params = query.params;
        assert_eq!(params.len(), 4);
        assert_eq!(params[0], command.model);
        assert_eq!(params[1], command.reported_id);
        assert_eq!(params[2], command.validity_start.to_rfc3339());
        assert_eq!(params[3], command.description);
    }

    #[test]
    fn legacy_assignment_soft_delete_assignment_correct_sql() {
        let id = 124;

        let query = LegacyAssignmentQuery::SoftDeleteAssignment(id).into_query();

        assert_eq!(
            query.sql,
            "UPDATE mappings SET deleted = true WHERE mapping_id = ?".to_string()
        );

        let params = query.params;
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], id.to_string());
    }

    #[test]
    fn legacy_assignment_restore_assignment_correct_sql() {
        let id = 1286;

        let query = LegacyAssignmentQuery::RestoreAssignment(id).into_query();

        assert_eq!(
            query.sql,
            "UPDATE mappings SET deleted = false WHERE mapping_id = ?".to_string()
        );

        let params = query.params;
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], id.to_string());
    }

    #[tokio::test]
    async fn test_create_assignment_success() {
        let fixture = RepositoryFixture::new().await;

        let res = fixture
            .repository
            .create_assignment(test_helper_assignment())
            .await;

        assert_eq!(res.unwrap(), test_helper_create_expected_assignment());
        fixture.shutdown().await;
    }

    #[tokio::test]
    async fn test_create_assignment_duplicate_should_fail() {
        let fixture = RepositoryFixture::new().await;

        let res1 = fixture
            .repository
            .create_assignment(test_helper_assignment())
            .await;
        assert_eq!(res1.unwrap(), test_helper_create_expected_assignment());

        let res2 = fixture
            .repository
            .create_assignment(test_helper_assignment())
            .await;
        assert_eq!(
            res2.err().unwrap(),
            LegacyAssignmentRepositoryError::AssignmentAlreadyExists
        );

        fixture.shutdown().await;
    }
}
