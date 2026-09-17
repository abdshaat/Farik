// Generated from docs/schemas/criteria.schema.json by `cargo xtask generate`. Do not edit.
#![allow(clippy::all, clippy::pedantic, missing_docs)]

///`CriterionTemplate`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CriterionTemplate {
    ///What a contract refers to this criterion by. Kebab-case, unique in the library.
    pub name: CriterionTemplateName,
    ///Where it came from. Left out, the human's: a refresh of the project scan replaces what the scan found and never touches what a person wrote.
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub source: ::std::option::Option<CriterionTemplateSource>,
    ///What the criterion says, in the words it will carry into every contract that uses it.
    pub text: CriterionTemplateText,
    pub verification: CriterionTemplateVerification,
}
///What a contract refers to this criterion by. Kebab-case, unique in the library.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct CriterionTemplateName(::std::string::String);
impl ::std::ops::Deref for CriterionTemplateName {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<CriterionTemplateName> for ::std::string::String {
    fn from(value: CriterionTemplateName) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for CriterionTemplateName {
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
impl ::std::convert::TryFrom<&str> for CriterionTemplateName {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for CriterionTemplateName {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for CriterionTemplateName {
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
///Where it came from. Left out, the human's: a refresh of the project scan replaces what the scan found and never touches what a person wrote.
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
pub enum CriterionTemplateSource {
    #[serde(rename = "project_scan")]
    ProjectScan,
    #[serde(rename = "human")]
    Human,
}
impl ::std::fmt::Display for CriterionTemplateSource {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::ProjectScan => f.write_str("project_scan"),
            Self::Human => f.write_str("human"),
        }
    }
}
impl ::std::str::FromStr for CriterionTemplateSource {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "project_scan" => Ok(Self::ProjectScan),
            "human" => Ok(Self::Human),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for CriterionTemplateSource {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for CriterionTemplateSource {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///What the criterion says, in the words it will carry into every contract that uses it.
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct CriterionTemplateText(::std::string::String);
impl ::std::ops::Deref for CriterionTemplateText {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<CriterionTemplateText> for ::std::string::String {
    fn from(value: CriterionTemplateText) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for CriterionTemplateText {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 10usize {
            return Err("shorter than 10 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<&str> for CriterionTemplateText {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for CriterionTemplateText {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl<'de> ::serde::Deserialize<'de> for CriterionTemplateText {
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
///`CriterionTemplateVerification`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum CriterionTemplateVerification {
    Variant0 {
        ///Run inside the sandbox from the project root.
        command: ::std::string::String,
        expect: CriterionTemplateVerificationVariant0Expect,
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
///`CriterionTemplateVerificationVariant0Expect`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CriterionTemplateVerificationVariant0Expect {
    #[serde(default)]
    pub exit_code: i64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub stdout_contains: ::std::option::Option<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub stdout_not_contains: ::std::option::Option<::std::string::String>,
}
///The named, reusable exit criteria a project verifies its work with, stored as .farik/team/criteria.yaml: the project's own check and test commands found by the project scan, and any the human adds. Referenced by name when a contract is written and expanded into it, so that contracts across a project verify the same way. See docs/SPEC.md section 5.13.
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FarikCriteriaLibrary {
    ///Names are unique, which JSON Schema cannot say and validate_criteria therefore does. A library may be empty: a project whose scan found nothing still has a file.
    pub criteria: ::std::vec::Vec<CriterionTemplate>,
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
