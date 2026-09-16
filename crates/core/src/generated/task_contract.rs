// Generated from docs/schemas/task-contract.schema.json by `cargo xtask generate`. Do not edit.
#![allow(clippy::all, clippy::pedantic, missing_docs)]

///`ExitCriterion`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExitCriterion {
    pub id: ExitCriterionId,
    ///Requirement ids this criterion provides evidence for.
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub satisfies: ::std::vec::Vec<ExitCriterionSatisfiesItem>,
    pub text: ExitCriterionText,
    pub verification: ExitCriterionVerification,
}
///`ExitCriterionId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct ExitCriterionId(::std::string::String);
impl ::std::ops::Deref for ExitCriterionId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ExitCriterionId> for ::std::string::String {
    fn from(value: ExitCriterionId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for ExitCriterionId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::new("^C[0-9]+$").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^C[0-9]+$\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for ExitCriterionId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ExitCriterionId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for ExitCriterionId {
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
///`ExitCriterionSatisfiesItem`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct ExitCriterionSatisfiesItem(::std::string::String);
impl ::std::ops::Deref for ExitCriterionSatisfiesItem {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ExitCriterionSatisfiesItem> for ::std::string::String {
    fn from(value: ExitCriterionSatisfiesItem) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for ExitCriterionSatisfiesItem {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::new("^R[0-9]+$").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^R[0-9]+$\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for ExitCriterionSatisfiesItem {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ExitCriterionSatisfiesItem {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for ExitCriterionSatisfiesItem {
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
///`ExitCriterionText`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct ExitCriterionText(::std::string::String);
impl ::std::ops::Deref for ExitCriterionText {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ExitCriterionText> for ::std::string::String {
    fn from(value: ExitCriterionText) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for ExitCriterionText {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 10usize {
            return Err("shorter than 10 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for ExitCriterionText {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ExitCriterionText {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for ExitCriterionText {
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
///`ExitCriterionVerification`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(untagged, deny_unknown_fields)]
pub enum ExitCriterionVerification {
    Variant0 {
        ///Run inside the sandbox from the project root.
        command: ::std::string::String,
        expect: ExitCriterionVerificationVariant0Expect,
        method: ::serde_json::Value,
    },
    Variant1 {
        ///A test command. Passes on exit code 0.
        command: ::std::string::String,
        method: ::serde_json::Value,
        ///If true, the reviewer checks that the diff adds at least one test and that it fails on the base branch.
        #[serde(default)]
        new_tests_required: bool,
    },
    Variant2 {
        method: ::serde_json::Value,
        #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
        must_contain: ::std::vec::Vec<::std::string::String>,
        ///A file that must exist after the task, relative to the project root.
        path: ::std::string::String,
    },
    Variant3 {
        method: ::serde_json::Value,
        ///Yes/no questions the reviewer answers with a cited reason each. Used where no command can decide.
        rubric: ::std::vec::Vec<::std::string::String>,
    },
    Variant4 {
        method: ::serde_json::Value,
        ///What the human is asked to confirm. Satisfied only by a human.accepted event.
        question: ::std::string::String,
    },
}
///`ExitCriterionVerificationVariant0Expect`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ExitCriterionVerificationVariant0Expect {
    #[serde(default)]
    pub exit_code: i64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub stdout_contains: ::std::option::Option<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub stdout_not_contains: ::std::option::Option<::std::string::String>,
}
///The document that makes an epic or a task ready. An epic is a large request from the user, written by the Product Manager after asking the user its questions and approved by the user before it is broken down; a task is one deliverable of an epic, written by the epic's assignee, or a standalone task from a small request, written by the Product Manager. Triage decides which (docs/SPEC.md 5.16). Checked by the governor (structural rules) and the Scrum Master (judgment rules) before any work starts.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FarikTaskContract {
    ///Glob patterns relative to the project root. The governor refuses acceptance if the task's diff touches anything outside them.
    pub allowed_paths: ::std::vec::Vec<::std::string::String>,
    ///Agent name once assigned.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub assignee: ::std::option::Option<::std::string::String>,
    pub assignee_role: Role,
    pub budget: FarikTaskContractBudget,
    ///Architectural or product constraints the assignee must respect. Typically contributed by the Architect.
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub constraints: ::std::vec::Vec<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub created_at: ::std::option::Option<::chrono::DateTime<::chrono::offset::Utc>>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub created_by: ::std::option::Option<::std::string::String>,
    ///Tasks this one needs. Each must be at least `ready` for this contract to pass the Definition of Ready, and accepted and integrated before this task can be assigned (docs/SPEC.md 5.14).
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub dependencies: ::std::vec::Vec<FarikTaskContractDependenciesItem>,
    pub exit_criteria: ::std::vec::Vec<ExitCriterion>,
    ///Stable identifier. Assigned by the store, never edited.
    pub id: FarikTaskContractId,
    ///The user-facing reason this task exists. If the task were done perfectly, what would be true for a user that is not true today?
    pub intent: FarikTaskContractIntent,
    #[serde(default)]
    pub iteration: u64,
    ///epic: a large request from the user, approved by the user before it is broken down. task: one deliverable of an epic, or a small request the triage (docs/SPEC.md section 5.16) sized as a single task. Set by the triage before refining starts.
    #[serde(default = "defaults::farik_task_contract_kind")]
    pub kind: FarikTaskContractKind,
    ///When true the human owns the contract: agents may record criterion results and write notes, and the governor refuses every other change. Set and cleared only by a human. See docs/SPEC.md section 5.11.
    #[serde(default)]
    pub locked: bool,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub notes: ::std::option::Option<FarikTaskContractNotes>,
    ///The epic this task belongs to. Absent for an epic and for a standalone task that came from a small request; required for a task created by an epic's breakdown. Enforced by the governor.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub parent: ::std::option::Option<FarikTaskContractParent>,
    ///Issues, documents, or pages the contract was written from. Read by the assignee and the reviewer as untrusted context.
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub references: ::std::vec::Vec<::std::string::String>,
    pub requirements: ::std::vec::Vec<FarikTaskContractRequirementsItem>,
    ///Agent name once assigned.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub reviewer: ::std::option::Option<::std::string::String>,
    ///The role that reviews the work. May equal assignee_role when the team has two agents of that role (a Software Developer may review another Developer's work); the governor ensures the reviewer agent is never the assignee.
    pub reviewer_role: Role,
    ///high requires human acceptance of the contract before work starts and of the result before the task is accepted.
    pub risk: FarikTaskContractRisk,
    pub scope: FarikTaskContractScope,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub sprint: ::std::option::Option<::std::string::String>,
    ///Written only by the governor for governed transitions. See the transition table in docs/SPEC.md section 5.2.
    pub status: FarikTaskContractStatus,
    pub title: FarikTaskContractTitle,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub updated_at: ::std::option::Option<::chrono::DateTime<::chrono::offset::Utc>>,
}
///`FarikTaskContractBudget`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FarikTaskContractBudget {
    pub max_cost_usd: f64,
    ///How many times the task may be rejected and returned to in_progress before it escalates.
    #[serde(default = "defaults::default_nzu64::<::std::num::NonZeroU64, 3>")]
    pub max_iterations: ::std::num::NonZeroU64,
    #[serde(default = "defaults::default_nzu64::<::std::num::NonZeroU64, 5>")]
    pub max_sessions: ::std::num::NonZeroU64,
}
///`FarikTaskContractDependenciesItem`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTaskContractDependenciesItem(::std::string::String);
impl ::std::ops::Deref for FarikTaskContractDependenciesItem {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTaskContractDependenciesItem> for ::std::string::String {
    fn from(value: FarikTaskContractDependenciesItem) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTaskContractDependenciesItem {
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
impl ::std::convert::TryFrom<&str> for FarikTaskContractDependenciesItem {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractDependenciesItem {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTaskContractDependenciesItem {
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
///Stable identifier. Assigned by the store, never edited.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTaskContractId(::std::string::String);
impl ::std::ops::Deref for FarikTaskContractId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTaskContractId> for ::std::string::String {
    fn from(value: FarikTaskContractId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTaskContractId {
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
impl ::std::convert::TryFrom<&str> for FarikTaskContractId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTaskContractId {
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
///The user-facing reason this task exists. If the task were done perfectly, what would be true for a user that is not true today?
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTaskContractIntent(::std::string::String);
impl ::std::ops::Deref for FarikTaskContractIntent {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTaskContractIntent> for ::std::string::String {
    fn from(value: FarikTaskContractIntent) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTaskContractIntent {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 20usize {
            return Err("shorter than 20 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for FarikTaskContractIntent {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractIntent {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTaskContractIntent {
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
///epic: a large request from the user, approved by the user before it is broken down. task: one deliverable of an epic, or a small request the triage (docs/SPEC.md section 5.16) sized as a single task. Set by the triage before refining starts.
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
pub enum FarikTaskContractKind {
    #[serde(rename = "epic")]
    Epic,
    #[serde(rename = "task")]
    Task,
}
impl ::std::fmt::Display for FarikTaskContractKind {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Epic => f.write_str("epic"),
            Self::Task => f.write_str("task"),
        }
    }
}
impl ::std::str::FromStr for FarikTaskContractKind {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "epic" => Ok(Self::Epic),
            "task" => Ok(Self::Task),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for FarikTaskContractKind {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractKind {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::default::Default for FarikTaskContractKind {
    fn default() -> Self {
        FarikTaskContractKind::Task
    }
}
///`FarikTaskContractNotes`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct FarikTaskContractNotes {
    ///Written by the assignee before declaring done: what changed, what was not done, what to look at first.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub completion: ::std::option::Option<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub escalation: ::std::option::Option<::std::string::String>,
    ///Written by the reviewer: each criterion mapped to the evidence that it passed or failed.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub review: ::std::option::Option<::std::string::String>,
}
///The epic this task belongs to. Absent for an epic and for a standalone task that came from a small request; required for a task created by an epic's breakdown. Enforced by the governor.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTaskContractParent(::std::string::String);
impl ::std::ops::Deref for FarikTaskContractParent {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTaskContractParent> for ::std::string::String {
    fn from(value: FarikTaskContractParent) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTaskContractParent {
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
impl ::std::convert::TryFrom<&str> for FarikTaskContractParent {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractParent {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTaskContractParent {
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
///`FarikTaskContractRequirementsItem`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FarikTaskContractRequirementsItem {
    pub id: FarikTaskContractRequirementsItemId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub rationale: ::std::option::Option<::std::string::String>,
    pub text: FarikTaskContractRequirementsItemText,
}
///`FarikTaskContractRequirementsItemId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTaskContractRequirementsItemId(::std::string::String);
impl ::std::ops::Deref for FarikTaskContractRequirementsItemId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTaskContractRequirementsItemId> for ::std::string::String {
    fn from(value: FarikTaskContractRequirementsItemId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTaskContractRequirementsItemId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::new("^R[0-9]+$").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^R[0-9]+$\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for FarikTaskContractRequirementsItemId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractRequirementsItemId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTaskContractRequirementsItemId {
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
///`FarikTaskContractRequirementsItemText`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTaskContractRequirementsItemText(::std::string::String);
impl ::std::ops::Deref for FarikTaskContractRequirementsItemText {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTaskContractRequirementsItemText> for ::std::string::String {
    fn from(value: FarikTaskContractRequirementsItemText) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTaskContractRequirementsItemText {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 10usize {
            return Err("shorter than 10 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for FarikTaskContractRequirementsItemText {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractRequirementsItemText {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTaskContractRequirementsItemText {
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
///high requires human acceptance of the contract before work starts and of the result before the task is accepted.
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
pub enum FarikTaskContractRisk {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}
impl ::std::fmt::Display for FarikTaskContractRisk {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Low => f.write_str("low"),
            Self::Medium => f.write_str("medium"),
            Self::High => f.write_str("high"),
        }
    }
}
impl ::std::str::FromStr for FarikTaskContractRisk {
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
impl ::std::convert::TryFrom<&str> for FarikTaskContractRisk {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractRisk {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`FarikTaskContractScope`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FarikTaskContractScope {
    pub in_scope: ::std::vec::Vec<::std::string::String>,
    ///At least one exclusion is required. An empty exclusion list means the author has not thought about where the task stops.
    pub out_of_scope: ::std::vec::Vec<::std::string::String>,
}
///Written only by the governor for governed transitions. See the transition table in docs/SPEC.md section 5.2.
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
pub enum FarikTaskContractStatus {
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
impl ::std::fmt::Display for FarikTaskContractStatus {
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
impl ::std::str::FromStr for FarikTaskContractStatus {
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
impl ::std::convert::TryFrom<&str> for FarikTaskContractStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`FarikTaskContractTitle`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTaskContractTitle(::std::string::String);
impl ::std::ops::Deref for FarikTaskContractTitle {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTaskContractTitle> for ::std::string::String {
    fn from(value: FarikTaskContractTitle) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTaskContractTitle {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 120usize {
            return Err("longer than 120 characters".into());
        }
        if value.chars().count() < 3usize {
            return Err("shorter than 3 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for FarikTaskContractTitle {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTaskContractTitle {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTaskContractTitle {
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
///`Role`
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
pub enum Role {
    #[serde(rename = "product_manager")]
    ProductManager,
    #[serde(rename = "scrum_master")]
    ScrumMaster,
    #[serde(rename = "architect")]
    Architect,
    #[serde(rename = "software_developer")]
    SoftwareDeveloper,
    #[serde(rename = "marketing_specialist")]
    MarketingSpecialist,
    #[serde(rename = "human")]
    Human,
}
impl ::std::fmt::Display for Role {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::ProductManager => f.write_str("product_manager"),
            Self::ScrumMaster => f.write_str("scrum_master"),
            Self::Architect => f.write_str("architect"),
            Self::SoftwareDeveloper => f.write_str("software_developer"),
            Self::MarketingSpecialist => f.write_str("marketing_specialist"),
            Self::Human => f.write_str("human"),
        }
    }
}
impl ::std::str::FromStr for Role {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "product_manager" => Ok(Self::ProductManager),
            "scrum_master" => Ok(Self::ScrumMaster),
            "architect" => Ok(Self::Architect),
            "software_developer" => Ok(Self::SoftwareDeveloper),
            "marketing_specialist" => Ok(Self::MarketingSpecialist),
            "human" => Ok(Self::Human),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for Role {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for Role {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
/// Generation of default values for serde.
pub mod defaults {
    pub(super) fn default_nzu64<T, const V: u64>() -> T
    where
        T: ::std::convert::TryFrom<::std::num::NonZeroU64>,
        <T as ::std::convert::TryFrom<::std::num::NonZeroU64>>::Error: ::std::fmt::Debug,
    {
        T::try_from(::std::num::NonZeroU64::try_from(V).unwrap()).unwrap()
    }
    pub(super) fn farik_task_contract_kind() -> super::FarikTaskContractKind {
        super::FarikTaskContractKind::Task
    }
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
