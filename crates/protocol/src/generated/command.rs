// Generated from docs/schemas/command.schema.json by `cargo xtask generate`. Do not edit.
#![allow(clippy::all, clippy::pedantic, missing_docs)]

///`CommandBodyWire`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum CommandBodyWire {
    TaskCreateBody(TaskCreateBody),
    RequestTriageBody(RequestTriageBody),
}
impl ::std::convert::From<TaskCreateBody> for CommandBodyWire {
    fn from(value: TaskCreateBody) -> Self {
        Self::TaskCreateBody(value)
    }
}
impl ::std::convert::From<RequestTriageBody> for CommandBodyWire {
    fn from(value: RequestTriageBody) -> Self {
        Self::RequestTriageBody(value)
    }
}
///`CommandName`
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
pub enum CommandName {
    #[serde(rename = "task_create")]
    TaskCreate,
    #[serde(rename = "request_triage")]
    RequestTriage,
}
impl ::std::fmt::Display for CommandName {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::TaskCreate => f.write_str("task_create"),
            Self::RequestTriage => f.write_str("request_triage"),
        }
    }
}
impl ::std::str::FromStr for CommandName {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "task_create" => Ok(Self::TaskCreate),
            "request_triage" => Ok(Self::RequestTriage),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for CommandName {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for CommandName {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///A request for the daemon to change something. Shaped like an event: command names what is asked and body carries that command's arguments. The pairing of the two is checked by the reader, farik_protocol::command::command_from_value.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FarikCommand {
    pub body: CommandBodyWire,
    pub command: CommandName,
}
///`RequestTriageBody`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RequestTriageBody {
    pub reason: ::std::string::String,
    pub size: RequestTriageBodySize,
    pub task_id: RequestTriageBodyTaskId,
}
///`RequestTriageBodySize`
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
pub enum RequestTriageBodySize {
    #[serde(rename = "large")]
    Large,
    #[serde(rename = "small")]
    Small,
}
impl ::std::fmt::Display for RequestTriageBodySize {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Large => f.write_str("large"),
            Self::Small => f.write_str("small"),
        }
    }
}
impl ::std::str::FromStr for RequestTriageBodySize {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "large" => Ok(Self::Large),
            "small" => Ok(Self::Small),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for RequestTriageBodySize {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for RequestTriageBodySize {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`RequestTriageBodyTaskId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct RequestTriageBodyTaskId(::std::string::String);
impl ::std::ops::Deref for RequestTriageBodyTaskId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<RequestTriageBodyTaskId> for ::std::string::String {
    fn from(value: RequestTriageBodyTaskId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for RequestTriageBodyTaskId {
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
impl ::std::convert::TryFrom<&str> for RequestTriageBodyTaskId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for RequestTriageBodyTaskId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for RequestTriageBodyTaskId {
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
///`TaskCreateBody`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskCreateBody {
    ///A task contract. This schema says only that it is an object: one schema never references another, and the contract's rules, the repeated-id ones among them, belong to farik_core::contract::validate_contract, which the reader calls.
    pub contract: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
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
