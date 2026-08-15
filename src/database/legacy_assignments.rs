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
        db_handle: DbHandle,
    }

    impl RepositoryFixture {
        async fn new() -> Self {
            let (db_shutdown_tx, db_shutdown_rx) = unbounded();
            let db_svc = DbService::new(None, db_shutdown_rx).unwrap();

            let db_handle = db_svc.get_handle();
            let join_handle = db_svc.start().await.unwrap();
            let _res = db_handle
                .query(SCHEMA_SQL.to_string())
                .await
                .expect("initialize test schema");

            let repo = DuckDBLegacyAssignmentRepository::new(db_handle.clone());

            Self {
                repository: repo,
                shutdown_tx: db_shutdown_tx,
                join_handle,
                db_handle,
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

    //
    // Functional tests
    //

    #[tokio::test]
    async fn test_create_assignment_success() {
        let fixture = RepositoryFixture::new().await;

        let res = fixture
            .repository
            .create_assignment(test_helper_assignment())
            .await;

        assert_eq!(res, Ok(test_helper_create_expected_assignment()));
        fixture.shutdown().await;
    }

    #[tokio::test]
    async fn test_create_assignment_duplicate_should_fail() {
        let fixture = RepositoryFixture::new().await;

        let res1 = fixture
            .repository
            .create_assignment(test_helper_assignment())
            .await;
        assert_eq!(res1, Ok(test_helper_create_expected_assignment()));

        let res2 = fixture
            .repository
            .create_assignment(test_helper_assignment())
            .await;
        assert_eq!(
            res2,
            Err(LegacyAssignmentRepositoryError::AssignmentAlreadyExists)
        );

        fixture.shutdown().await;
    }

    #[tokio::test]
    async fn test_list_assignments_empty_database_success() {
        let fixture = RepositoryFixture::new().await;
        let res = fixture.repository.list_discovered_assignments().await;

        assert_eq!(res, Ok(Vec::new()));

        fixture.shutdown().await;
    }

    impl RepositoryFixture {
        async fn populate_database(&self) {
            self
            .db_handle
            .query(
                r#"BEGIN;
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ5MGZTEQHF5PHBPEEW67', TIMESTAMP '2026-01-10 10:37:33.200231', '{"time":"1768041453.200231", "protocol":20, "model":"Ambientweather-F007TH", "id":44, "channel":1, "battery_ok":1, "temperature_C":22.61111, "humidity":2, "mic":"CRC","mod":"ASK","freq":433.90758,"rssi":-0.189728,"snr":16.7441, "noise":-16.9338}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ765D742SDETKASHTPSP', TIMESTAMP '2026-01-10 10:37:34.789691', '{"time":"1768041454.789691", "protocol":4, "model":"Waveman-Switch", "id":"A", "channel":1, "button":1, "state":"OFF","mod":"ASK","freq":433.88842,"rssi":-9.28286,"snr":7.23996, "noise":-16.5228}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQARQ0650GH85SJ9FYAWK', TIMESTAMP '2026-01-10 10:37:38.455727', '{"time":"1768041458.455727", "protocol":20, "model":"Ambientweather-F007TH", "id":141, "channel":1, "battery_ok":0, "temperature_C":16.05556, "humidity":1, "mic":"CRC","mod":"ASK","freq":433.8936,"rssi":-0.239174,"snr":16.14269, "noise":-16.3819}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQB6P4TT3MG70ZMNR47T8', TIMESTAMP '2026-01-10 10:37:38.902171', '{"time":"1768041458.902171", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":254, "channel":1, "battery_ok":1, "temperature_C":20.2, "test":"No","mod":"ASK","freq":433.91968,"rssi":-0.304817,"snr":16.67757, "noise":-16.9824}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQBEPKEVXVS9KTYSCD8YH', TIMESTAMP '2026-01-10 10:37:39.158496', '{"time":"1768041459.158496", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":254, "channel":1, "battery_ok":1, "temperature_C":20.2, "test":"No","mod":"ASK","freq":433.9201,"rssi":-0.369858,"snr":16.69533, "noise":-17.0652}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQFKYBA02XZ2Q113SG5NT', TIMESTAMP '2026-01-10 10:37:43.422414', '{"time":"1768041463.422414", "protocol":20, "model":"Ambientweather-F007TH", "id":143, "channel":1, "battery_ok":1, "temperature_C":-9.27778, "humidity":6, "mic":"CRC","mod":"ASK","freq":433.9127,"rssi":-0.537415,"snr":16.42252, "noise":-16.9599}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQNXGC4FSZ1HG3KM002HA', TIMESTAMP '2026-01-10 10:37:49.872493', '{"time":"1768041469.872493", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":246, "channel":1, "battery_ok":1, "temperature_C":20.4, "test":"No","mod":"ASK","freq":434.01165,"rssi":-0.354927,"snr":17.0025, "noise":-17.3574}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQP5GFQ616FBNBFHNVD0D', TIMESTAMP '2026-01-10 10:37:50.128827', '{"time":"1768041470.128827", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":246, "channel":1, "battery_ok":1, "temperature_C":20.4, "test":"No","mod":"ASK","freq":434.01133,"rssi":-0.332863,"snr":17.02847, "noise":-17.3613}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQR57DH8HXXVAR62QFV9FK', TIMESTAMP '2026-01-10 10:38:05.549194', '{"time":"1768041485.549194", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":217, "channel":1, "battery_ok":1, "temperature_C":-9.5, "test":"No","mod":"ASK","freq":433.8897,"rssi":-8.03949,"snr":10.04779, "noise":-18.0873}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQR8T7NEZG8GT3R7KMYEZP', TIMESTAMP '2026-01-10 10:38:09.223944', '{"time":"1768041489.223944", "protocol":2, "model":"Rubicson-Temperature", "id":112, "channel":1, "battery_ok":1, "temperature_C":-9.9, "mic":"CRC","mod":"ASK","freq":433.92019,"rssi":-7.03146,"snr":11.34119, "noise":-18.3727}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQR9FEXC9A80Q2KM5FFYZB', TIMESTAMP '2026-01-10 10:38:09.902302', '{"time":"1768041489.902302", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":254, "channel":1, "battery_ok":1, "temperature_C":20.2, "test":"No","mod":"ASK","freq":433.92109,"rssi":-0.300972,"snr":17.31042, "noise":-17.6114}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQREAXF2W8SPTKVW7RD7C6', TIMESTAMP '2026-01-10 10:38:14.877032', '{"time":"1768041494.877032", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":254, "channel":1, "battery_ok":1, "temperature_C":20.2, "test":"No","mod":"ASK","freq":433.89136,"rssi":-5.9509,"snr":11.72516, "noise":-17.6761}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQRG8S7BFPBAJVE9KFB8GT', TIMESTAMP '2026-01-10 10:38:16.857048', '{"time":"1768041496.857048", "protocol":165, "model":"TFA-303221", "id":1, "channel":1, "battery_ok":0, "temperature_C":-9.3, "humidity":73, "sendmode":0, "mic":"CRC","mod":"ASK","freq":433.9183,"rssi":-12.1307,"snr":5.9099, "noise":-18.0406}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQRM68KT5XYWP6XZZZKW5M', TIMESTAMP '2026-01-10 10:38:20.872574', '{"time":"1768041500.872574", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":246, "channel":1, "battery_ok":1, "temperature_C":20.4, "test":"No","mod":"ASK","freq":434.01126,"rssi":-0.337822,"snr":18.03484, "noise":-18.3727}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQRME84PQNHFGY83AS11SD', TIMESTAMP '2026-01-10 10:38:21.128824', '{"time":"1768041501.128824", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":246, "channel":1, "battery_ok":1, "temperature_C":20.4, "test":"No","mod":"ASK","freq":434.01213,"rssi":-0.258278,"snr":17.21332, "noise":-17.4716}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQRY8CN7F1S1KWZBWKHENG', TIMESTAMP '2026-01-10 10:38:31.180737', '{"time":"1768041511.180737", "protocol":20, "model":"Ambientweather-F007TH", "id":44, "channel":1, "battery_ok":1, "temperature_C":22.61111, "humidity":2, "mic":"CRC","mod":"ASK","freq":433.89424,"rssi":-0.258278,"snr":17.28893, "noise":-17.5472}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQS34EA4YYEFK0H0PT36BJ', TIMESTAMP '2026-01-10 10:38:36.174418', '{"time":"1768041516.174418", "protocol":20, "model":"Ambientweather-F007TH", "id":202, "channel":1, "battery_ok":0, "temperature_C":16.05556, "humidity":1, "mic":"CRC","mod":"ASK","freq":433.89498,"rssi":-0.259361,"snr":17.19639, "noise":-17.4557}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQS3GG2B89Z9B0RSYAC8H2', TIMESTAMP '2026-01-10 10:38:36.560692', '{"time":"1768041516.560692", "protocol":20, "model":"Ambientweather-F007TH", "id":143, "channel":1, "battery_ok":1, "temperature_C":-9.27778, "humidity":6, "mic":"CRC","mod":"ASK","freq":433.9104,"rssi":-0.276863,"snr":16.93706, "noise":-17.2139}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQS7J5F9QBP8Z9QYM2XV2F', TIMESTAMP '2026-01-10 10:38:40.709175', '{"time":"1768041520.709175", "protocol":73, "model":"LaCrosse-TX141Bv3", "id":217, "channel":1, "battery_ok":1, "temperature_C":-9.5, "test":"No","mod":"ASK","freq":433.9361,"rssi":-9.54773,"snr":7.60868, "noise":-17.1564}');
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (1, 'LaCrosse-TX141Bv3', '246', 'Oton huone', TIMESTAMP '2026-01-10 00:00:00', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (2, 'LaCrosse-TX141Bv3', '254', 'Aapon huone', TIMESTAMP '2026-01-10 00:00:00', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (3, 'LaCrosse-TX29IT', '19', 'Makuuhuone', TIMESTAMP '2026-01-10 00:00:00', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (4, 'LaCrosse-TX29IT', '83', 'Ulkoilma', TIMESTAMP '2026-01-10 00:00:00', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (5, 'Ambientweather-F007TH', '44', 'Reitinkaappi', TIMESTAMP '2026-01-10 00:00:00', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (6, 'Ambientweather-F007TH', '141', 'Varasto' , TIMESTAMP '2026-01-10 00:00:00', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (7, 'sensor-a', '001', 'Living Room', TIMESTAMP '2026-02-12 17:42:41.704593', true);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (8, 'sensor-a', '002', 'Living Room', TIMESTAMP '2026-02-12 17:53:38.257108', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (9, 'Ambientweather-F007TH', '202', 'Varasto', TIMESTAMP '2026-02-14 10:37:16.573727', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (10, 'Ambientweather-F007TH', '248', 'Varasto', TIMESTAMP '2026-02-14 10:44:30.067259', true);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (11, 'Ambientweather-F007TH', '202', 'Varasto', TIMESTAMP '2026-02-14 11:06:20.73908', true);
                   COMMIT;"#.to_string()
            )
            .await
            .expect("populate repository test database");
        }
    }

    fn populated_database_ground_truth() -> Vec<DiscoveredDeviceAssignment> {
        let timestamp = |value: &str| {
            DateTime::parse_from_rfc3339(value)
                .unwrap()
                .with_timezone(&Utc)
        };
        let validity_start = timestamp("2026-01-10T00:00:00Z");

        vec![
            DiscoveredDeviceAssignment {
                mapping_id: Some(5),
                model: "Ambientweather-F007TH".into(),
                reported_id: "44".into(),
                last_seen: timestamp("2026-01-10T10:38:31.180737Z"),
                latest_ulid: "01KEKQRY8CN7F1S1KWZBWKHENG".into(),
                description: Some("Reitinkaappi".into()),
                validity_start: Some(validity_start),
                deleted: Some(false),
            },
            DiscoveredDeviceAssignment {
                mapping_id: Some(6),
                model: "Ambientweather-F007TH".into(),
                reported_id: "141".into(),
                last_seen: timestamp("2026-01-10T10:37:38.455727Z"),
                latest_ulid: "01KEKQQARQ0650GH85SJ9FYAWK".into(),
                description: Some("Varasto".into()),
                validity_start: Some(validity_start),
                deleted: Some(false),
            },
            DiscoveredDeviceAssignment {
                mapping_id: None,
                model: "Ambientweather-F007TH".into(),
                reported_id: "143".into(),
                last_seen: timestamp("2026-01-10T10:38:36.560692Z"),
                latest_ulid: "01KEKQS3GG2B89Z9B0RSYAC8H2".into(),
                description: None,
                validity_start: None,
                deleted: None,
            },
            DiscoveredDeviceAssignment {
                mapping_id: Some(1),
                model: "LaCrosse-TX141Bv3".into(),
                reported_id: "246".into(),
                last_seen: timestamp("2026-01-10T10:38:21.128824Z"),
                latest_ulid: "01KEKQRME84PQNHFGY83AS11SD".into(),
                description: Some("Oton huone".into()),
                validity_start: Some(validity_start),
                deleted: Some(false),
            },
            DiscoveredDeviceAssignment {
                mapping_id: Some(2),
                model: "LaCrosse-TX141Bv3".into(),
                reported_id: "254".into(),
                last_seen: timestamp("2026-01-10T10:38:14.877032Z"),
                latest_ulid: "01KEKQREAXF2W8SPTKVW7RD7C6".into(),
                description: Some("Aapon huone".into()),
                validity_start: Some(validity_start),
                deleted: Some(false),
            },
            DiscoveredDeviceAssignment {
                mapping_id: None,
                model: "LaCrosse-TX141Bv3".into(),
                reported_id: "217".into(),
                last_seen: timestamp("2026-01-10T10:38:40.709175Z"),
                latest_ulid: "01KEKQS7J5F9QBP8Z9QYM2XV2F".into(),
                description: None,
                validity_start: None,
                deleted: None,
            },
            DiscoveredDeviceAssignment {
                mapping_id: None,
                model: "Rubicson-Temperature".into(),
                reported_id: "112".into(),
                last_seen: timestamp("2026-01-10T10:38:09.223944Z"),
                latest_ulid: "01KEKQR8T7NEZG8GT3R7KMYEZP".into(),
                description: None,
                validity_start: None,
                deleted: None,
            },
            DiscoveredDeviceAssignment {
                mapping_id: None,
                model: "TFA-303221".into(),
                reported_id: "1".into(),
                last_seen: timestamp("2026-01-10T10:38:16.857048Z"),
                latest_ulid: "01KEKQRG8S7BFPBAJVE9KFB8GT".into(),
                description: None,
                validity_start: None,
                deleted: None,
            },
            DiscoveredDeviceAssignment {
                mapping_id: None,
                model: "Waveman-Switch".into(),
                reported_id: "A".into(),
                last_seen: timestamp("2026-01-10T10:37:34.789691Z"),
                latest_ulid: "01KEKQQ765D742SDETKASHTPSP".into(),
                description: None,
                validity_start: None,
                deleted: None,
            },
            DiscoveredDeviceAssignment {
                mapping_id: Some(9),
                model: "Ambientweather-F007TH".into(),
                reported_id: "202".into(),
                last_seen: timestamp("2026-01-10T10:38:36.174418Z"),
                latest_ulid: "01KEKQS34EA4YYEFK0H0PT36BJ".into(),
                description: Some("Varasto".into()),
                validity_start: Some(timestamp("2026-02-14T10:37:16.573727Z")),
                deleted: Some(false),
            },
            DiscoveredDeviceAssignment {
                mapping_id: Some(11),
                model: "Ambientweather-F007TH".into(),
                reported_id: "202".into(),
                last_seen: timestamp("2026-01-10T10:38:36.174418Z"),
                latest_ulid: "01KEKQS34EA4YYEFK0H0PT36BJ".into(),
                description: Some("Varasto".into()),
                validity_start: Some(timestamp("2026-02-14T11:06:20.739080Z")),
                deleted: Some(true),
            },
        ]
    }

    fn sort_assignments_by_identity(
        mut assignments: Vec<DiscoveredDeviceAssignment>,
    ) -> Vec<DiscoveredDeviceAssignment> {
        assignments.sort_by(|left, right| {
            left.model
                .cmp(&right.model)
                .then(left.reported_id.cmp(&right.reported_id))
                .then(left.mapping_id.cmp(&right.mapping_id))
        });
        assignments
    }

    // Legacy compatibility behavior: one discovered physical identity may produce
    // multiple rows when it has mapping history. Deleted and active mappings are
    // returned separately. Phase 3 will replace this with explicit binding history.
    #[tokio::test]
    async fn test_list_assignments_populated_database_success() {
        let fixture = RepositoryFixture::new().await;
        fixture.populate_database().await;

        let res = fixture.repository.list_discovered_assignments().await;

        assert_eq!(
            sort_assignments_by_identity(res.unwrap()),
            sort_assignments_by_identity(populated_database_ground_truth())
        );

        fixture.shutdown().await;
    }

    #[tokio::test]
    async fn list_discovered_assignments_returns_persistence_error_for_incompatible_schema_missing_columns()
     {
        let fixture = RepositoryFixture::new().await;

        fixture
            .db_handle
            .query(
                r#"
            CREATE OR REPLACE VIEW all_sensors AS
            SELECT -- this is missing columns
                1 AS mapping_id,
                'model' AS model,
                'id' AS id
        "#
                .to_string(),
            )
            .await
            .expect("alter table in test database");

        let res = fixture.repository.list_discovered_assignments().await;
        fixture.shutdown().await;

        assert_eq!(res, Err(LegacyAssignmentRepositoryError::Persistence));
    }

    #[tokio::test]
    async fn list_discovered_assignments_ignores_unrequested_columns() {
        let fixture = RepositoryFixture::new().await;

        fixture
            .db_handle
            .query(
                r#"
            CREATE OR REPLACE VIEW all_sensors AS
            SELECT
                NULL::BIGINT AS mapping_id,
                '42' AS model,
                'sensor-id'::VARCHAR AS id,
                TIMESTAMP '2026-08-02 19:30:00' AS last_seen,
                '01KEKQQ5MGZTEQHF5PHBPEEW67'::VARCHAR AS latest_ulid,
                NULL::VARCHAR AS description,
                NULL::TIMESTAMP AS validity_start,
                NULL::BOOLEAN AS deleted,
                'EXTRA' as extra -- this is extra column that shouldn't matter
        "#
                .to_string(),
            )
            .await
            .expect("alter table in test database");

        let res = fixture.repository.list_discovered_assignments().await;
        fixture.shutdown().await;

        let expected_time = test_helper_time();

        assert_eq!(
            res,
            Ok(vec![DiscoveredDeviceAssignment {
                mapping_id: None,
                model: "42".into(),
                reported_id: "sensor-id".into(),
                last_seen: expected_time,
                latest_ulid: "01KEKQQ5MGZTEQHF5PHBPEEW67".into(),
                description: None,
                validity_start: None,
                deleted: None,
            }])
        );
    }

    #[tokio::test]
    async fn list_discovered_assignments_returns_serialization_error_for_incompatible_row_shape() {
        let fixture = RepositoryFixture::new().await;

        fixture
            .db_handle
            .query(
                r#"
            CREATE OR REPLACE VIEW all_sensors AS
            SELECT
                NULL::BIGINT AS mapping_id,
                42::BIGINT AS model, -- Sensorake expects this to be a String
                'sensor-id'::VARCHAR AS id,
                TIMESTAMP '2026-01-10 10:00:00' AS last_seen,
                '01KEKQQ5MGZTEQHF5PHBPEEW67'::VARCHAR AS latest_ulid,
                NULL::VARCHAR AS description,
                NULL::TIMESTAMP AS validity_start,
                NULL::BOOLEAN AS deleted;
        "#
                .to_string(),
            )
            .await
            .expect("alter table in test database");

        let res = fixture.repository.list_discovered_assignments().await;
        fixture.shutdown().await;

        assert_eq!(res, Err(LegacyAssignmentRepositoryError::Serialization));
    }

    #[tokio::test]
    async fn list_discovered_assignments_ignores_raw_messages_without_physical_identity() {
        let fixture = RepositoryFixture::new().await;

        let timestamp = |value: &str| {
            DateTime::parse_from_rfc3339(value)
                .unwrap()
                .with_timezone(&Utc)
        };
        let validity_start = timestamp("2026-01-10T00:00:00Z");

        let expected = vec![DiscoveredDeviceAssignment {
            mapping_id: Some(2),
            model: "LaCrosse-TX141Bv3".into(),
            reported_id: "254".into(),
            last_seen: timestamp("2026-01-10T10:38:14.877032Z"),
            latest_ulid: "01KEKQREAXF2W8SPTKVW7RD7C6".into(),
            description: Some("Aapon huone".into()),
            validity_start: Some(validity_start),
            deleted: Some(false),
        }];

        fixture
            .db_handle
            .query(
                r#"BEGIN;
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ5MGZTEQHF5PHBPEEW67', TIMESTAMP '2026-01-10 10:37:33.200231', '{"time":"1768041453.200231", "model":"Ambientweather-F007TH", "id": "", "temperature_C":22.61111, "humidity":2}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ5MGZTEQHF5PHBPEEW67', TIMESTAMP '2026-01-10 10:37:33.200231', '{"time":"1768041453.200231", "model": "", "id":44, "temperature_C":22.61111, "humidity":2, "mic":"CRC"}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ5MGZTEQHF5PHBPEEW67', TIMESTAMP '2026-01-10 10:37:33.200231', '{"time":"1768041453.200231", "model":"Ambientweather-F007TH", "id": "  ", "temperature_C":22.61111, "humidity":2}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ5MGZTEQHF5PHBPEEW67', TIMESTAMP '2026-01-10 10:37:33.200231', '{"time":"1768041453.200231", "model": "   ", "id":44, "temperature_C":22.61111, "humidity":2, "mic":"CRC"}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ5MGZTEQHF5PHBPEEW67', TIMESTAMP '2026-01-10 10:37:33.200231', '{"time":"1768041453.200231", "model":"Ambientweather-F007TH", "temperature_C":22.61111, "humidity":2}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQQ5MGZTEQHF5PHBPEEW67', TIMESTAMP '2026-01-10 10:37:33.200231', '{"time":"1768041453.200231", "id":44, "temperature_C":22.61111, "humidity":2, "mic":"CRC"}');
                   INSERT INTO data_landing (ulid, ts, raw_json) VALUES ('01KEKQREAXF2W8SPTKVW7RD7C6', TIMESTAMP '2026-01-10 10:38:14.877032', '{"time":"1768041494.877032", "model":"LaCrosse-TX141Bv3", "id":254, "temperature_C":20.2}');
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (2, 'LaCrosse-TX141Bv3', '254', 'Aapon huone', TIMESTAMP '2026-01-10 00:00:00', false);
                   INSERT INTO mappings (mapping_id, model, id, description, validity_start, deleted) VALUES (5, 'Ambientweather-F007TH', '44', 'Reitinkaappi', TIMESTAMP '2026-01-10 00:00:00', false);
                   COMMIT;"#.to_string()
            )
            .await
            .expect("populate repository test database");

        let res = fixture.repository.list_discovered_assignments().await;
        fixture.shutdown().await;

        assert_eq!(res, Ok(expected));
    }
}
