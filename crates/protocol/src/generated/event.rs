// Generated from docs/schemas/event.schema.json by `cargo xtask generate`. Do not edit.
#![allow(clippy::all, clippy::pedantic, missing_docs)]

///A human took ownership of a contract (docs/SPEC.md section 5.11).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContractLockedBody {
    pub locked_by: ::std::string::String,
}
///The fields of a contract the board shows, repeated on every event that writes one so that the projections can be rebuilt from the log alone (docs/SPEC.md section 8.4). The vocabularies match task-contract.schema.json, which is the source of truth for them; a test in farik-protocol fails when the two drift.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContractSummary {
    pub kind: ContractSummaryKind,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub parent: ::std::option::Option<ContractSummaryParent>,
    pub risk: ContractSummaryRisk,
    pub status: ContractSummaryStatus,
    pub title: ::std::string::String,
}
///`ContractSummaryKind`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum ContractSummaryKind {
    #[serde(rename = "epic")]
    Epic,
    #[serde(rename = "task")]
    Task,
}
impl ::std::fmt::Display for ContractSummaryKind {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Epic => f.write_str("epic"),
            Self::Task => f.write_str("task"),
        }
    }
}
impl ::std::str::FromStr for ContractSummaryKind {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "epic" => Ok(Self::Epic),
            "task" => Ok(Self::Task),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for ContractSummaryKind {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ContractSummaryKind {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`ContractSummaryParent`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct ContractSummaryParent(::std::string::String);
impl ::std::ops::Deref for ContractSummaryParent {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ContractSummaryParent> for ::std::string::String {
    fn from(value: ContractSummaryParent) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for ContractSummaryParent {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::new("^FRK-[0-9]{1,6}$").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^FRK-[0-9]{1,6}$\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for ContractSummaryParent {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ContractSummaryParent {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for ContractSummaryParent {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::string::String::deserialize(deserializer)?
            .parse()
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`ContractSummaryRisk`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum ContractSummaryRisk {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}
impl ::std::fmt::Display for ContractSummaryRisk {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Low => f.write_str("low"),
            Self::Medium => f.write_str("medium"),
            Self::High => f.write_str("high"),
        }
    }
}
impl ::std::str::FromStr for ContractSummaryRisk {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for ContractSummaryRisk {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ContractSummaryRisk {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`ContractSummaryStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum ContractSummaryStatus {
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "refining")]
    Refining,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "assigned")]
    Assigned,
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "blocked")]
    Blocked,
    #[serde(rename = "verifying")]
    Verifying,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "escalated")]
    Escalated,
    #[serde(rename = "cancelled")]
    Cancelled,
}
impl ::std::fmt::Display for ContractSummaryStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Draft => f.write_str("draft"),
            Self::Refining => f.write_str("refining"),
            Self::Ready => f.write_str("ready"),
            Self::Assigned => f.write_str("assigned"),
            Self::InProgress => f.write_str("in_progress"),
            Self::Blocked => f.write_str("blocked"),
            Self::Verifying => f.write_str("verifying"),
            Self::Rejected => f.write_str("rejected"),
            Self::Accepted => f.write_str("accepted"),
            Self::Escalated => f.write_str("escalated"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}
impl ::std::str::FromStr for ContractSummaryStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "draft" => Ok(Self::Draft),
            "refining" => Ok(Self::Refining),
            "ready" => Ok(Self::Ready),
            "assigned" => Ok(Self::Assigned),
            "in_progress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "verifying" => Ok(Self::Verifying),
            "rejected" => Ok(Self::Rejected),
            "accepted" => Ok(Self::Accepted),
            "escalated" => Ok(Self::Escalated),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for ContractSummaryStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ContractSummaryStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///A human gave a contract back to the team (docs/SPEC.md section 5.11).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContractUnlockedBody {
    pub unlocked_by: ::std::string::String,
}
///A contract's content was written or changed.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContractWrittenBody {
    pub summary: ContractSummary,
    pub written_by: ::std::string::String,
}
///The criterion library was written (docs/SPEC.md section 5.13).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CriteriaUpdatedBody {
    pub criterion_names: ::std::vec::Vec<::std::string::String>,
    pub updated_by: ::std::string::String,
}
///Reconciliation found the files and the log disagreeing (docs/SPEC.md section 8.4).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DriftDetectedBody {
    pub detail: ::std::string::String,
    pub drift: DriftDetectedBodyDrift,
}
///`DriftDetectedBodyDrift`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum DriftDetectedBodyDrift {
    #[serde(rename = "contract_without_events")]
    ContractWithoutEvents,
    #[serde(rename = "events_without_contract")]
    EventsWithoutContract,
    #[serde(rename = "status_mismatch")]
    StatusMismatch,
}
impl ::std::fmt::Display for DriftDetectedBodyDrift {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::ContractWithoutEvents => f.write_str("contract_without_events"),
            Self::EventsWithoutContract => f.write_str("events_without_contract"),
            Self::StatusMismatch => f.write_str("status_mismatch"),
        }
    }
}
impl ::std::str::FromStr for DriftDetectedBodyDrift {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "contract_without_events" => Ok(Self::ContractWithoutEvents),
            "events_without_contract" => Ok(Self::EventsWithoutContract),
            "status_mismatch" => Ok(Self::StatusMismatch),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for DriftDetectedBodyDrift {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for DriftDetectedBodyDrift {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///Every body shape the log holds. The branches have distinct required properties and none accepts an unknown one, so a body matches exactly one. Which one it must be is decided by kind, in the reader.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum EventBodyWire {
    TaskCreatedBody(TaskCreatedBody),
    RequestTriagedBody(RequestTriagedBody),
    ContractWrittenBody(ContractWrittenBody),
    ContractLockedBody(ContractLockedBody),
    ContractUnlockedBody(ContractUnlockedBody),
    DriftDetectedBody(DriftDetectedBody),
    ProjectScannedBody(ProjectScannedBody),
    TeamUpdatedBody(TeamUpdatedBody),
    CriteriaUpdatedBody(CriteriaUpdatedBody),
}
impl ::std::convert::From<TaskCreatedBody> for EventBodyWire {
    fn from(value: TaskCreatedBody) -> Self {
        Self::TaskCreatedBody(value)
    }
}
impl ::std::convert::From<RequestTriagedBody> for EventBodyWire {
    fn from(value: RequestTriagedBody) -> Self {
        Self::RequestTriagedBody(value)
    }
}
impl ::std::convert::From<ContractWrittenBody> for EventBodyWire {
    fn from(value: ContractWrittenBody) -> Self {
        Self::ContractWrittenBody(value)
    }
}
impl ::std::convert::From<ContractLockedBody> for EventBodyWire {
    fn from(value: ContractLockedBody) -> Self {
        Self::ContractLockedBody(value)
    }
}
impl ::std::convert::From<ContractUnlockedBody> for EventBodyWire {
    fn from(value: ContractUnlockedBody) -> Self {
        Self::ContractUnlockedBody(value)
    }
}
impl ::std::convert::From<DriftDetectedBody> for EventBodyWire {
    fn from(value: DriftDetectedBody) -> Self {
        Self::DriftDetectedBody(value)
    }
}
impl ::std::convert::From<ProjectScannedBody> for EventBodyWire {
    fn from(value: ProjectScannedBody) -> Self {
        Self::ProjectScannedBody(value)
    }
}
impl ::std::convert::From<TeamUpdatedBody> for EventBodyWire {
    fn from(value: TeamUpdatedBody) -> Self {
        Self::TeamUpdatedBody(value)
    }
}
impl ::std::convert::From<CriteriaUpdatedBody> for EventBodyWire {
    fn from(value: CriteriaUpdatedBody) -> Self {
        Self::CriteriaUpdatedBody(value)
    }
}
///`EventKind`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum EventKind {
    #[serde(rename = "task.created")]
    TaskCreated,
    #[serde(rename = "request.triaged")]
    RequestTriaged,
    #[serde(rename = "contract.written")]
    ContractWritten,
    #[serde(rename = "contract.locked")]
    ContractLocked,
    #[serde(rename = "contract.unlocked")]
    ContractUnlocked,
    #[serde(rename = "drift.detected")]
    DriftDetected,
    #[serde(rename = "project.scanned")]
    ProjectScanned,
    #[serde(rename = "team.updated")]
    TeamUpdated,
    #[serde(rename = "criteria.updated")]
    CriteriaUpdated,
}
impl ::std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::TaskCreated => f.write_str("task.created"),
            Self::RequestTriaged => f.write_str("request.triaged"),
            Self::ContractWritten => f.write_str("contract.written"),
            Self::ContractLocked => f.write_str("contract.locked"),
            Self::ContractUnlocked => f.write_str("contract.unlocked"),
            Self::DriftDetected => f.write_str("drift.detected"),
            Self::ProjectScanned => f.write_str("project.scanned"),
            Self::TeamUpdated => f.write_str("team.updated"),
            Self::CriteriaUpdated => f.write_str("criteria.updated"),
        }
    }
}
impl ::std::str::FromStr for EventKind {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "task.created" => Ok(Self::TaskCreated),
            "request.triaged" => Ok(Self::RequestTriaged),
            "contract.written" => Ok(Self::ContractWritten),
            "contract.locked" => Ok(Self::ContractLocked),
            "contract.unlocked" => Ok(Self::ContractUnlocked),
            "drift.detected" => Ok(Self::DriftDetected),
            "project.scanned" => Ok(Self::ProjectScanned),
            "team.updated" => Ok(Self::TeamUpdated),
            "criteria.updated" => Ok(Self::CriteriaUpdated),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for EventKind {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for EventKind {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///One record in Farik's append-only log (docs/SPEC.md section 8.5). The envelope says where the event belongs and when it was recorded; kind says what happened and body carries that kind's payload. The pairing of kind and body is checked by the reader, farik_protocol::event::event_from_value, and not here: a schema that pairs them with if and then cannot be turned into Rust types by the generator.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FarikEvent {
    ///The agent whose work produced the event, when an agent did.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub agent_id: ::std::option::Option<::std::string::String>,
    pub body: EventBodyWire,
    pub kind: EventKind,
    ///The project the event belongs to. A blank one is refused by the reader.
    pub project_id: ::std::string::String,
    ///When the event was recorded, from the injected clock. Never read from the machine's clock by the crate that builds the event.
    pub recorded_at: ::chrono::DateTime<::chrono::offset::Utc>,
    ///The event's place in the log. Assigned by the store on append and never reused.
    pub seq: u64,
    ///The session the event was recorded in, when it was recorded in one.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub session_id: ::std::option::Option<::std::string::String>,
    ///The contract the event is about, when it is about one.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub task_id: ::std::option::Option<FarikEventTaskId>,
    ///The team the event belongs to. A blank one is refused by the reader.
    pub team_id: ::std::string::String,
}
///The contract the event is about, when it is about one.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikEventTaskId(::std::string::String);
impl ::std::ops::Deref for FarikEventTaskId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikEventTaskId> for ::std::string::String {
    fn from(value: FarikEventTaskId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikEventTaskId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::new("^FRK-[0-9]{1,6}$").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^FRK-[0-9]{1,6}$\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for FarikEventTaskId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikEventTaskId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikEventTaskId {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::string::String::deserialize(deserializer)?
            .parse()
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///The project scan read the repository back to the user and proposed criteria for the library (docs/SPEC.md section 5.13).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectScannedBody {
    pub detected_criteria: ::std::vec::Vec<::std::string::String>,
    pub read_back: ::std::string::String,
}
///Triage sized a request (docs/SPEC.md section 5.16).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RequestTriagedBody {
    pub reason: ::std::string::String,
    pub size: RequestTriagedBodySize,
    pub triaged_by: ::std::string::String,
}
///`RequestTriagedBodySize`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum RequestTriagedBodySize {
    #[serde(rename = "large")]
    Large,
    #[serde(rename = "small")]
    Small,
}
impl ::std::fmt::Display for RequestTriagedBodySize {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Large => f.write_str("large"),
            Self::Small => f.write_str("small"),
        }
    }
}
impl ::std::str::FromStr for RequestTriagedBodySize {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "large" => Ok(Self::Large),
            "small" => Ok(Self::Small),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for RequestTriagedBodySize {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for RequestTriagedBodySize {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///A request was filed as a draft contract (docs/SPEC.md section 5.16 item 1).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskCreatedBody {
    ///The agent id that filed the request, or human.
    pub created_by: ::std::string::String,
    pub summary: ContractSummary,
}
///The team file was written.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TeamUpdatedBody {
    pub agent_ids: ::std::vec::Vec<::std::string::String>,
    pub team_name: ::std::string::String,
    pub updated_by: ::std::string::String,
}
/// Error types.
pub mod error {
    /// Error from a `TryFrom` or `FromStr` implementation.
    pub struct ConversionError(::std::borrow::Cow<'static, str>);
    impl ::std::error::Error for ConversionError {}
    impl ::std::fmt::Display for ConversionError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
            ::std::fmt::Display::fmt(&self.0, f)
        }
    }
    impl ::std::fmt::Debug for ConversionError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
            ::std::fmt::Debug::fmt(&self.0, f)
        }
    }
    impl From<&'static str> for ConversionError {
        fn from(value: &'static str) -> Self {
            Self(value.into())
        }
    }
    impl From<String> for ConversionError {
        fn from(value: String) -> Self {
            Self(value.into())
        }
    }
}
