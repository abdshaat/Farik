// Generated from docs/schemas/team.schema.json by `cargo xtask generate`. Do not edit.
#![allow(clippy::all, clippy::pedantic, missing_docs)]

///`Agent`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    ///The name of a shipped avatar or a path under .farik/team/avatars/.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub avatar: ::std::option::Option<AgentAvatar>,
    pub display_name: AgentDisplayName,
    ///Permission tiers this agent holds on top of its role's defaults (spec 5.6).
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub grants: ::std::option::Option<::std::vec::Vec<PermissionTier>>,
    ///A kebab-case slug of the display name, unique within the team. Events and contracts refer to an agent by it, so it never changes.
    pub id: AgentId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub model: ::std::option::Option<Model>,
    ///A few lines of character added to the role's system prompt. It never grants anything.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub persona: ::std::option::Option<AgentPersona>,
    ///External-effect tools this agent may use without asking the human each time (spec 5.6).
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub preauthorized_external_tools: ::std::vec::Vec<AgentPreauthorizedExternalToolsItem>,
    ///Permission tiers this agent does not hold, whatever its role's defaults say (spec 5.6: a role's tiers are the user's to override, which means taking one away as well as adding one). Taking away wins over granting.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub revokes: ::std::option::Option<::std::vec::Vec<PermissionTier>>,
    pub role: Role,
    ///A paused agent keeps its work and takes none; a retired one is kept only so that its past events still name someone.
    pub status: AgentStatus,
}
///The name of a shipped avatar or a path under .farik/team/avatars/.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AgentAvatar(::std::string::String);
impl ::std::ops::Deref for AgentAvatar {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AgentAvatar> for ::std::string::String {
    fn from(value: AgentAvatar) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AgentAvatar {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 200usize {
            return Err("longer than 200 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for AgentAvatar {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AgentAvatar {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for AgentAvatar {
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
///`AgentDisplayName`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AgentDisplayName(::std::string::String);
impl ::std::ops::Deref for AgentDisplayName {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AgentDisplayName> for ::std::string::String {
    fn from(value: AgentDisplayName) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AgentDisplayName {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 100usize {
            return Err("longer than 100 characters".into());
        }
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for AgentDisplayName {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AgentDisplayName {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for AgentDisplayName {
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
///A kebab-case slug of the display name, unique within the team. Events and contracts refer to an agent by it, so it never changes.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AgentId(::std::string::String);
impl ::std::ops::Deref for AgentId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AgentId> for ::std::string::String {
    fn from(value: AgentId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AgentId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 64usize {
            return Err("longer than 64 characters".into());
        }
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| {
                ::regress::Regex::new("^[a-z0-9]+(-[a-z0-9]+)*$").unwrap()
            });
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^[a-z0-9]+(-[a-z0-9]+)*$\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for AgentId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AgentId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for AgentId {
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
///A few lines of character added to the role's system prompt. It never grants anything.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AgentPersona(::std::string::String);
impl ::std::ops::Deref for AgentPersona {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AgentPersona> for ::std::string::String {
    fn from(value: AgentPersona) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AgentPersona {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 2000usize {
            return Err("longer than 2000 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for AgentPersona {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AgentPersona {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for AgentPersona {
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
///`AgentPreauthorizedExternalToolsItem`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AgentPreauthorizedExternalToolsItem(::std::string::String);
impl ::std::ops::Deref for AgentPreauthorizedExternalToolsItem {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AgentPreauthorizedExternalToolsItem> for ::std::string::String {
    fn from(value: AgentPreauthorizedExternalToolsItem) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AgentPreauthorizedExternalToolsItem {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for AgentPreauthorizedExternalToolsItem {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AgentPreauthorizedExternalToolsItem {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for AgentPreauthorizedExternalToolsItem {
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
///A paused agent keeps its work and takes none; a retired one is kept only so that its past events still name someone.
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
pub enum AgentStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "retired")]
    Retired,
}
impl ::std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Active => f.write_str("active"),
            Self::Paused => f.write_str("paused"),
            Self::Retired => f.write_str("retired"),
        }
    }
}
impl ::std::str::FromStr for AgentStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "retired" => Ok(Self::Retired),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AgentStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AgentStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`Budgets`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    ///What the team may spend in a day before it pauses (spec 5.5). A sprint's own budget belongs to the sprint, not here.
    pub daily_usd: f64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub session: ::std::option::Option<SessionLimits>,
}
///A team and everything the human configures about it, stored as .farik/team.yaml and read by the governor on every decision. See docs/SPEC.md sections 3, 5.12, 5.14 and 5.16, and decision D18 in docs/plans/project-plan.md.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FarikTeam {
    ///Two to seven agents. Three more rules are validate_team's rather than this schema's, because typify cannot generate a usable type from an array that carries a `contains`: ids are unique, and the team has an active Product Manager and an active Software Developer.
    pub agents: ::std::vec::Vec<Agent>,
    pub budgets: Budgets,
    ///What the human calls this team.
    pub name: FarikTeamName,
    pub policy: Policy,
    pub rules: Rules,
}
///What the human calls this team.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct FarikTeamName(::std::string::String);
impl ::std::ops::Deref for FarikTeamName {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<FarikTeamName> for ::std::string::String {
    fn from(value: FarikTeamName) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for FarikTeamName {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 100usize {
            return Err("longer than 100 characters".into());
        }
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for FarikTeamName {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FarikTeamName {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for FarikTeamName {
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
///`Model`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Model {
    ///How hard the model thinks. The runtime's own Effort arrives in phase 3.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub effort: ::std::option::Option<ModelEffort>,
    ///The provider's model id, exactly as the runtime reports it in usage, so that the price table can be asked about it.
    pub id: ModelId,
}
///How hard the model thinks. The runtime's own Effort arrives in phase 3.
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
pub enum ModelEffort {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}
impl ::std::fmt::Display for ModelEffort {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Low => f.write_str("low"),
            Self::Medium => f.write_str("medium"),
            Self::High => f.write_str("high"),
        }
    }
}
impl ::std::str::FromStr for ModelEffort {
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
impl ::std::convert::TryFrom<&str> for ModelEffort {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ModelEffort {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///The provider's model id, exactly as the runtime reports it in usage, so that the price table can be asked about it.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct ModelId(::std::string::String);
impl ::std::ops::Deref for ModelId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ModelId> for ::std::string::String {
    fn from(value: ModelId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for ModelId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 100usize {
            return Err("longer than 100 characters".into());
        }
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for ModelId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ModelId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for ModelId {
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
///`PermissionTier`
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
pub enum PermissionTier {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "write_workspace")]
    WriteWorkspace,
    #[serde(rename = "execute")]
    Execute,
    #[serde(rename = "network")]
    Network,
    #[serde(rename = "git_local")]
    GitLocal,
    #[serde(rename = "git_remote")]
    GitRemote,
    #[serde(rename = "external_effect")]
    ExternalEffect,
}
impl ::std::fmt::Display for PermissionTier {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Read => f.write_str("read"),
            Self::WriteWorkspace => f.write_str("write_workspace"),
            Self::Execute => f.write_str("execute"),
            Self::Network => f.write_str("network"),
            Self::GitLocal => f.write_str("git_local"),
            Self::GitRemote => f.write_str("git_remote"),
            Self::ExternalEffect => f.write_str("external_effect"),
        }
    }
}
impl ::std::str::FromStr for PermissionTier {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "read" => Ok(Self::Read),
            "write_workspace" => Ok(Self::WriteWorkspace),
            "execute" => Ok(Self::Execute),
            "network" => Ok(Self::Network),
            "git_local" => Ok(Self::GitLocal),
            "git_remote" => Ok(Self::GitRemote),
            "external_effect" => Ok(Self::ExternalEffect),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for PermissionTier {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PermissionTier {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`Policy`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    ///How long a task may stay blocked before it escalates (spec 5.7).
    pub blocked_limit_hours: ::std::num::NonZeroU64,
    ///Which contracts wait for the human's acceptance (spec 5.16). `high_risk` is the default the first run writes.
    pub human_accepts_contracts: PolicyHumanAcceptsContracts,
    ///What happens to a task's branch after it is accepted (spec 5.14).
    pub integration: PolicyIntegration,
    ///The branch accepted work integrates into. Left out, it is the repository's default branch (spec 5.14).
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub integration_branch: ::std::option::Option<PolicyIntegrationBranch>,
    ///How many times a task may be rejected before it escalates (spec 5.7).
    pub max_iterations: ::std::num::NonZeroU64,
    ///How many tasks one agent may hold that are neither accepted nor cancelled (spec 5.2). Zero refuses every assignment, which is how a team pauses an agent without retiring it.
    pub wip_limit_per_agent: i64,
}
///Which contracts wait for the human's acceptance (spec 5.16). `high_risk` is the default the first run writes.
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
pub enum PolicyHumanAcceptsContracts {
    #[serde(rename = "high_risk")]
    HighRisk,
    #[serde(rename = "all")]
    All,
}
impl ::std::fmt::Display for PolicyHumanAcceptsContracts {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::HighRisk => f.write_str("high_risk"),
            Self::All => f.write_str("all"),
        }
    }
}
impl ::std::str::FromStr for PolicyHumanAcceptsContracts {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "high_risk" => Ok(Self::HighRisk),
            "all" => Ok(Self::All),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for PolicyHumanAcceptsContracts {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PolicyHumanAcceptsContracts {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///What happens to a task's branch after it is accepted (spec 5.14).
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
pub enum PolicyIntegration {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "local_merge")]
    LocalMerge,
    #[serde(rename = "pull_request")]
    PullRequest,
}
impl ::std::fmt::Display for PolicyIntegration {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Manual => f.write_str("manual"),
            Self::LocalMerge => f.write_str("local_merge"),
            Self::PullRequest => f.write_str("pull_request"),
        }
    }
}
impl ::std::str::FromStr for PolicyIntegration {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "manual" => Ok(Self::Manual),
            "local_merge" => Ok(Self::LocalMerge),
            "pull_request" => Ok(Self::PullRequest),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for PolicyIntegration {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PolicyIntegration {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///The branch accepted work integrates into. Left out, it is the repository's default branch (spec 5.14).
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct PolicyIntegrationBranch(::std::string::String);
impl ::std::ops::Deref for PolicyIntegrationBranch {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<PolicyIntegrationBranch> for ::std::string::String {
    fn from(value: PolicyIntegrationBranch) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for PolicyIntegrationBranch {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() > 200usize {
            return Err("longer than 200 characters".into());
        }
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for PolicyIntegrationBranch {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PolicyIntegrationBranch {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for PolicyIntegrationBranch {
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
///The role an agent is instantiated from. `human` is a role a contract may name as a reviewer, never a role an agent holds.
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
}
impl ::std::fmt::Display for Role {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::ProductManager => f.write_str("product_manager"),
            Self::ScrumMaster => f.write_str("scrum_master"),
            Self::Architect => f.write_str("architect"),
            Self::SoftwareDeveloper => f.write_str("software_developer"),
            Self::MarketingSpecialist => f.write_str("marketing_specialist"),
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
///`Rules`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    ///Globs a contract's allowed_paths must fall within. Empty means no ceiling.
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub allowed_paths_ceiling: ::std::vec::Vec<RulesAllowedPathsCeilingItem>,
    ///ECMAScript regular expressions a command must not match (spec 5.12).
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub forbidden_commands: ::std::vec::Vec<RulesForbiddenCommandsItem>,
    ///The most one task's budget may be. Left out, the five dollars farik-core ships; to lift the cap in practice, write a number large enough. There is no way to say "no cap", because a team that can turn a rule off is a rule that only narrows in name (spec 5.12).
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub max_task_budget_usd: ::std::option::Option<f64>,
    ///Globs no tool may read or write, whatever its tier (spec 5.6). Left out, the five farik-core ships.
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub protected_paths: ::std::vec::Vec<RulesProtectedPathsItem>,
    ///Whether every `test` criterion must set new_tests_required.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub require_new_tests: ::std::option::Option<bool>,
    ///Verification methods every contract must have at least one criterion of.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub required_criteria: ::std::option::Option<::std::vec::Vec<RulesRequiredCriteriaItem>>,
}
///`RulesAllowedPathsCeilingItem`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct RulesAllowedPathsCeilingItem(::std::string::String);
impl ::std::ops::Deref for RulesAllowedPathsCeilingItem {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<RulesAllowedPathsCeilingItem> for ::std::string::String {
    fn from(value: RulesAllowedPathsCeilingItem) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for RulesAllowedPathsCeilingItem {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for RulesAllowedPathsCeilingItem {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for RulesAllowedPathsCeilingItem {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for RulesAllowedPathsCeilingItem {
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
///`RulesForbiddenCommandsItem`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct RulesForbiddenCommandsItem(::std::string::String);
impl ::std::ops::Deref for RulesForbiddenCommandsItem {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<RulesForbiddenCommandsItem> for ::std::string::String {
    fn from(value: RulesForbiddenCommandsItem) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for RulesForbiddenCommandsItem {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for RulesForbiddenCommandsItem {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for RulesForbiddenCommandsItem {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for RulesForbiddenCommandsItem {
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
///`RulesProtectedPathsItem`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct RulesProtectedPathsItem(::std::string::String);
impl ::std::ops::Deref for RulesProtectedPathsItem {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<RulesProtectedPathsItem> for ::std::string::String {
    fn from(value: RulesProtectedPathsItem) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for RulesProtectedPathsItem {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for RulesProtectedPathsItem {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for RulesProtectedPathsItem {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for RulesProtectedPathsItem {
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
///`RulesRequiredCriteriaItem`
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
pub enum RulesRequiredCriteriaItem {
    #[serde(rename = "command")]
    Command,
    #[serde(rename = "test")]
    Test,
    #[serde(rename = "artifact")]
    Artifact,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "human")]
    Human,
}
impl ::std::fmt::Display for RulesRequiredCriteriaItem {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Command => f.write_str("command"),
            Self::Test => f.write_str("test"),
            Self::Artifact => f.write_str("artifact"),
            Self::Review => f.write_str("review"),
            Self::Human => f.write_str("human"),
        }
    }
}
impl ::std::str::FromStr for RulesRequiredCriteriaItem {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "command" => Ok(Self::Command),
            "test" => Ok(Self::Test),
            "artifact" => Ok(Self::Artifact),
            "review" => Ok(Self::Review),
            "human" => Ok(Self::Human),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for RulesRequiredCriteriaItem {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for RulesRequiredCriteriaItem {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///Overrides for one session's limits. What is left out keeps the role's default (farik-core's default_session_limits).
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SessionLimits {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub max_input_tokens: ::std::option::Option<::std::num::NonZeroU64>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub max_output_tokens: ::std::option::Option<::std::num::NonZeroU64>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub max_tool_calls: ::std::option::Option<::std::num::NonZeroU64>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub max_wall_clock_seconds: ::std::option::Option<::std::num::NonZeroU64>,
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
