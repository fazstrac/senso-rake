use async_trait::async_trait;
use chrono::{DateTime, Utc};

pub struct CreateLegacyAssignment {
    pub model: String,
    pub reported_id: String,
    pub description: String,
    pub validity_start: DateTime<Utc>,
}

// TODO Change this according to need
#[derive(Debug, PartialEq)]
pub struct CreatedLegacyAssignment {
    pub mapping_id: i64,
    pub model: String,
    pub reported_id: String,
    pub description: String,
    pub validity_start: DateTime<Utc>,
}

#[derive(Clone, Debug)]
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
    General,
    AssignmentNotFound,
    AssignmentAlreadyExists,
}

#[derive(Debug, PartialEq)]
pub enum LegacyAssignmentServiceError {
    Unexpected,
    AssignmentNotFound,
    AssignmentAlreadyExists,
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
            // list_discovered_assignments should not error for business reasons,
            // but there may be other errors related to connections
            .map_err(|_| LegacyAssignmentServiceError::Unexpected)
    }

    pub async fn create_assignment(
        &self,
        command: CreateLegacyAssignment,
    ) -> Result<CreatedLegacyAssignment, LegacyAssignmentServiceError> {
        self.repository
            .create_assignment(command)
            .await
            .map_err(|e| match e {
                LegacyAssignmentRepositoryError::AssignmentAlreadyExists => {
                    LegacyAssignmentServiceError::AssignmentAlreadyExists
                }
                _ => LegacyAssignmentServiceError::Unexpected,
            })
    }

    pub async fn soft_delete_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentServiceError> {
        self.repository
            .soft_delete_assignment(mapping_id)
            .await
            .map_err(|e| match e {
                LegacyAssignmentRepositoryError::AssignmentNotFound => {
                    LegacyAssignmentServiceError::AssignmentNotFound
                }
                _ => LegacyAssignmentServiceError::Unexpected,
            })
    }

    pub async fn restore_assignment(
        &self,
        mapping_id: i64,
    ) -> Result<(), LegacyAssignmentServiceError> {
        self.repository
            .restore_assignment(mapping_id)
            .await
            .map_err(|e| match e {
                LegacyAssignmentRepositoryError::AssignmentNotFound => {
                    LegacyAssignmentServiceError::AssignmentNotFound
                }
                _ => LegacyAssignmentServiceError::Unexpected,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Utc};

    enum FakeRepositoryResponse {
        Succeed,
        FailAlreadyExists,
        DoesNotExist,
        FailGeneric,
    }

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

    // TODO: Implement instrumentation to orchestrate FakeRepository's behavior in tests
    struct FakeRepository {
        discovered_assignments: Vec<DiscoveredDeviceAssignment>,
        pub list_discovered_assignments_response: FakeRepositoryResponse,
        pub create_assignment_response: FakeRepositoryResponse,
        pub soft_delete_response: FakeRepositoryResponse,
        pub restore_assignment_response: FakeRepositoryResponse,
    }

    impl FakeRepository {
        pub fn new() -> Self {
            let ts1 = test_helper_time();

            Self {
                discovered_assignments: vec![DiscoveredDeviceAssignment {
                    mapping_id: Some(1),
                    model: "Mock sensor".into(),
                    reported_id: "Mock reported id".into(),
                    last_seen: ts1,
                    latest_ulid: "mock ulid".into(),
                    description: Some("Mock description".into()),
                    validity_start: Some(ts1),
                    deleted: None,
                }],
                list_discovered_assignments_response: FakeRepositoryResponse::Succeed,
                create_assignment_response: FakeRepositoryResponse::Succeed,
                soft_delete_response: FakeRepositoryResponse::Succeed,
                restore_assignment_response: FakeRepositoryResponse::Succeed,
            }
        }
    }

    #[async_trait]
    impl LegacyAssignmentRepository for FakeRepository {
        async fn list_discovered_assignments(
            &self,
        ) -> Result<Vec<DiscoveredDeviceAssignment>, LegacyAssignmentRepositoryError> {
            match self.list_discovered_assignments_response {
                FakeRepositoryResponse::Succeed => Ok(self.discovered_assignments.clone()),
                _ => Err(LegacyAssignmentRepositoryError::General),
            }
        }

        #[allow(unused_variables)]
        async fn create_assignment(
            &self,
            command: CreateLegacyAssignment,
        ) -> Result<CreatedLegacyAssignment, LegacyAssignmentRepositoryError> {
            match self.create_assignment_response {
                FakeRepositoryResponse::Succeed => Ok(CreatedLegacyAssignment {
                    mapping_id: 1,
                    model: command.model,
                    reported_id: command.reported_id,
                    description: command.description,
                    validity_start: command.validity_start,
                }),
                FakeRepositoryResponse::FailAlreadyExists => {
                    Err(LegacyAssignmentRepositoryError::AssignmentAlreadyExists)
                }
                _ => Err(LegacyAssignmentRepositoryError::General),
            }
        }

        #[allow(unused_variables)]
        async fn soft_delete_assignment(
            &self,
            mapping_id: i64,
        ) -> Result<(), LegacyAssignmentRepositoryError> {
            match self.soft_delete_response {
                FakeRepositoryResponse::Succeed => Ok(()),
                FakeRepositoryResponse::DoesNotExist => {
                    Err(LegacyAssignmentRepositoryError::AssignmentNotFound)
                }
                _ => Err(LegacyAssignmentRepositoryError::General),
            }
        }

        #[allow(unused_variables)]
        async fn restore_assignment(
            &self,
            mapping_id: i64,
        ) -> Result<(), LegacyAssignmentRepositoryError> {
            match self.restore_assignment_response {
                FakeRepositoryResponse::Succeed => Ok(()),
                FakeRepositoryResponse::DoesNotExist => {
                    Err(LegacyAssignmentRepositoryError::AssignmentNotFound)
                }
                _ => Err(LegacyAssignmentRepositoryError::General),
            }
        }
    }

    #[test]
    fn smoke_test() {
        // Just check that this compiles
    }

    #[tokio::test]
    async fn list_discovered_assignments_success() {
        let mut repo = FakeRepository::new();
        repo.list_discovered_assignments_response = FakeRepositoryResponse::Succeed;

        let app_svc = LegacyAssignmentService::new(repo);

        let res = app_svc.list_discovered_assignments().await;

        res.expect("result should contain value");
    }

    #[tokio::test]
    async fn list_discovered_assignments_failure() {
        let mut repo = FakeRepository::new();
        repo.list_discovered_assignments_response = FakeRepositoryResponse::FailGeneric;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.list_discovered_assignments().await;

        res.expect_err("result should contain error");
    }

    #[tokio::test]
    async fn create_assignment_success() {
        let mut repo = FakeRepository::new();
        repo.create_assignment_response = FakeRepositoryResponse::Succeed;

        let assignment = test_helper_assignment();

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(assignment).await;

        res.expect("result should contain value");
    }

    #[tokio::test]
    async fn create_assignment_already_exists() {
        let mut repo = FakeRepository::new();
        repo.create_assignment_response = FakeRepositoryResponse::FailAlreadyExists;

        let assignment = test_helper_assignment();

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(assignment).await;

        res.expect_err("result should contain error");
    }

    #[tokio::test]
    async fn create_assignment_generic_failure() {
        let mut repo = FakeRepository::new();
        repo.create_assignment_response = FakeRepositoryResponse::FailGeneric;

        let assignment = test_helper_assignment();

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(assignment).await;

        res.expect_err("result should contain error");
    }

    #[tokio::test]
    async fn soft_delete_assignment_success() {
        let mut repo = FakeRepository::new();
        repo.soft_delete_response = FakeRepositoryResponse::Succeed;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.soft_delete_assignment(1).await;

        res.expect("result should contain value");
    }

    #[tokio::test]
    async fn soft_delete_assignment_failure_doesnotexit() {
        let mut repo = FakeRepository::new();
        repo.soft_delete_response = FakeRepositoryResponse::DoesNotExist;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.soft_delete_assignment(1).await;

        res.expect_err("result should contain error");
    }

    #[tokio::test]
    async fn soft_delete_assignment_failure_generic() {
        let mut repo = FakeRepository::new();
        repo.soft_delete_response = FakeRepositoryResponse::FailGeneric;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.soft_delete_assignment(1).await;

        res.expect_err("result should contain error");
    }

    #[tokio::test]
    async fn restore_assignment_success() {
        let mut repo = FakeRepository::new();
        repo.restore_assignment_response = FakeRepositoryResponse::Succeed;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.restore_assignment(1).await;

        res.expect("result should contain value");
    }

    #[tokio::test]
    async fn restore_assignment_failure_doesnotexit() {
        let mut repo = FakeRepository::new();
        repo.restore_assignment_response = FakeRepositoryResponse::DoesNotExist;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.restore_assignment(1).await;

        res.expect_err("result should contain error");
    }

    #[tokio::test]
    async fn restore_assignment_failure_generic() {
        let mut repo = FakeRepository::new();
        repo.restore_assignment_response = FakeRepositoryResponse::FailGeneric;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.restore_assignment(1).await;

        res.expect_err("result should contain error");
    }
}
