use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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
pub enum LegacyAssignmentServiceInvalidAssignmentField {
    Model,
    ReportedID,
    Description,
}

#[derive(Debug, PartialEq)]
pub enum LegacyAssignmentServiceError {
    Unexpected,
    AssignmentNotFound,
    AssignmentAlreadyExists,
    InvalidAssignment(LegacyAssignmentServiceInvalidAssignmentField),
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
        if command.model.trim().is_empty() {
            Err(LegacyAssignmentServiceError::InvalidAssignment(
                LegacyAssignmentServiceInvalidAssignmentField::Model,
            ))
        } else if command.reported_id.trim().is_empty() {
            Err(LegacyAssignmentServiceError::InvalidAssignment(
                LegacyAssignmentServiceInvalidAssignmentField::ReportedID,
            ))
        } else if command.description.trim().is_empty() {
            Err(LegacyAssignmentServiceError::InvalidAssignment(
                LegacyAssignmentServiceInvalidAssignmentField::Description,
            ))
        } else {
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
    use std::sync::{Arc, Mutex};

    enum FakeRepositoryResponse {
        Succeed,
        FailAlreadyExists,
        DoesNotExist,
        FailGeneric,
    }

    #[derive(Default, Debug, PartialEq)]
    struct FakeRepositoryCalls {
        list_assignments: usize,
        create_assignment: usize,
        soft_delete_assignment: usize,
        restore_assignment: usize,
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

    // Fake repository, which is instrumented so that its behavior can
    // be pre-determined and it records call counts to its
    // methods
    struct FakeRepository {
        discovered_assignments: Vec<DiscoveredDeviceAssignment>,

        pub list_discovered_assignments_response: FakeRepositoryResponse,
        pub create_assignment_response: FakeRepositoryResponse,
        pub soft_delete_response: FakeRepositoryResponse,
        pub restore_assignment_response: FakeRepositoryResponse,
        pub call_counts: Arc<Mutex<FakeRepositoryCalls>>,
        pub create_assignment_called_with: Arc<Mutex<Option<CreateLegacyAssignment>>>,
    }

    impl FakeRepository {
        pub fn new(
            call_counts: Arc<Mutex<FakeRepositoryCalls>>,
            create_assignment_called_with: Arc<Mutex<Option<CreateLegacyAssignment>>>,
        ) -> Self {
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
                call_counts,
                create_assignment_called_with,
            }
        }

        pub fn get_assignments(&self) -> Vec<DiscoveredDeviceAssignment> {
            self.discovered_assignments.clone()
        }
    }

    #[async_trait]
    impl LegacyAssignmentRepository for FakeRepository {
        async fn list_discovered_assignments(
            &self,
        ) -> Result<Vec<DiscoveredDeviceAssignment>, LegacyAssignmentRepositoryError> {
            self.call_counts.lock().unwrap().list_assignments += 1;

            match self.list_discovered_assignments_response {
                FakeRepositoryResponse::Succeed => Ok(self.discovered_assignments.clone()),
                _ => Err(LegacyAssignmentRepositoryError::General),
            }
        }

        async fn create_assignment(
            &self,
            command: CreateLegacyAssignment,
        ) -> Result<CreatedLegacyAssignment, LegacyAssignmentRepositoryError> {
            self.call_counts.lock().unwrap().create_assignment += 1;
            *self.create_assignment_called_with.lock().unwrap() = Some(command.clone());

            match self.create_assignment_response {
                FakeRepositoryResponse::Succeed => Ok(test_helper_create_expected_assignment()),
                FakeRepositoryResponse::FailAlreadyExists => {
                    Err(LegacyAssignmentRepositoryError::AssignmentAlreadyExists)
                }
                _ => Err(LegacyAssignmentRepositoryError::General),
            }
        }

        async fn soft_delete_assignment(
            &self,
            _mapping_id: i64,
        ) -> Result<(), LegacyAssignmentRepositoryError> {
            self.call_counts.lock().unwrap().soft_delete_assignment += 1;

            match self.soft_delete_response {
                FakeRepositoryResponse::Succeed => Ok(()),
                FakeRepositoryResponse::DoesNotExist => {
                    Err(LegacyAssignmentRepositoryError::AssignmentNotFound)
                }
                _ => Err(LegacyAssignmentRepositoryError::General),
            }
        }

        async fn restore_assignment(
            &self,
            _mapping_id: i64,
        ) -> Result<(), LegacyAssignmentRepositoryError> {
            self.call_counts.lock().unwrap().restore_assignment += 1;

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
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        LegacyAssignmentService::new(repo);

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn list_discovered_assignments_success() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());
        repo.list_discovered_assignments_response = FakeRepositoryResponse::Succeed;

        let assignments = repo.get_assignments();

        let app_svc = LegacyAssignmentService::new(repo);

        let res = app_svc.list_discovered_assignments().await;

        assert_eq!(res.unwrap(), assignments);

        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn list_discovered_assignments_failure() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());
        repo.list_discovered_assignments_response = FakeRepositoryResponse::FailGeneric;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.list_discovered_assignments().await;

        assert_eq!(res.err(), Some(LegacyAssignmentServiceError::Unexpected));

        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn create_assignment_success() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.create_assignment_response = FakeRepositoryResponse::Succeed;
        let app_svc = LegacyAssignmentService::new(repo);

        let res = app_svc.create_assignment(test_helper_assignment()).await;

        assert_eq!(res.unwrap(), test_helper_create_expected_assignment());

        assert_eq!(
            *create_assignment_called_with.lock().unwrap(),
            Some(test_helper_assignment())
        );
        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn create_assignment_already_exists() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.create_assignment_response = FakeRepositoryResponse::FailAlreadyExists;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(test_helper_assignment()).await;

        assert_eq!(
            res.err(),
            Some(LegacyAssignmentServiceError::AssignmentAlreadyExists)
        );
        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn create_assignment_generic_failure() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.create_assignment_response = FakeRepositoryResponse::FailGeneric;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(test_helper_assignment()).await;

        assert_eq!(res.err(), Some(LegacyAssignmentServiceError::Unexpected));

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn create_assignment_empty_model_field() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.create_assignment_response = FakeRepositoryResponse::FailGeneric;

        let mut assignment = test_helper_assignment();

        assignment.model = "  ".into();

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(assignment).await;

        match res.err() {
            Some(LegacyAssignmentServiceError::InvalidAssignment(f)) => {
                assert_eq!(f, LegacyAssignmentServiceInvalidAssignmentField::Model)
            }
            _ => panic!(),
        };

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn create_assignment_empty_reported_id_field() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.create_assignment_response = FakeRepositoryResponse::FailGeneric;

        let mut assignment = test_helper_assignment();

        assignment.reported_id = "  ".into();

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(assignment).await;

        match res.err() {
            Some(LegacyAssignmentServiceError::InvalidAssignment(f)) => {
                assert_eq!(f, LegacyAssignmentServiceInvalidAssignmentField::ReportedID)
            }
            _ => panic!(),
        };

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn create_assignment_empty_description_field() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());
        repo.create_assignment_response = FakeRepositoryResponse::FailGeneric;

