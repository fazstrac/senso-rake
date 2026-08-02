use async_trait::async_trait;
use chrono::{DateTime, Utc};

pub struct CreateLegacyAssignment {
    pub model: String,
    pub reported_id: String,
    pub description: String,
    pub validity_start: DateTime<Utc>,
}

// TODO Change this according to need
pub struct CreatedLegacyAssignment {
    pub model: String,
    pub reported_id: String,
    pub description: String,
    pub validity_start: DateTime<Utc>,
}

pub struct DiscoveredDeviceAssignment {
    pub mapping_id: Option<i64>,
    pub model: String,
    pub reported_id: String,
    pub last_seen: DateTime<Utc>,
    pub latest_ulid: String,
    pub description: Option<String>,
    pub validity_start: Option<DateTime<Utc>>,
    pub deleted: Option<bool>,
}

pub enum LegacyAssignmentRepositoryError {
    Error,
}

#[derive(Debug, PartialEq)]
pub enum LegacyAssignmentServiceError {
    Error,
}

pub struct LegacyAssignmentService<R> {
    repository: R,
}

#[async_trait]
pub trait LegacyAssignmentRepository {
    async fn list_discovered_assignments(
        &self,
    ) -> Result<Vec<DiscoveredDeviceAssignment>, LegacyAssignmentRepositoryError>;
    async fn create_assignment(
        &self,
        command: CreateLegacyAssignment,
    ) -> Result<CreatedLegacyAssignment, LegacyAssignmentRepositoryError>;
    async fn soft_delete_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentRepositoryError>;
    async fn restore_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentRepositoryError>;
}

impl<R> LegacyAssignmentService<R>
where
    R: LegacyAssignmentRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn list_discovered_assignments(
        &self,
    ) -> Result<Vec<DiscoveredDeviceAssignment>, LegacyAssignmentServiceError> {
        self.repository
            .list_discovered_assignments()
            .await
            .map_err(|_e| LegacyAssignmentServiceError::Error)
    }

    pub async fn create_assignment(
        &self,
        command: CreateLegacyAssignment,
    ) -> Result<CreatedLegacyAssignment, LegacyAssignmentServiceError> {
        self.repository
            .create_assignment(command)
            .await
            .map_err(|_e| LegacyAssignmentServiceError::Error)
    }

    pub async fn soft_delete_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentServiceError> {
        self.repository
            .soft_delete_assignment(mapping_id)
            .await
            .map_err(|_e| LegacyAssignmentServiceError::Error)
    }

    pub async fn restore_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentServiceError> {
        self.repository
            .restore_assignment(mapping_id)
            .await
            .map_err(|_e| LegacyAssignmentServiceError::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Utc};

    // TODO: Implement instrumentation to orchestrate FakeRepository's behavior in tests
    struct FakeRepository {}

    impl FakeRepository {
        pub fn new() -> Self {
            Self {}
        }
    }

    #[async_trait]
    impl LegacyAssignmentRepository for FakeRepository {
        async fn list_discovered_assignments(
            &self,
        ) -> Result<Vec<DiscoveredDeviceAssignment>, LegacyAssignmentRepositoryError> {
            let ts1 = NaiveDate::from_ymd_opt(2026, 8, 2)
                .unwrap()
                .and_hms_opt(19, 30, 0)
                .unwrap()
                .and_local_timezone(Utc)
                .unwrap();

            Ok(vec![DiscoveredDeviceAssignment {
                mapping_id: Some(1),
                model: "Mock sensor".into(),
                reported_id: "Mock reported id".into(),
                last_seen: ts1,
                latest_ulid: "mock ulid".into(),
                description: Some("Mock description".into()),
                validity_start: Some(ts1),
                deleted: None,
            }])
        }

        #[allow(unused_variables)]
        async fn create_assignment(
            &self,
            command: CreateLegacyAssignment,
        ) -> Result<CreatedLegacyAssignment, LegacyAssignmentRepositoryError> {
            Ok(CreatedLegacyAssignment {
                model: command.model,
                reported_id: command.reported_id,
                description: command.description,
                validity_start: command.validity_start,
            })
        }

        #[allow(unused_variables)]
        async fn soft_delete_assignment(
            &self,
            mapping_id: i64,
        ) -> Result<(), LegacyAssignmentRepositoryError> {
            Ok(())
        }

        #[allow(unused_variables)]
        async fn restore_assignment(
            &self,
            mapping_id: i64,
        ) -> Result<(), LegacyAssignmentRepositoryError> {
            Ok(())
        }
    }

    #[test]
    fn smoke_test() {
        // Just check that this compiles
    }

    #[tokio::test]
    async fn list_discovered_assignments() {
        let app_svc = LegacyAssignmentService::new(FakeRepository::new());

        let _res = app_svc.list_discovered_assignments().await.unwrap();
    }
}