        let mut assignment = test_helper_assignment();

        assignment.description = "  ".into();

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.create_assignment(assignment).await;

        match res.err() {
            Some(LegacyAssignmentServiceError::InvalidAssignment(f)) => {
                assert_eq!(
                    f,
                    LegacyAssignmentServiceInvalidAssignmentField::Description
                )
            }
            _ => panic!(),
        };

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn soft_delete_assignment_success() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.soft_delete_response = FakeRepositoryResponse::Succeed;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.soft_delete_assignment(1).await;

        assert_eq!(res.unwrap(), ());

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn soft_delete_assignment_failure_doesnotexit() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());
        repo.soft_delete_response = FakeRepositoryResponse::DoesNotExist;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.soft_delete_assignment(1).await;

        assert_eq!(
            res.err(),
            Some(LegacyAssignmentServiceError::AssignmentNotFound)
        );

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn soft_delete_assignment_failure_generic() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.soft_delete_response = FakeRepositoryResponse::FailGeneric;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.soft_delete_assignment(1).await;

        assert_eq!(res.err(), Some(LegacyAssignmentServiceError::Unexpected));

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().restore_assignment, 0);
    }

    #[tokio::test]
    async fn restore_assignment_success() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.restore_assignment_response = FakeRepositoryResponse::Succeed;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.restore_assignment(1).await;

        assert_eq!(res.unwrap(), ());

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
    }

    #[tokio::test]
    async fn restore_assignment_failure_doesnotexit() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.restore_assignment_response = FakeRepositoryResponse::DoesNotExist;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.restore_assignment(1).await;

        assert_eq!(
            res.err(),
            Some(LegacyAssignmentServiceError::AssignmentNotFound)
        );

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
    }

    #[tokio::test]
    async fn restore_assignment_failure_generic() {
        let calls = Arc::new(Mutex::new(FakeRepositoryCalls::default()));
        let create_assignment_called_with = Arc::new(Mutex::new(None));

        let mut repo = FakeRepository::new(calls.clone(), create_assignment_called_with.clone());

        repo.restore_assignment_response = FakeRepositoryResponse::FailGeneric;

        let app_svc = LegacyAssignmentService::new(repo);
        let res = app_svc.restore_assignment(1).await;

        assert_eq!(res.err(), Some(LegacyAssignmentServiceError::Unexpected));

        assert_eq!(calls.lock().unwrap().list_assignments, 0);
        assert_eq!(calls.lock().unwrap().create_assignment, 0);
        assert_eq!(calls.lock().unwrap().soft_delete_assignment, 0);
    }
}
