//! Farik's own MCP server over the United States' product and vehicle safety agencies
//! (`docs/SPEC.md` 6.7, ADR 0038), started by `farik connector recalls`: the CPSC's recalls of
//! products, NHTSA's recalls, complaints and crash ratings of vehicles, and vPIC's decoding of a
//! VIN. It speaks to three fixed addresses, follows no redirect and uses no proxy, checks every
//! input before it leaves, and asks for nothing an answer names. What the agencies answer is data
//! the agent reads under the untrusted-content notice.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike as _, NaiveDate};
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Map, Value, json};

/// The CPSC's address. Never an argument, an environment value or a tool input: the tests pass a
/// fixture's address to [`Recalls::new`], not to the command.
pub const CPSC_API: &str = "https://www.saferproducts.gov/RestWebServices";
/// NHTSA's address, for recalls, complaints and crash ratings.
pub const NHTSA_API: &str = "https://api.nhtsa.gov";
/// vPIC's address, NHTSA's VIN decoder.
pub const VPIC_API: &str = "https://vpic.nhtsa.dot.gov/api";

/// Why the server could not run.
#[derive(Debug)]
pub enum RecallsError {
    /// The web client could not be made.
    Client,
    /// The server could not start, or ended with an error.
    Serving(String),
}

impl fmt::Display for RecallsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client => write!(formatter, "the web client could not be made"),
            Self::Serving(why) => write!(formatter, "the safety-recalls server stopped: {why}"),
        }
    }
}

impl std::error::Error for RecallsError {}

/// The tools the server lists, in the order it lists them.
#[must_use]
pub fn tool_names() -> Vec<&'static str> {
    vec![
        "product_recalls",
        "vehicle_recalls",
        "vehicle_complaints",
        "vehicle_safety_ratings",
        "decode_vin",
    ]
}

/// How long one call to an agency may take.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The most of an answer Farik reads.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// The most of a vehicle's complaints Farik reads: a popular car's are several megabytes.
const MAX_COMPLAINTS_BODY: usize = 8 * 1024 * 1024;
/// The most product recalls an answer holds.
const MAX_RECALLS: usize = 50;
/// The most of a recall's description an answer holds, in characters.
const DESCRIPTION_CHARACTERS: usize = 2000;
/// The most complaints an answer lists in full.
const MAX_COMPLAINTS: usize = 20;
/// The most of a complaint's summary an answer holds, in characters.
const SUMMARY_CHARACTERS: usize = 600;
/// The most vehicles whose crash ratings are read.
const MAX_VEHICLES: usize = 10;
/// The most characters a search's words hold.
const MAX_WORDS: usize = 100;
/// The most characters a make or a model holds.
const MAX_NAME: usize = 40;
/// The earliest model year Farik asks about.
const FIRST_YEAR: i64 = 1950;
/// The fields of a decoded VIN an answer keeps, as vPIC names them.
const VIN_FIELDS: [&str; 12] = [
    "Make",
    "Model",
    "ModelYear",
    "Trim",
    "BodyClass",
    "EngineCylinders",
    "DisplacementL",
    "FuelTypePrimary",
    "DriveType",
    "PlantCountry",
    "ErrorCode",
    "ErrorText",
];

/// The three hosts the server asks.
#[derive(Clone, Copy)]
enum Host {
    Cpsc,
    Nhtsa,
    Vpic,
}

/// Who answers, for the words of a refusal: vPIC is NHTSA's.
#[derive(Clone, Copy)]
enum Agency {
    Cpsc,
    Nhtsa,
}

impl Host {
    fn agency(self) -> Agency {
        match self {
            Self::Cpsc => Agency::Cpsc,
            Self::Nhtsa | Self::Vpic => Agency::Nhtsa,
        }
    }
}

impl Agency {
    fn name(self) -> &'static str {
        match self {
            Self::Cpsc => "The CPSC",
            Self::Nhtsa => "NHTSA",
        }
    }

    /// What a call says whenever the agency does not answer with rows: never its own words.
    fn refused(self) -> String {
        format!("{} could not answer that", self.name())
    }

    fn unexpected(self) -> String {
        format!("{}'s answer was not what was expected", self.name())
    }

    fn too_large(self) -> String {
        match self {
            Self::Cpsc => "The CPSC's answer is too large; ask a narrower question".to_string(),
            Self::Nhtsa => "NHTSA's answer is too large to read here".to_string(),
        }
    }

    fn trouble(self, timed_out: bool, seconds: u64) -> String {
        if timed_out {
            format!("{} did not answer within {seconds} seconds", self.name())
        } else {
            format!("{} could not be reached", self.name())
        }
    }
}

/// The server, speaking to three addresses.
#[derive(Clone)]
pub struct Recalls {
    client: reqwest::Client,
    timeout: Duration,
    today: Option<NaiveDate>,
    cpsc: reqwest::Url,
    nhtsa: reqwest::Url,
    vpic: reqwest::Url,
}

impl Recalls {
    /// A server that asks `cpsc`, `nhtsa` and `vpic`, and reads the year from the clock.
    ///
    /// # Errors
    ///
    /// An address is not a web address, or the web client could not be made.
    pub fn new(cpsc: &str, nhtsa: &str, vpic: &str) -> Result<Self, RecallsError> {
        Self::build(cpsc, nhtsa, vpic, None, TIMEOUT)
    }

    /// A server that believes it is `today`, for a test.
    #[cfg(test)]
    fn with_today(
        cpsc: &str,
        nhtsa: &str,
        vpic: &str,
        today: NaiveDate,
    ) -> Result<Self, RecallsError> {
        Self::build(cpsc, nhtsa, vpic, Some(today), TIMEOUT)
    }

    /// [`Recalls::with_today`] with a timeout of its own.
    #[cfg(test)]
    fn with_timeout(
        cpsc: &str,
        nhtsa: &str,
        vpic: &str,
        today: NaiveDate,
        timeout: Duration,
    ) -> Result<Self, RecallsError> {
        Self::build(cpsc, nhtsa, vpic, Some(today), timeout)
    }

    fn build(
        cpsc: &str,
        nhtsa: &str,
        vpic: &str,
        today: Option<NaiveDate>,
        timeout: Duration,
    ) -> Result<Self, RecallsError> {
        let client = reqwest::Client::builder()
            // Nothing an agency answers sends Farik anywhere else, and nothing is sent through a
            // proxy.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(timeout)
            .build()
            .map_err(|_| RecallsError::Client)?;
        let address = |text: &str| reqwest::Url::parse(text).map_err(|_| RecallsError::Client);
        Ok(Self {
            client,
            timeout,
            today,
            cpsc: address(cpsc)?,
            nhtsa: address(nhtsa)?,
            vpic: address(vpic)?,
        })
    }

    /// Runs `tool` with `input`: the answer as JSON, or why it was refused, in words.
    ///
    /// # Errors
    ///
    /// The input is not valid, or an agency could not be reached or answered badly.
    pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String> {
        match tool {
            "product_recalls" => self.product_recalls(input).await,
            "vehicle_recalls" => self.vehicle_recalls(input).await,
            "vehicle_complaints" => self.vehicle_complaints(input).await,
            "vehicle_safety_ratings" => self.vehicle_safety_ratings(input).await,
            "decode_vin" => self.decode_vin(input).await,
            _ => Err(format!("there is no tool named {tool}")),
        }
    }

    async fn product_recalls(&self, input: &Value) -> Result<Value, String> {
        let words = text_of(input, "words", MAX_WORDS)?;
        let parameter = match input["field"].as_str() {
            Some("title") => "RecallTitle",
            Some("product_name") => "ProductName",
            Some("product_type") => "ProductType",
            _ => return Err("field is title, product_name or product_type".to_string()),
        };
        let since = since_of(input)?;
        let mut query = vec![("format", "json"), (parameter, words.as_str())];
        if let Some(since) = &since {
            query.push(("RecallDateStart", since.as_str()));
        }
        let answer = self.ask(Host::Cpsc, &["Recall"], &query, MAX_BODY).await?;
        let mut recalls: Vec<&Value> = answer
            .as_array()
            .ok_or_else(|| Agency::Cpsc.unexpected())?
            .iter()
            .collect();
        // The newest first; a recall with no date goes last.
        recalls.sort_by(|a, b| b["RecallDate"].as_str().cmp(&a["RecallDate"].as_str()));
        let more = recalls.len() > MAX_RECALLS;
        let rows: Vec<Value> = recalls
            .into_iter()
            .take(MAX_RECALLS)
            .map(recall_row)
            .collect();
        Ok(json!({ "recalls": rows, "more": more }))
    }

    async fn vehicle_recalls(&self, input: &Value) -> Result<Value, String> {
        let vehicle = self.vehicle_of(input)?;
        let answer = self
            .ask(
                Host::Nhtsa,
                &["recalls", "recallsByVehicle"],
                &vehicle.query(),
                MAX_BODY,
            )
            .await?;
        let recalls: Vec<Value> = rows_of(&answer, "results")?
            .iter()
            .map(|recall| {
                json!({
                    "campaign": scalar(&recall["NHTSACampaignNumber"]),
                    "date": scalar(&recall["ReportReceivedDate"]),
                    "component": scalar(&recall["Component"]),
                    "summary": scalar(&recall["Summary"]),
                    "consequence": scalar(&recall["Consequence"]),
                    "remedy": scalar(&recall["Remedy"]),
                    "park_it": flag(&recall["parkIt"]),
                    "park_outside": flag(&recall["parkOutSide"]),
                })
            })
            .collect();
        Ok(json!({ "recalls": recalls }))
    }

    async fn vehicle_complaints(&self, input: &Value) -> Result<Value, String> {
        let vehicle = self.vehicle_of(input)?;
        let answer = self
            .ask(
                Host::Nhtsa,
                &["complaints", "complaintsByVehicle"],
                &vehicle.query(),
                MAX_COMPLAINTS_BODY,
            )
            .await?;
        let complaints = rows_of(&answer, "results")?;
        // A component counts once for a complaint, however often it is listed.
        let mut by_component: BTreeMap<String, u64> = BTreeMap::new();
        for complaint in complaints {
            for component in components_of(&complaint["components"]) {
                *by_component.entry(component).or_default() += 1;
            }
        }
        let mut newest: Vec<&Value> = complaints.iter().collect();
        // By the day it was filed, read as MM/DD/YYYY; one that cannot be read goes last.
        newest.sort_by_key(|complaint| std::cmp::Reverse(filed_on(complaint)));
        let newest: Vec<Value> = newest
            .into_iter()
            .take(MAX_COMPLAINTS)
            .map(complaint_row)
            .collect();
        Ok(json!({
            "count": complaints.len(),
            "by_component": by_component,
            "newest": newest,
        }))
    }

    async fn vehicle_safety_ratings(&self, input: &Value) -> Result<Value, String> {
        let vehicle = self.vehicle_of(input)?;
        let listed = self
            .ask(
                Host::Nhtsa,
                &[
                    "SafetyRatings",
                    "modelyear",
                    &vehicle.year,
                    "make",
                    &vehicle.make,
                    "model",
                    &vehicle.model,
                ],
                &[],
                MAX_BODY,
            )
            .await?;
        let ids: Vec<u64> = rows_of(&listed, "Results")?
            .iter()
            .filter_map(|row| vehicle_id(&row["VehicleId"]))
            .collect();
        let mut ratings: Vec<Value> = Vec::new();
        for id in ids.iter().take(MAX_VEHICLES) {
            let id = id.to_string();
            let one = self
                .ask(
                    Host::Nhtsa,
                    &["SafetyRatings", "VehicleId", &id],
                    &[],
                    MAX_BODY,
                )
                .await?;
            if let Some(rating) = rows_of(&one, "Results")?.first() {
                ratings.push(json!({
                    "vehicle": scalar(&rating["VehicleDescription"]),
                    "overall": scalar(&rating["OverallRating"]),
                    "frontal": scalar(&rating["OverallFrontCrashRating"]),
                    "side": scalar(&rating["OverallSideCrashRating"]),
                    "rollover": scalar(&rating["RolloverRating"]),
                }));
            }
        }
        Ok(json!({ "ratings": ratings, "more": ids.len() > MAX_VEHICLES }))
    }

    async fn decode_vin(&self, input: &Value) -> Result<Value, String> {
        let vin = input["vin"]
            .as_str()
            .filter(|vin| {
                vin.len() == 17
                    && vin.bytes().all(|byte| {
                        byte.is_ascii_digit() || b"ABCDEFGHJKLMNPRSTUVWXYZ".contains(&byte)
                    })
            })
            .ok_or_else(|| {
                "vin is 17 capital letters and digits, none of them I, O or Q".to_string()
            })?;
        let answer = self
            .ask(
                Host::Vpic,
                &["vehicles", "decodevinvalues", vin],
                &[("format", "json")],
                MAX_BODY,
            )
            .await?;
        let decoded = rows_of(&answer, "Results")?
            .first()
            .ok_or_else(|| Agency::Nhtsa.unexpected())?;
        let kept: Map<String, Value> = VIN_FIELDS
            .iter()
            .map(|name| ((*name).to_string(), scalar(&decoded[*name])))
            .collect();
        Ok(Value::Object(kept))
    }

    /// A vehicle's make, model and year, checked.
    fn vehicle_of(&self, input: &Value) -> Result<Vehicle, String> {
        let make = name_of(input, "make")?;
        let model = name_of(input, "model")?;
        let today = self
            .today
            .unwrap_or_else(|| chrono::Utc::now().date_naive());
        let latest = i64::from(today.year()) + 1;
        let year = input["model_year"]
            .as_i64()
            .filter(|year| (FIRST_YEAR..=latest).contains(year))
            .ok_or_else(|| format!("model_year is a whole number from {FIRST_YEAR} to {latest}"))?;
        Ok(Vehicle {
            make,
            model,
            year: year.to_string(),
        })
    }

    /// One call: GET `segments` under `host`'s fixed address with `query`, built as path segments
    /// and pairs and never formatted into the address.
    async fn ask(
        &self,
        host: Host,
        segments: &[&str],
        query: &[(&str, &str)],
        limit: usize,
    ) -> Result<Value, String> {
        let agency = host.agency();
        let mut url = match host {
            Host::Cpsc => self.cpsc.clone(),
            Host::Nhtsa => self.nhtsa.clone(),
            Host::Vpic => self.vpic.clone(),
        };
        url.path_segments_mut()
            .map_err(|()| agency.unexpected())?
            .pop_if_empty()
            .extend(segments);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let seconds = self.timeout.as_secs();
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| agency.trouble(error.is_timeout(), seconds))?;
        // Whatever it says other than rows, a redirect included, is one sentence of Farik's.
        if !response.status().is_success() {
            return Err(agency.refused());
        }
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| agency.trouble(error.is_timeout(), seconds))?
        {
            if chunk.len() > limit - bytes.len() {
                return Err(agency.too_large());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| agency.unexpected())
    }
}

/// A vehicle the tools ask about, its year as the text of a whole number.
struct Vehicle {
    make: String,
    model: String,
    year: String,
}

impl Vehicle {
    /// The query NHTSA's recalls and complaints take.
    fn query(&self) -> [(&str, &str); 3] {
        [
            ("make", &self.make),
            ("model", &self.model),
            ("modelYear", &self.year),
        ]
    }
}

/// The text `input[key]` holds, of 1 to `most` characters.
fn text_of(input: &Value, key: &str, most: usize) -> Result<String, String> {
    let text = input[key]
        .as_str()
        .ok_or_else(|| format!("{key} is needed, as text"))?;
    let length = text.chars().count();
    if length == 0 || length > most {
        return Err(format!("{key} is 1 to {most} characters"));
    }
    Ok(text.to_string())
}

/// A make or a model: 1 to 40 letters, digits, spaces and hyphens, so it is one path segment.
fn name_of(input: &Value, key: &str) -> Result<String, String> {
    let name = text_of(input, key, MAX_NAME)?;
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-')
    {
        return Err(format!("{key} is letters, digits, spaces and hyphens"));
    }
    Ok(name)
}

/// The day `input["since"]` names, an ISO date, when it names one.
fn since_of(input: &Value) -> Result<Option<String>, String> {
    let wrong = || "since is a real day, written YYYY-MM-DD".to_string();
    let since = &input["since"];
    if since.is_null() {
        return Ok(None);
    }
    let text = since.as_str().ok_or_else(wrong)?;
    let shaped = text.len() == 10
        && text.char_indices().all(|(at, c)| {
            if at == 4 || at == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    shaped
        .then(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
        .flatten()
        .ok_or_else(wrong)?;
    Ok(Some(text.to_string()))
}

/// The rows of NHTSA's answer under `key`, or under the other spelling of it, NHTSA's services
/// not agreeing on one.
fn rows_of<'a>(answer: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    let other = if key == "results" {
        "Results"
    } else {
        "results"
    };
    answer[key]
        .as_array()
        .or_else(|| answer[other].as_array())
        .ok_or_else(|| Agency::Nhtsa.unexpected())
}

/// `value` when it is text or a number, as text; else null.
fn scalar(value: &Value) -> Value {
    match value {
        Value::String(_) => value.clone(),
        Value::Number(number) => Value::String(number.to_string()),
        _ => Value::Null,
    }
}

/// `value` when it is true or false; else null.
fn flag(value: &Value) -> Value {
    if value.is_boolean() {
        value.clone()
    } else {
        Value::Null
    }
}

/// `value` as a vehicle's id: a positive whole number, or the digits of one.
fn vehicle_id(value: &Value) -> Option<u64> {
    let id = match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) if text.bytes().all(|byte| byte.is_ascii_digit()) => text.parse().ok(),
        _ => None,
    };
    id.filter(|id| *id > 0)
}

/// `value` cut to `most` characters, and whether it was cut; null when it is not text.
fn cut(value: &Value, most: usize) -> (Value, bool) {
    let Some(text) = value.as_str() else {
        return (Value::Null, false);
    };
    if text.chars().count() > most {
        (Value::String(text.chars().take(most).collect()), true)
    } else {
        (Value::String(text.to_string()), false)
    }
}

/// Each `key` text of the objects `list` holds.
fn names_in(list: &Value, key: &str) -> Vec<String> {
    list.as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item[key].as_str().map(str::to_string))
        .collect()
}

/// One product recall, as the answer gives it.
fn recall_row(recall: &Value) -> Value {
    let products: Vec<Value> = recall["Products"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|product| {
            json!({
                "name": scalar(&product["Name"]),
                "model": scalar(&product["Model"]),
                "type": scalar(&product["Type"]),
                "units": scalar(&product["NumberOfUnits"]),
            })
        })
        .collect();
    let (description, was_cut) = cut(&recall["Description"], DESCRIPTION_CHARACTERS);
    let mut row = json!({
        "number": scalar(&recall["RecallNumber"]),
        "date": scalar(&recall["RecallDate"]),
        "title": scalar(&recall["Title"]),
        "url": scalar(&recall["URL"]),
        "description": description,
        "products": products,
        "hazards": names_in(&recall["Hazards"], "Name"),
        "remedies": names_in(&recall["Remedies"], "Name"),
        "manufacturers": names_in(&recall["Manufacturers"], "Name"),
        "retailers": names_in(&recall["Retailers"], "Name"),
        "countries": names_in(&recall["ManufacturerCountries"], "Country"),
    });
    if was_cut {
        row["description_cut"] = json!(true);
    }
    row
}

/// The components of a complaint, split at commas, each once.
fn components_of(value: &Value) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for component in value.as_str().unwrap_or_default().split(',') {
        let component = component.trim();
        if !component.is_empty() && !found.iter().any(|seen| seen == component) {
            found.push(component.to_string());
        }
    }
    found
}

/// The day a complaint was filed, when it can be read.
fn filed_on(complaint: &Value) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(complaint["dateComplaintFiled"].as_str()?, "%m/%d/%Y").ok()
}

/// One complaint, as the answer lists it.
fn complaint_row(complaint: &Value) -> Value {
    let (summary, was_cut) = cut(&complaint["summary"], SUMMARY_CHARACTERS);
    let mut row = json!({
        "date": scalar(&complaint["dateComplaintFiled"]),
        "components": components_of(&complaint["components"]),
        "summary": summary,
    });
    if was_cut {
        row["summary_cut"] = json!(true);
    }
    row
}

/// The name the server gives itself.
const SERVER_NAME: &str = "farik-recalls";

/// Every tool, with what it takes.
fn descriptors() -> Vec<Tool> {
    let text = |about: &str| json!({ "type": "string", "description": about });
    let vehicle = || {
        json!({
            "type": "object",
            "properties": {
                "make": {
                    "type": "string", "minLength": 1, "maxLength": 40,
                    "description": "The car's make, such as Honda: letters, digits, spaces and hyphens."
                },
                "model": {
                    "type": "string", "minLength": 1, "maxLength": 40,
                    "description": "The car's model, such as Accord: letters, digits, spaces and hyphens."
                },
                "model_year": {
                    "type": "integer", "minimum": 1950,
                    "description": "The model year, from 1950 to next year."
                }
            },
            "required": ["make", "model", "model_year"]
        })
    };
    let schemas = [
        (
            "product_recalls",
            "Search the United States Consumer Product Safety Commission's recalls of products. The search is a plain text match on the one field you choose, not a meaning search: try the product's type, then its name, then its maker's name. Answers the newest 50 recalls, with more set when there are others.",
            json!({
                "type": "object",
                "properties": {
                    "words": {
                        "type": "string", "minLength": 1, "maxLength": 100,
                        "description": "The words to match, such as mirror."
                    },
                    "field": {
                        "type": "string",
                        "enum": ["title", "product_name", "product_type"],
                        "description": "Which field the words must appear in: the recall's title, the product's name or the product's type."
                    },
                    "since": text("Only recalls from this day on, as YYYY-MM-DD.")
                },
                "required": ["words", "field"]
            }),
        ),
        (
            "vehicle_recalls",
            "List the recalls the National Highway Traffic Safety Administration holds for a car's make, model and model year, each with the component, the consequence and the remedy, and whether to stop driving it (park_it) or park it outside (park_outside).",
            vehicle(),
        ),
        (
            "vehicle_complaints",
            "Count the complaints owners filed with the National Highway Traffic Safety Administration about a car's make, model and model year, by component, and list the 20 newest. Complaints are owners' words, not findings.",
            vehicle(),
        ),
        (
            "vehicle_safety_ratings",
            "Read the National Highway Traffic Safety Administration's crash-test star ratings (overall, frontal, side and rollover) for a car's make, model and model year, for each of up to 10 versions of it, with more set when there are others.",
            vehicle(),
        ),
        (
            "decode_vin",
            "Decode a VIN with the National Highway Traffic Safety Administration's decoder: the make, model, year, trim, body, engine, fuel, drive and plant it names. ErrorCode 0 means the VIN decoded cleanly; anything else is explained in ErrorText.",
            json!({
                "type": "object",
                "properties": {
                    "vin": {
                        "type": "string",
                        "pattern": "^[A-HJ-NPR-Z0-9]{17}$",
                        "description": "The 17-character VIN, in capital letters and digits, none of them I, O or Q."
                    }
                },
                "required": ["vin"]
            }),
        ),
    ];
    schemas
        .into_iter()
        .map(|(name, about, schema)| {
            let Value::Object(schema): Value = schema else {
                unreachable!("each schema above is an object")
            };
            Tool::new(name, about, Arc::new(schema as JsonObject))
                .with_annotations(ToolAnnotations::new().read_only(true))
        })
        .collect()
}

impl ServerHandler for Recalls {
    fn get_info(&self) -> InitializeResult {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"));
        info
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        // As Farik's own server answers: protocol 2026-07-28 wants a list's freshness said.
        std::future::ready(Ok(ListToolsResult::with_all_items(descriptors())
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let input = request.arguments.map_or_else(|| json!({}), Value::Object);
        let result = match self.call(&request.name, &input).await {
            Ok(answer) => CallToolResult::success(vec![ContentBlock::text(answer.to_string())]),
            Err(why) => CallToolResult::error(vec![ContentBlock::text(why)]),
        };
        Ok(result.into())
    }
}

/// Serves on standard input and output until the client leaves.
///
/// # Errors
///
/// The client could not be made, or the server stopped with an error.
pub async fn serve_stdio(cpsc: &str, nhtsa: &str, vpic: &str) -> Result<(), RecallsError> {
    use rmcp::ServiceExt as _;

    let server = Recalls::new(cpsc, nhtsa, vpic)?;
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|error| RecallsError::Serving(error.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|error| RecallsError::Serving(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderMap, Method, Response, StatusCode, Uri};
    use chrono::NaiveDate;
    use serde_json::{Value, json};

    use super::{CPSC_API, NHTSA_API, Recalls, VPIC_API, descriptors, tool_names};

    /// What the fixture was asked.
    #[derive(Clone, Debug)]
    struct Seen {
        method: Method,
        uri: Uri,
        header_names: Vec<String>,
    }

    impl Seen {
        /// The query as pairs, decoded.
        fn pairs(&self) -> Vec<(String, String)> {
            url::form_urlencoded::parse(self.uri.query().unwrap_or_default().as_bytes())
                .into_owned()
                .collect()
        }
    }

    /// What the fixture answers.
    #[derive(Clone)]
    enum Reply {
        Json(Value),
        Bytes(Vec<u8>),
        Redirect(String),
        Status(u16),
        Hang,
    }

    type Answer = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

    /// A stand-in for one of the agencies' hosts on this computer, whose address ends in `base`.
    struct Fixture {
        address: String,
        seen: Arc<Mutex<Vec<Seen>>>,
    }

    impl Fixture {
        async fn start(
            base: &str,
            answer: impl Fn(&Seen) -> Reply + Send + Sync + 'static,
        ) -> Self {
            let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
            let answer: Answer = Arc::new(answer);
            let record = seen.clone();
            let app = Router::new().fallback(
                move |method: Method, uri: Uri, headers: HeaderMap, body: Body| {
                    let record = record.clone();
                    let answer = answer.clone();
                    async move {
                        let _ = to_bytes(body, usize::MAX).await;
                        let one = Seen {
                            method,
                            uri,
                            header_names: headers
                                .keys()
                                .map(|name| name.as_str().to_string())
                                .collect(),
                        };
                        record.lock().expect("the record").push(one.clone());
                        match answer(&one) {
                            Reply::Json(value) => Response::new(Body::from(value.to_string())),
                            Reply::Bytes(bytes) => Response::new(Body::from(bytes)),
                            Reply::Redirect(to) => Response::builder()
                                .status(StatusCode::MOVED_PERMANENTLY)
                                .header("location", to)
                                .body(Body::empty())
                                .expect("a response"),
                            Reply::Status(code) => Response::builder()
                                .status(code)
                                .body(Body::from("{\"message\":\"internal path /srv/agency/db\"}"))
                                .expect("a response"),
                            Reply::Hang => {
                                tokio::time::sleep(Duration::from_secs(3600)).await;
                                Response::new(Body::empty())
                            }
                        }
                    }
                },
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("a port");
            let address = format!(
                "http://{}{base}",
                listener.local_addr().expect("an address")
            );
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Self { address, seen }
        }

        /// A fixture for the CPSC's host, answering `answer` to everything.
        async fn cpsc(answer: Reply) -> Self {
            Self::start("/RestWebServices", move |_| answer.clone()).await
        }

        /// A fixture for NHTSA's host, answering `answer` to everything.
        async fn nhtsa(answer: Reply) -> Self {
            Self::start("", move |_| answer.clone()).await
        }

        /// A fixture for vPIC's host, answering `answer` to everything.
        async fn vpic(answer: Reply) -> Self {
            Self::start("/api", move |_| answer.clone()).await
        }

        fn requests(&self) -> Vec<Seen> {
            self.seen.lock().expect("the record").clone()
        }
    }

    /// The three hosts, each answering `answer` to everything.
    struct Hosts {
        cpsc: Fixture,
        nhtsa: Fixture,
        vpic: Fixture,
    }

    impl Hosts {
        async fn answering(answer: &Reply) -> Self {
            Self::split(answer, answer).await
        }

        /// The CPSC answering `cpsc`, and NHTSA and vPIC answering `nhtsa`.
        async fn split(cpsc: &Reply, nhtsa: &Reply) -> Self {
            Self {
                cpsc: Fixture::cpsc(cpsc.clone()).await,
                nhtsa: Fixture::nhtsa(nhtsa.clone()).await,
                vpic: Fixture::vpic(nhtsa.clone()).await,
            }
        }

        fn server(&self) -> Recalls {
            Recalls::with_today(
                &self.cpsc.address,
                &self.nhtsa.address,
                &self.vpic.address,
                today(),
            )
            .expect("a server")
        }

        fn requests(&self) -> usize {
            self.cpsc.requests().len() + self.nhtsa.requests().len() + self.vpic.requests().len()
        }
    }

    /// The day the server believes it is, so no test depends on the clock: next year is 2027.
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 8).expect("a day")
    }

    fn asking_cpsc(fixture: &Fixture) -> Recalls {
        Recalls::with_today(
            &fixture.address,
            "http://127.0.0.1:9",
            "http://127.0.0.1:9",
            today(),
        )
        .expect("a server")
    }

    fn asking_nhtsa(fixture: &Fixture) -> Recalls {
        Recalls::with_today(
            "http://127.0.0.1:9/RestWebServices",
            &fixture.address,
            &fixture.address,
            today(),
        )
        .expect("a server")
    }

    fn honda() -> Value {
        json!({ "make": "Honda", "model": "Accord", "model_year": 2003 })
    }

    fn keys(value: &Value) -> Vec<&str> {
        let mut found: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        found.sort_unstable();
        found
    }

    /// A recall as the CPSC writes it, with the fields Farik never reads among them.
    fn cpsc_recall(number: &str, date: &str, description: &str) -> Value {
        json!({
            "RecallID": 1234, "RecallNumber": number, "RecallDate": date,
            "Description": description, "URL": "https://www.cpsc.gov/Recalls/2024/baby-mirror",
            "Title": "Baby mirrors recalled for a strangulation hazard",
            "ConsumerContact": "Call 800-555-0100", "LastPublishDate": "2024-03-01T00:00:00",
            "Products": [{
                "Name": "Baby mirror", "Description": "", "Model": "BM-1", "Type": "Mirror",
                "CategoryID": "", "NumberOfUnits": "About 12,000"
            }],
            "Inconjunctions": [], "Images": [{ "URL": "https://www.cpsc.gov/img.jpg" }],
            "Injuries": [{ "Name": "None reported" }],
            "Manufacturers": [{ "Name": "Acme Baby", "CompanyID": "" }],
            "Retailers": [{ "Name": "Big Shop", "CompanyID": "" }, { "Name": "Web Shop", "CompanyID": "" }],
            "Importers": [], "Distributors": [], "SoldAtLabel": "Online",
            "ManufacturerCountries": [{ "Country": "China" }],
            "ProductUPCs": [],
            "Hazards": [{ "Name": "The strap can strangle a child.", "HazardTypeID": "" }],
            "Remedies": [{ "Name": "Replace" }], "RemedyOptions": [{ "Option": "Replace" }]
        })
    }

    /// Sixty recalls on sixty different days, in no order: `N000` is 2024-01-01, `N059` the newest.
    fn sixty_recalls() -> Vec<Value> {
        let first = NaiveDate::from_ymd_opt(2024, 1, 1).expect("a day");
        (0..60_i64)
            .map(|n| {
                let k = (n * 7) % 60;
                let date = (first + chrono::Duration::days(k))
                    .format("%Y-%m-%dT00:00:00")
                    .to_string();
                let description = match k {
                    59 => "d".repeat(2001),
                    58 => "d".repeat(2000),
                    _ => "A short description.".to_string(),
                };
                cpsc_recall(&format!("N{k:03}"), &date, &description)
            })
            .collect()
    }

    #[test]
    fn the_hosts_are_fixed() {
        assert_eq!(CPSC_API, "https://www.saferproducts.gov/RestWebServices");
        assert_eq!(NHTSA_API, "https://api.nhtsa.gov");
        assert_eq!(VPIC_API, "https://vpic.nhtsa.dot.gov/api");
    }

    /// Over MCP, as a client sees it: the name it gives, exactly five tools each with an input
    /// schema, an answer, and a refusal that is a tool error.
    #[tokio::test]
    async fn lists_exactly_five_tools() {
        use rmcp::ServiceExt as _;
        use rmcp::model::CallToolRequestParams;

        assert_eq!(
            tool_names(),
            [
                "product_recalls",
                "vehicle_recalls",
                "vehicle_complaints",
                "vehicle_safety_ratings",
                "decode_vin"
            ]
        );
        let listed: Vec<String> = descriptors()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(listed, tool_names());

        let hosts = Hosts::answering(&Reply::Json(json!([]))).await;
        let (server_io, client_io) = tokio::io::duplex(1 << 16);
        let server = hosts.server();
        tokio::spawn(async move {
            if let Ok(running) = server.serve(server_io).await {
                let _ = running.waiting().await;
            }
        });
        let client = ().serve(client_io).await.expect("a client");
        let info = client
            .peer_info()
            .expect("the server's answer to initialize");
        assert_eq!(
            info.server_info.as_ref().expect("a name").name,
            "farik-recalls"
        );
        let tools = client.list_all_tools().await.expect("a list");
        let names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
        assert_eq!(names, tool_names());
        for tool in &tools {
            assert_eq!(
                tool.input_schema.get("type"),
                Some(&json!("object")),
                "{}",
                tool.name
            );
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|note| note.read_only_hint),
                Some(true),
                "{} only reads",
                tool.name
            );
            let required = match tool.name.as_ref() {
                "product_recalls" => json!(["words", "field"]),
                "decode_vin" => json!(["vin"]),
                _ => json!(["make", "model", "model_year"]),
            };
            assert_eq!(tool.input_schema.get("required"), Some(&required));
        }
        let fields = &tools[0].input_schema["properties"]["field"]["enum"];
        assert_eq!(fields, &json!(["title", "product_name", "product_type"]));
        let arguments = |value: Value| value.as_object().cloned().expect("an object");
        let answer = client
            .call_tool(
                CallToolRequestParams::new("product_recalls")
                    .with_arguments(arguments(json!({ "words": "baby", "field": "title" }))),
            )
            .await
            .expect("a call");
        assert_ne!(answer.is_error, Some(true));
        let text = answer.content[0].as_text().expect("text");
        let said: Value = serde_json::from_str(&text.text).expect("JSON");
        assert_eq!(said["recalls"], json!([]));
        let refused = client
            .call_tool(
                CallToolRequestParams::new("product_recalls")
                    .with_arguments(arguments(json!({ "words": "", "field": "title" }))),
            )
            .await
            .expect("a call");
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(hosts.requests(), 1, "the refused call was not sent");
        let _ = client.cancel().await;
    }

    #[tokio::test]
    async fn product_recalls_searches_the_chosen_field() {
        let rows = sixty_recalls();
        let cpsc = Fixture::cpsc(Reply::Json(Value::Array(rows))).await;
        let server = asking_cpsc(&cpsc);
        let answer = server
            .call(
                "product_recalls",
                &json!({ "field": "title", "words": "baby", "since": "2024-01-01" }),
            )
            .await
            .expect("an answer");
        let seen = cpsc.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, Method::GET);
        assert_eq!(seen[0].uri.path(), "/RestWebServices/Recall");
        assert_eq!(
            seen[0].uri.query(),
            Some("format=json&RecallTitle=baby&RecallDateStart=2024-01-01")
        );
        for name in &seen[0].header_names {
            assert!(
                ["host", "accept"].contains(&name.as_str()),
                "an unexpected header {name}"
            );
        }
        // The newest 50 of 60 by date, newest first, and the answer says it was cut.
        assert_eq!(answer["more"], true);
        let recalls = answer["recalls"].as_array().expect("a list");
        assert_eq!(recalls.len(), 50);
        let numbers: Vec<&str> = recalls
            .iter()
            .map(|recall| recall["number"].as_str().expect("a number"))
            .collect();
        let wanted: Vec<String> = (10..60).rev().map(|k| format!("N{k:03}")).collect();
        assert_eq!(numbers, wanted);
        assert_eq!(keys(&answer), ["more", "recalls"]);
        // The newest has a description of 2,001 characters, cut to 2,000 and said so; the next has
        // exactly 2,000 and is whole.
        assert_eq!(
            keys(&recalls[0]),
            [
                "countries",
                "date",
                "description",
                "description_cut",
                "hazards",
                "manufacturers",
                "number",
                "products",
                "remedies",
                "retailers",
                "title",
                "url"
            ]
        );
        assert_eq!(recalls[0]["description"], "d".repeat(2000));
        assert_eq!(recalls[0]["description_cut"], true);
        assert_eq!(recalls[1]["description"], "d".repeat(2000));
        assert_eq!(
            keys(&recalls[1]),
            [
                "countries",
                "date",
                "description",
                "hazards",
                "manufacturers",
                "number",
                "products",
                "remedies",
                "retailers",
                "title",
                "url"
            ]
        );
        assert_eq!(recalls[0]["date"], "2024-02-29T00:00:00");
        assert_eq!(
            recalls[0]["title"],
            "Baby mirrors recalled for a strangulation hazard"
        );
        assert_eq!(
            recalls[0]["url"],
            "https://www.cpsc.gov/Recalls/2024/baby-mirror"
        );
        assert_eq!(
            recalls[0]["products"],
            json!([{ "name": "Baby mirror", "model": "BM-1", "type": "Mirror", "units": "About 12,000" }])
        );
        assert_eq!(
            recalls[0]["hazards"],
            json!(["The strap can strangle a child."])
        );
        assert_eq!(recalls[0]["remedies"], json!(["Replace"]));
        assert_eq!(recalls[0]["manufacturers"], json!(["Acme Baby"]));
        assert_eq!(recalls[0]["retailers"], json!(["Big Shop", "Web Shop"]));
        assert_eq!(recalls[0]["countries"], json!(["China"]));
    }

    /// `field` decides which of the CPSC's parameters carries the words, and never `Title`, which
    /// the CPSC ignores while answering everything it has.
    #[tokio::test]
    async fn product_recalls_names_the_cpsc_s_parameter_for_each_field() {
        let cpsc = Fixture::cpsc(Reply::Json(json!([]))).await;
        let server = asking_cpsc(&cpsc);
        for (field, parameter) in [
            ("title", "RecallTitle"),
            ("product_name", "ProductName"),
            ("product_type", "ProductType"),
        ] {
            let answer = server
                .call(
                    "product_recalls",
                    &json!({ "field": field, "words": "mirror" }),
                )
                .await
                .expect("an answer");
            assert_eq!(answer, json!({ "recalls": [], "more": false }), "{field}");
            let seen = cpsc.requests();
            assert_eq!(
                seen.last().expect("a request").pairs(),
                [
                    ("format".to_string(), "json".to_string()),
                    (parameter.to_string(), "mirror".to_string())
                ],
                "{field}"
            );
        }
        assert_eq!(cpsc.requests().len(), 3);
    }

    /// Words with an ampersand or an equals sign stay one value.
    #[tokio::test]
    async fn words_cannot_add_a_parameter() {
        let cpsc = Fixture::cpsc(Reply::Json(json!([]))).await;
        asking_cpsc(&cpsc)
            .call(
                "product_recalls",
                &json!({ "field": "title", "words": "baby&ProductName=x#y=z" }),
            )
            .await
            .expect("an answer");
        assert_eq!(
            cpsc.requests()[0].pairs(),
            [
                ("format".to_string(), "json".to_string()),
                (
                    "RecallTitle".to_string(),
                    "baby&ProductName=x#y=z".to_string()
                )
            ]
        );
    }

    #[tokio::test]
    async fn vehicle_recalls_reads_by_query() {
        let row = |campaign: &str| {
            json!({
                "Manufacturer": "Honda", "NHTSACampaignNumber": campaign, "parkIt": false,
                "parkOutSide": true, "overTheAirUpdate": false, "NHTSAActionNumber": "",
                "ReportReceivedDate": "26/04/2018", "Component": "AIR BAGS",
                "Summary": "The inflator can rupture.", "Consequence": "Metal fragments.",
                "Remedy": "Dealers replace the inflator.", "Notes": "Call Honda.",
                "ModelYear": "2003", "Make": "HONDA", "Model": "ACCORD"
            })
        };
        // The second recall says things in a way Farik does not read: neither is a flag.
        let mut odd = row("14V351");
        odd["parkIt"] = json!("yes");
        odd.as_object_mut()
            .expect("an object")
            .remove("parkOutSide");
        let nhtsa = Fixture::nhtsa(Reply::Json(json!({
            "Count": 2, "Message": "Results returned successfully",
            "results": [row("18V268"), odd]
        })))
        .await;
        let answer = asking_nhtsa(&nhtsa)
            .call("vehicle_recalls", &honda())
            .await
            .expect("an answer");
        let seen = nhtsa.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].uri.path(), "/recalls/recallsByVehicle");
        assert_eq!(
            seen[0].uri.query(),
            Some("make=Honda&model=Accord&modelYear=2003")
        );
        let recalls = answer["recalls"].as_array().expect("a list");
        assert_eq!(recalls.len(), 2);
        for recall in recalls {
            assert_eq!(
                keys(recall),
                [
                    "campaign",
                    "component",
                    "consequence",
                    "date",
                    "park_it",
                    "park_outside",
                    "remedy",
                    "summary"
                ]
            );
        }
        assert_eq!(
            recalls[0],
            json!({
                "campaign": "18V268", "date": "26/04/2018", "component": "AIR BAGS",
                "summary": "The inflator can rupture.", "consequence": "Metal fragments.",
                "remedy": "Dealers replace the inflator.", "park_it": false, "park_outside": true
            })
        );
        assert_eq!(keys(&answer), ["recalls"]);
        assert_eq!(recalls[1]["campaign"], "14V351");
        assert_eq!(recalls[1]["park_it"], Value::Null);
        assert_eq!(recalls[1]["park_outside"], Value::Null);
    }

    /// 25 complaints whose text order is not their date order: the 20 newest by date are kept, a
    /// component is counted once for a complaint however often it is listed.
    #[tokio::test]
    async fn vehicle_complaints_counts_and_keeps_the_newest() {
        let first = NaiveDate::from_ymd_opt(2019, 12, 20).expect("a day");
        let complaints: Vec<Value> = (0..25_i64)
            .map(|n| {
                let k = (n * 7) % 25;
                let filed = (first + chrono::Duration::days(k))
                    .format("%m/%d/%Y")
                    .to_string();
                let components = match k {
                    0..3 => "ENGINE,POWER TRAIN",
                    3..5 => " ENGINE , POWER TRAIN ,,",
                    5..10 => "ENGINE",
                    _ => "AIR BAGS,ENGINE,AIR BAGS",
                };
                let summary = match k {
                    24 => "s".repeat(601),
                    23 => "s".repeat(600),
                    _ => format!("Complaint {k}"),
                };
                json!({
                    "odiNumber": 1000 + k, "manufacturer": "Ford", "crash": false, "fire": false,
                    "numberOfInjuries": 0, "numberOfDeaths": 0, "dateOfIncident": "12/01/2019",
                    "dateComplaintFiled": filed, "vin": "1FADP3F2XCL", "components": components,
                    "summary": summary, "products": []
                })
            })
            .collect();
        let nhtsa = Fixture::nhtsa(Reply::Json(json!({
            "Count": 25, "Message": "Results returned successfully", "results": complaints
        })))
        .await;
        let answer = asking_nhtsa(&nhtsa)
            .call(
                "vehicle_complaints",
                &json!({ "make": "Ford", "model": "Focus", "model_year": 2012 }),
            )
            .await
            .expect("an answer");
        let seen = nhtsa.requests();
        assert_eq!(seen[0].uri.path(), "/complaints/complaintsByVehicle");
        assert_eq!(
            seen[0].uri.query(),
            Some("make=Ford&model=Focus&modelYear=2012")
        );
        assert_eq!(keys(&answer), ["by_component", "count", "newest"]);
        assert_eq!(answer["count"], 25);
        assert_eq!(
            answer["by_component"],
            json!({ "AIR BAGS": 15, "ENGINE": 25, "POWER TRAIN": 5 })
        );
        let newest = answer["newest"].as_array().expect("a list");
        assert_eq!(newest.len(), 20);
        let dates: Vec<&str> = newest
            .iter()
            .map(|complaint| complaint["date"].as_str().expect("a date"))
            .collect();
        let wanted: Vec<String> = (5..25_i64)
            .rev()
            .map(|k| {
                (first + chrono::Duration::days(k))
                    .format("%m/%d/%Y")
                    .to_string()
            })
            .collect();
        assert_eq!(dates, wanted, "the newest by date, not by text");
        assert_eq!(dates[0], "01/13/2020");
        assert_eq!(
            keys(&newest[0]),
            ["components", "date", "summary", "summary_cut"]
        );
        assert_eq!(newest[0]["components"], json!(["AIR BAGS", "ENGINE"]));
        assert_eq!(newest[0]["summary"], "s".repeat(600));
        assert_eq!(newest[0]["summary_cut"], true);
        assert_eq!(newest[1]["summary"], "s".repeat(600));
        assert_eq!(keys(&newest[1]), ["components", "date", "summary"]);
        assert_eq!(newest[2]["summary"], "Complaint 22");
    }

    fn ratings_list(ids: &[Value]) -> Value {
        let rows: Vec<Value> = ids
            .iter()
            .map(|id| json!({ "VehicleDescription": "2003 Toyota Land Cruiser 4 DR SUV 4WD", "VehicleId": id }))
            .collect();
        json!({ "Count": rows.len(), "Message": "Results returned successfully", "Results": rows })
    }

    fn one_rating(id: &str) -> Value {
        json!({
            "Count": 1, "Message": "Results returned successfully",
            "Results": [{
                "VehicleDescription": format!("2003 Toyota Land Cruiser {id}"),
                "OverallRating": "5", "OverallFrontCrashRating": "4",
                "OverallSideCrashRating": "5", "RolloverRating": "3",
                "NHTSAElectronicStabilityControl": "Standard", "ComplaintsCount": 4,
                "RecallsCount": 2, "VehicleId": 1
            }]
        })
    }

    async fn ratings_fixture(list: Value) -> Fixture {
        Fixture::start("", move |seen| {
            if seen.uri.path().contains("/VehicleId/") {
                let id = seen.uri.path().rsplit('/').next().unwrap_or_default();
                Reply::Json(one_rating(id))
            } else {
                Reply::Json(list.clone())
            }
        })
        .await
    }

    #[tokio::test]
    async fn vehicle_safety_ratings_reads_each_vehicle() {
        let ids: Vec<Value> = (101..113).map(|id| json!(id)).collect();
        let nhtsa = ratings_fixture(ratings_list(&ids)).await;
        let answer = asking_nhtsa(&nhtsa)
            .call(
                "vehicle_safety_ratings",
                &json!({ "make": "Toyota", "model": "Land Cruiser", "model_year": 2003 }),
            )
            .await
            .expect("an answer");
        let seen = nhtsa.requests();
        assert_eq!(seen.len(), 11, "the list, then ten vehicles");
        assert_eq!(
            seen[0].uri.path(),
            "/SafetyRatings/modelyear/2003/make/Toyota/model/Land%20Cruiser"
        );
        assert_eq!(seen[0].uri.query(), None);
        let paths: Vec<&str> = seen[1..].iter().map(|one| one.uri.path()).collect();
        let wanted: Vec<String> = (101..111)
            .map(|id| format!("/SafetyRatings/VehicleId/{id}"))
            .collect();
        assert_eq!(paths, wanted);
        assert_eq!(answer["more"], true);
        assert_eq!(keys(&answer), ["more", "ratings"]);
        let ratings = answer["ratings"].as_array().expect("a list");
        assert_eq!(ratings.len(), 10);
        for rating in ratings {
            assert_eq!(
                keys(rating),
                ["frontal", "overall", "rollover", "side", "vehicle"]
            );
        }
        assert_eq!(
            ratings[0],
            json!({
                "vehicle": "2003 Toyota Land Cruiser 101", "overall": "5", "frontal": "4",
                "side": "5", "rollover": "3"
            })
        );
    }

    #[tokio::test]
    async fn vehicle_safety_ratings_skips_an_id_that_is_not_a_positive_integer() {
        let ids = [
            json!(0),
            json!("x"),
            json!(105),
            json!(-3),
            json!(1.5),
            json!(null),
            json!("+5"),
            json!("106"),
            json!(""),
        ];
        let nhtsa = ratings_fixture(ratings_list(&ids)).await;
        let answer = asking_nhtsa(&nhtsa)
            .call(
                "vehicle_safety_ratings",
                &json!({ "make": "Toyota", "model": "Land Cruiser", "model_year": 2003 }),
            )
            .await
            .expect("an answer");
        let paths: Vec<String> = nhtsa
            .requests()
            .iter()
            .map(|one| one.uri.path().to_string())
            .collect();
        assert_eq!(
            paths,
            [
                "/SafetyRatings/modelyear/2003/make/Toyota/model/Land%20Cruiser",
                "/SafetyRatings/VehicleId/105",
                "/SafetyRatings/VehicleId/106"
            ]
        );
        assert_eq!(answer["ratings"].as_array().expect("a list").len(), 2);
        assert_eq!(answer["more"], false);
    }

    #[tokio::test]
    async fn decode_vin_keeps_twelve_fields() {
        let mut fields = serde_json::Map::new();
        for name in [
            "ABS",
            "ActiveSafetySysNote",
            "AirBagLocFront",
            "BasePrice",
            "BatteryType",
            "BodyCabType",
            "BusFloorConfigType",
            "DestinationMarket",
            "EngineManufacturer",
            "EngineModel",
            "ForwardCollisionWarning",
            "GVWR",
            "Manufacturer",
            "ManufacturerId",
            "NCSABodyType",
            "OtherEngineInfo",
            "PlantCity",
            "PlantCompanyName",
            "PlantState",
            "SuggestedVIN",
            "VIN",
            "VehicleType",
            "Series",
            "Turbo",
        ] {
            fields.insert(name.to_string(), json!("a value we do not read"));
        }
        for (name, value) in [
            ("Make", "HONDA"),
            ("Model", "Accord"),
            ("ModelYear", "2003"),
            ("Trim", "EX"),
            ("BodyClass", "Sedan/Saloon"),
            ("EngineCylinders", "4"),
            ("DisplacementL", "2.4"),
            ("FuelTypePrimary", "Gasoline"),
            ("DriveType", "FWD"),
            ("PlantCountry", "UNITED STATES (USA)"),
            ("ErrorCode", "0"),
            (
                "ErrorText",
                "0 - VIN decoded clean. Check Digit (9th position) is correct",
            ),
        ] {
            fields.insert(name.to_string(), json!(value));
        }
        let vpic = Fixture::vpic(Reply::Json(json!({
            "Count": 1, "Message": "Results returned successfully",
            "SearchCriteria": "VIN:1HGCM82633A004352", "Results": [Value::Object(fields)]
        })))
        .await;
        let nhtsa = Fixture::nhtsa(Reply::Json(json!({}))).await;
        let server = Recalls::with_today(
            "http://127.0.0.1:9/RestWebServices",
            &nhtsa.address,
            &vpic.address,
            today(),
        )
        .expect("a server");
        let answer = server
            .call("decode_vin", &json!({ "vin": "1HGCM82633A004352" }))
            .await
            .expect("an answer");
        let seen = vpic.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].uri.path(),
            "/api/vehicles/decodevinvalues/1HGCM82633A004352"
        );
        assert_eq!(seen[0].uri.query(), Some("format=json"));
        assert!(
            nhtsa.requests().is_empty(),
            "vPIC is not NHTSA's other host"
        );
        assert_eq!(
            keys(&answer),
            [
                "BodyClass",
                "DisplacementL",
                "DriveType",
                "EngineCylinders",
                "ErrorCode",
                "ErrorText",
                "FuelTypePrimary",
                "Make",
                "Model",
                "ModelYear",
                "PlantCountry",
                "Trim"
            ]
        );
        assert_eq!(answer["Make"], "HONDA");
        assert_eq!(answer["ErrorCode"], "0");
    }

    fn long(n: usize) -> String {
        "A".repeat(n)
    }

    #[tokio::test]
    async fn refuses_bad_input_before_sending() {
        let hosts = Hosts::answering(&Reply::Json(json!({ "results": [], "Results": [] }))).await;
        let server = hosts.server();
        let vehicle = |make: &str, model: &str, year: Value| json!({ "make": make, "model": model, "model_year": year });
        let mut cases: Vec<(&str, Value)> = vec![
            ("decode_vin", json!({ "vin": "1HGCM82633A00435O" })),
            ("decode_vin", json!({ "vin": "1HGCM82633A00435I" })),
            ("decode_vin", json!({ "vin": "1HGCM82633A00435Q" })),
            ("decode_vin", json!({ "vin": "1HGCM82633A00435" })),
            ("decode_vin", json!({ "vin": "1HGCM82633A0043520" })),
            ("decode_vin", json!({ "vin": "1hgcm82633a004352" })),
            ("decode_vin", json!({ "vin": "1HGCM82633A00435/" })),
            ("decode_vin", json!({ "vin": 12_345_678_901_234_567_u64 })),
            ("decode_vin", json!({})),
            ("product_recalls", json!({ "field": "title", "words": "" })),
            (
                "product_recalls",
                json!({ "field": "title", "words": long(101) }),
            ),
            ("product_recalls", json!({ "field": "title", "words": 7 })),
            ("product_recalls", json!({ "field": "title" })),
            (
                "product_recalls",
                json!({ "field": "name", "words": "baby" }),
            ),
            (
                "product_recalls",
                json!({ "field": "Title", "words": "baby" }),
            ),
            ("product_recalls", json!({ "words": "baby" })),
            (
                "product_recalls",
                json!({ "field": "title", "words": "baby", "since": "2026-02-30" }),
            ),
            (
                "product_recalls",
                json!({ "field": "title", "words": "baby", "since": "2026-2-3" }),
            ),
            (
                "product_recalls",
                json!({ "field": "title", "words": "baby", "since": "20260203" }),
            ),
            (
                "product_recalls",
                json!({ "field": "title", "words": "baby", "since": "yesterday" }),
            ),
            (
                "product_recalls",
                json!({ "field": "title", "words": "baby", "since": 2024 }),
            ),
            ("nothing", json!({})),
        ];
        for tool in [
            "vehicle_recalls",
            "vehicle_complaints",
            "vehicle_safety_ratings",
        ] {
            for case in [
                vehicle("Ho/nda", "Accord", json!(2003)),
                vehicle("Honda", "Acc?ord", json!(2003)),
                vehicle("Honda", "Accord\n", json!(2003)),
                vehicle("Ho%6Eda", "Accord", json!(2003)),
                vehicle("", "Accord", json!(2003)),
                vehicle("Honda", "", json!(2003)),
                vehicle(&long(41), "Accord", json!(2003)),
                vehicle("Honda", &long(41), json!(2003)),
                vehicle("Honda", "Accord", json!(1949)),
                vehicle("Honda", "Accord", json!(2028)),
                vehicle("Honda", "Accord", json!("2003")),
                vehicle("Honda", "Accord", json!(2003.5)),
                vehicle("Honda", "Accord", json!(null)),
                json!({ "make": "Honda", "model": "Accord" }),
                json!({ "make": "Honda", "model_year": 2003 }),
                json!({ "model": "Accord", "model_year": 2003 }),
            ] {
                cases.push((tool, case));
            }
        }
        for (tool, input) in cases {
            assert!(server.call(tool, &input).await.is_err(), "{tool} {input}");
        }
        assert_eq!(hosts.requests(), 0, "something was sent");
        assert_eq!(
            server.call("nothing", &json!({})).await,
            Err("there is no tool named nothing".to_string())
        );
    }

    /// Each limit is the edge: one step inside it is read.
    #[tokio::test]
    async fn takes_the_edges_of_every_input() {
        let hosts = Hosts::split(
            &Reply::Json(json!([])),
            &Reply::Json(json!({ "results": [], "Results": [{}] })),
        )
        .await;
        let server = hosts.server();
        for (tool, input) in [
            (
                "product_recalls",
                json!({ "field": "title", "words": long(100) }),
            ),
            (
                "product_recalls",
                json!({ "field": "product_name", "words": "a", "since": "2024-02-29" }),
            ),
            ("decode_vin", json!({ "vin": "ABCDEFGHJKLMNPRST" })),
            ("decode_vin", json!({ "vin": "0123456789ZYXWVUT" })),
            (
                "vehicle_recalls",
                json!({ "make": long(40), "model": long(40), "model_year": 1950 }),
            ),
            (
                "vehicle_recalls",
                json!({ "make": "a-1 b", "model": "-", "model_year": 2027 }),
            ),
            (
                "vehicle_complaints",
                json!({ "make": "Honda", "model": "Accord", "model_year": 1950 }),
            ),
            (
                "vehicle_complaints",
                json!({ "make": "Honda", "model": "Accord", "model_year": 2027 }),
            ),
        ] {
            server
                .call(tool, &input)
                .await
                .unwrap_or_else(|error| panic!("{tool} {input}: {error}"));
        }
        assert_eq!(hosts.requests(), 8);
    }

    /// The path of a vehicle's ratings is built from segments: whatever the characters, a make or
    /// a model is one segment.
    #[tokio::test]
    async fn a_make_and_a_model_are_one_path_segment_each() {
        let nhtsa = ratings_fixture(ratings_list(&[])).await;
        asking_nhtsa(&nhtsa)
            .call(
                "vehicle_safety_ratings",
                &json!({ "make": "Mercedes-Benz", "model": "S 500", "model_year": 2003 }),
            )
            .await
            .expect("an answer");
        assert_eq!(
            nhtsa.requests()[0].uri.path(),
            "/SafetyRatings/modelyear/2003/make/Mercedes-Benz/model/S%20500"
        );
    }

    #[tokio::test]
    async fn a_refusal_hides_the_agency_s_words() {
        for status in [500, 404, 422] {
            let hosts = Hosts::answering(&Reply::Status(status)).await;
            let server = hosts.server();
            let error = server
                .call(
                    "product_recalls",
                    &json!({ "field": "title", "words": "baby" }),
                )
                .await
                .expect_err("a refusal");
            assert_eq!(error, "The CPSC could not answer that", "{status}");
            for (tool, input) in [
                ("vehicle_recalls", honda()),
                ("vehicle_complaints", honda()),
                ("vehicle_safety_ratings", honda()),
                ("decode_vin", json!({ "vin": "1HGCM82633A004352" })),
            ] {
                let error = server.call(tool, &input).await.expect_err("a refusal");
                assert_eq!(error, "NHTSA could not answer that", "{status} {tool}");
            }
        }
    }

    /// A refusal of one vehicle's own ratings is the same sentence.
    #[tokio::test]
    async fn a_refusal_of_one_vehicle_s_ratings_is_nhtsa_s_too() {
        let nhtsa = Fixture::start("", |seen| {
            if seen.uri.path().contains("/VehicleId/") {
                Reply::Status(500)
            } else {
                Reply::Json(ratings_list(&[json!(7)]))
            }
        })
        .await;
        let error = asking_nhtsa(&nhtsa)
            .call("vehicle_safety_ratings", &honda())
            .await
            .expect_err("a refusal");
        assert_eq!(error, "NHTSA could not answer that");
    }

    #[tokio::test]
    async fn says_so_when_an_answer_is_not_what_was_expected() {
        let hosts = Hosts::answering(&Reply::Json(json!("a string"))).await;
        let server = hosts.server();
        let error = server
            .call(
                "product_recalls",
                &json!({ "field": "title", "words": "baby" }),
            )
            .await
            .expect_err("not a list");
        assert_eq!(error, "The CPSC's answer was not what was expected");
        for (tool, input) in [
            ("vehicle_recalls", honda()),
            ("vehicle_complaints", honda()),
            ("vehicle_safety_ratings", honda()),
            ("decode_vin", json!({ "vin": "1HGCM82633A004352" })),
        ] {
            let error = server.call(tool, &input).await.expect_err("not rows");
            assert_eq!(error, "NHTSA's answer was not what was expected", "{tool}");
        }
        let hosts = Hosts::answering(&Reply::Bytes(b"<html>not json</html>".to_vec())).await;
        let error = hosts
            .server()
            .call("decode_vin", &json!({ "vin": "1HGCM82633A004352" }))
            .await
            .expect_err("not JSON");
        assert_eq!(error, "NHTSA's answer was not what was expected");
    }

    #[tokio::test]
    async fn follows_no_redirect() {
        let elsewhere = Fixture::start("/RestWebServices", |_| Reply::Json(json!([]))).await;
        let to = format!("{}/Recall", elsewhere.address);
        for (tool, input, words) in [
            (
                "product_recalls",
                json!({ "field": "title", "words": "baby" }),
                "The CPSC could not answer that",
            ),
            ("vehicle_recalls", honda(), "NHTSA could not answer that"),
            ("vehicle_complaints", honda(), "NHTSA could not answer that"),
            (
                "vehicle_safety_ratings",
                honda(),
                "NHTSA could not answer that",
            ),
            (
                "decode_vin",
                json!({ "vin": "1HGCM82633A004352" }),
                "NHTSA could not answer that",
            ),
        ] {
            let hosts = Hosts::answering(&Reply::Redirect(to.clone())).await;
            let error = hosts
                .server()
                .call(tool, &input)
                .await
                .expect_err("a redirect is refused");
            assert_eq!(error, words, "{tool}");
            assert_eq!(hosts.requests(), 1, "{tool}");
        }
        assert!(elsewhere.requests().is_empty(), "the redirect was followed");
    }

    #[tokio::test]
    async fn gives_up_after_the_timeout() {
        let silent = Fixture::start("/RestWebServices", |_| Reply::Hang).await;
        let started = Instant::now();
        let error = Recalls::with_timeout(
            &silent.address,
            &silent.address,
            &silent.address,
            today(),
            Duration::from_secs(1),
        )
        .expect("a server")
        .call(
            "product_recalls",
            &json!({ "field": "title", "words": "baby" }),
        )
        .await
        .expect_err("no answer");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(error, "The CPSC did not answer within 1 seconds");
        assert_eq!(
            asking_cpsc(&silent).timeout,
            Duration::from_secs(20),
            "the server waits twenty seconds"
        );
    }

    /// The environment's proxy is never used: a child copy of this test, with the proxy variables
    /// set to a fixture that records, asks a second fixture directly.
    #[tokio::test]
    async fn uses_no_proxy_from_the_environment() {
        if let Ok(target) = std::env::var("FARIK_RECALLS_PROXY_CHILD") {
            asking_cpsc_at(&target)
                .call(
                    "product_recalls",
                    &json!({ "field": "title", "words": "baby" }),
                )
                .await
                .expect("answered");
            return;
        }
        let proxy = Fixture::cpsc(Reply::Json(json!([]))).await;
        let target = Fixture::cpsc(Reply::Json(json!([]))).await;
        let through = proxy
            .address
            .trim_end_matches("/RestWebServices")
            .to_string();
        let child = tokio::process::Command::new(std::env::current_exe().expect("this test"))
            .args([
                "--exact",
                "recalls::tests::uses_no_proxy_from_the_environment",
            ])
            .env("FARIK_RECALLS_PROXY_CHILD", &target.address)
            .env("HTTP_PROXY", &through)
            .env("http_proxy", &through)
            .env("HTTPS_PROXY", &through)
            .env("https_proxy", &through)
            .env("ALL_PROXY", &through)
            .env_remove("NO_PROXY")
            .env_remove("no_proxy")
            .output()
            .await
            .expect("the child runs");
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        assert!(proxy.requests().is_empty(), "the proxy was used");
        assert_eq!(target.requests().len(), 1);
    }

    fn asking_cpsc_at(address: &str) -> Recalls {
        Recalls::with_today(address, address, address, today()).expect("a server")
    }

    /// `{"results":[]}` padded with spaces to `total` bytes: still JSON.
    fn padded(total: usize) -> Reply {
        let mut body = b"{\"results\":[],\"Results\":[]}".to_vec();
        body.resize(total, b' ');
        Reply::Bytes(body)
    }

    #[tokio::test]
    async fn cuts_an_oversized_answer() {
        const MIB: usize = 1024 * 1024;
        let cpsc_words = "The CPSC's answer is too large; ask a narrower question";
        let nhtsa_words = "NHTSA's answer is too large to read here";
        let recall_input = json!({ "field": "title", "words": "baby" });
        // Exactly 4 MiB is read; one byte more is not.
        let at = Fixture::cpsc(Reply::Bytes({
            let mut body = b"[]".to_vec();
            body.resize(4 * MIB, b' ');
            body
        }))
        .await;
        asking_cpsc(&at)
            .call("product_recalls", &recall_input)
            .await
            .expect("exactly 4 MiB is read");
        let over = Fixture::cpsc(Reply::Bytes({
            let mut body = b"[]".to_vec();
            body.resize(4 * MIB + 1, b' ');
            body
        }))
        .await;
        let error = asking_cpsc(&over)
            .call("product_recalls", &recall_input)
            .await
            .expect_err("one byte more");
        assert_eq!(error, cpsc_words);
        for tool in ["vehicle_recalls", "vehicle_safety_ratings"] {
            let at = Fixture::nhtsa(padded(4 * MIB)).await;
            asking_nhtsa(&at)
                .call(tool, &honda())
                .await
                .unwrap_or_else(|error| panic!("{tool}: {error}"));
            let over = Fixture::nhtsa(padded(4 * MIB + 1)).await;
            let error = asking_nhtsa(&over)
                .call(tool, &honda())
                .await
                .expect_err("one byte more");
            assert_eq!(error, nhtsa_words, "{tool}");
        }
        let vin = json!({ "vin": "1HGCM82633A004352" });
        let over = Fixture::vpic(padded(4 * MIB + 1)).await;
        let error = asking_nhtsa(&over)
            .call("decode_vin", &vin)
            .await
            .expect_err("one byte more");
        assert_eq!(error, nhtsa_words);
    }

    /// Complaints alone are read to 8 MiB: a Ford Focus of 2012 is 3.7 MB.
    #[tokio::test]
    async fn reads_a_vehicle_s_complaints_to_eight_mebibytes() {
        const MIB: usize = 1024 * 1024;
        let at = Fixture::nhtsa(padded(8 * MIB)).await;
        let answer = asking_nhtsa(&at)
            .call("vehicle_complaints", &honda())
            .await
            .expect("exactly 8 MiB is read");
        assert_eq!(answer["count"], 0);
        let over = Fixture::nhtsa(padded(8 * MIB + 1)).await;
        let error = asking_nhtsa(&over)
            .call("vehicle_complaints", &honda())
            .await
            .expect_err("one byte more");
        assert_eq!(error, "NHTSA's answer is too large to read here");
        let between = Fixture::nhtsa(padded(4 * MIB + 1)).await;
        asking_nhtsa(&between)
            .call("vehicle_complaints", &honda())
            .await
            .expect("4 MiB and a byte are read for complaints");
    }

    /// No address an answer carries is ever asked for: not the recall's own page, not a `next`
    /// or an `href`.
    #[tokio::test]
    async fn never_asks_for_an_address_an_answer_carries() {
        let elsewhere = Fixture::start("", |_| Reply::Json(json!({}))).await;
        let there = elsewhere.address.clone();
        let mut recall = cpsc_recall("N001", "2024-01-01T00:00:00", "d");
        recall["URL"] = json!(format!("{there}/recall"));
        recall["next"] = json!(format!("{there}/next"));
        let cpsc = Fixture::cpsc(Reply::Json(json!([recall]))).await;
        let nhtsa = ratings_fixture({
            let mut list = ratings_list(&[json!(7)]);
            list["next"] = json!(format!("{there}/next"));
            list["Results"][0]["href"] = json!(format!("{there}/href"));
            list["Results"][0]["itemHref"] = json!(format!("{there}/itemHref"));
            list["results"] = json!([{ "href": format!("{there}/href"), "URL": format!("{there}/URL"), "components": "ENGINE", "dateComplaintFiled": "01/02/2020", "summary": "s" }]);
            list
        })
        .await;
        let vpic = Fixture::vpic(Reply::Json(json!({
            "next": format!("{there}/next"),
            "Results": [{ "Make": "HONDA", "URL": format!("{there}/URL") }]
        })))
        .await;
        let server = Recalls::with_today(&cpsc.address, &nhtsa.address, &vpic.address, today())
            .expect("a server");
        let answer = server
            .call(
                "product_recalls",
                &json!({ "field": "title", "words": "baby" }),
            )
            .await
            .expect("an answer");
        assert_eq!(answer["recalls"][0]["url"], format!("{there}/recall"));
        for (tool, input) in [
            ("vehicle_recalls", honda()),
            ("vehicle_complaints", honda()),
            ("vehicle_safety_ratings", honda()),
            ("decode_vin", json!({ "vin": "1HGCM82633A004352" })),
        ] {
            server
                .call(tool, &input)
                .await
                .unwrap_or_else(|error| panic!("{tool}: {error}"));
        }
        assert!(
            elsewhere.requests().is_empty(),
            "an address from an answer was asked"
        );
    }

    /// Exactly 50 recalls are whole; one more is cut and said so.
    #[tokio::test]
    async fn product_recalls_say_more_only_when_cut() {
        let rows = sixty_recalls();
        for (count, more) in [(50, false), (51, true)] {
            let cpsc = Fixture::cpsc(Reply::Json(Value::Array(rows[..count].to_vec()))).await;
            let answer = asking_cpsc(&cpsc)
                .call(
                    "product_recalls",
                    &json!({ "field": "title", "words": "baby" }),
                )
                .await
                .expect("an answer");
            assert_eq!(answer["recalls"].as_array().expect("a list").len(), 50);
            assert_eq!(answer["more"], more, "{count}");
        }
    }

    /// A recall with fields missing, or of another shape, has empty ones and no cut.
    #[tokio::test]
    async fn a_recall_with_fields_missing_has_empty_ones() {
        let cpsc = Fixture::cpsc(Reply::Json(json!([
            { "RecallNumber": 24_123, "Hazards": "none", "Products": [{}], "Description": 5 }
        ])))
        .await;
        let answer = asking_cpsc(&cpsc)
            .call(
                "product_recalls",
                &json!({ "field": "title", "words": "baby" }),
            )
            .await
            .expect("an answer");
        assert_eq!(
            answer["recalls"][0],
            json!({
                "number": "24123", "date": null, "title": null, "url": null, "description": null,
                "products": [{ "name": null, "model": null, "type": null, "units": null }],
                "hazards": [], "remedies": [], "manufacturers": [], "retailers": [],
                "countries": []
            })
        );
    }

    /// Exactly ten vehicles are whole; one more is cut and said so.
    #[tokio::test]
    async fn vehicle_safety_ratings_say_more_only_when_cut() {
        let ids: Vec<Value> = (101..111).map(|id| json!(id)).collect();
        let nhtsa = ratings_fixture(ratings_list(&ids)).await;
        let answer = asking_nhtsa(&nhtsa)
            .call("vehicle_safety_ratings", &honda())
            .await
            .expect("an answer");
        assert_eq!(answer["ratings"].as_array().expect("a list").len(), 10);
        assert_eq!(answer["more"], false);
        assert_eq!(nhtsa.requests().len(), 11);
    }

    /// A VIN's fields are text or numbers as text; anything else is null.
    #[tokio::test]
    async fn decode_vin_keeps_text_and_numbers_only() {
        let vpic = Fixture::vpic(Reply::Json(json!({
            "Results": [{ "Make": "HONDA", "EngineCylinders": 4, "Trim": { "x": 1 }, "ErrorText": true }]
        })))
        .await;
        let answer = asking_nhtsa(&vpic)
            .call("decode_vin", &json!({ "vin": "1HGCM82633A004352" }))
            .await
            .expect("an answer");
        assert_eq!(answer["Make"], "HONDA");
        assert_eq!(answer["EngineCylinders"], "4");
        assert_eq!(answer["Trim"], Value::Null);
        assert_eq!(answer["ErrorText"], Value::Null);
        assert_eq!(answer["Model"], Value::Null);
    }

    /// NHTSA's services spell the key of their rows `results` or `Results`; either is read.
    #[tokio::test]
    async fn reads_either_spelling_of_results() {
        let upper = Fixture::nhtsa(Reply::Json(json!({ "Results": [] }))).await;
        let lower = Fixture::nhtsa(Reply::Json(json!({ "results": [] }))).await;
        for fixture in [&upper, &lower] {
            let server = asking_nhtsa(fixture);
            assert_eq!(
                server.call("vehicle_recalls", &honda()).await,
                Ok(json!({ "recalls": [] }))
            );
            assert_eq!(
                server.call("vehicle_safety_ratings", &honda()).await,
                Ok(json!({ "ratings": [], "more": false }))
            );
            assert_eq!(
                server
                    .call("vehicle_complaints", &honda())
                    .await
                    .expect("an answer")["count"],
                0
            );
        }
        let error = asking_nhtsa(&upper)
            .call("decode_vin", &json!({ "vin": "1HGCM82633A004352" }))
            .await
            .expect_err("no first row");
        assert_eq!(error, "NHTSA's answer was not what was expected");
    }

    /// An address written with a closing slash is still one path, not an empty segment before the
    /// first.
    #[tokio::test]
    async fn an_address_with_a_closing_slash_is_not_doubled() {
        let cpsc = Fixture::cpsc(Reply::Json(json!([]))).await;
        let slashed = format!("{}/", cpsc.address);
        Recalls::with_today(&slashed, &slashed, &slashed, today())
            .expect("a server")
            .call(
                "product_recalls",
                &json!({ "field": "title", "words": "baby" }),
            )
            .await
            .expect("an answer");
        assert_eq!(cpsc.requests()[0].uri.path(), "/RestWebServices/Recall");
    }

    /// A host that does not answer at all is said to be out of reach, without the reason.
    #[tokio::test]
    async fn says_so_when_an_agency_cannot_be_reached() {
        // Held, bound and not listening, for the whole test: a connect is refused, and no other
        // test's server can take the port.
        let held = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)
            .expect("a socket");
        held.bind(&std::net::SocketAddr::from(([127, 0, 0, 1], 0)).into())
            .expect("a port");
        let address = held
            .local_addr()
            .expect("an address")
            .as_socket()
            .expect("an IP address");
        let closed = format!("http://{address}");
        assert!(
            tokio::net::TcpListener::bind(address).await.is_err(),
            "another server cannot take the held port"
        );
        let server = Recalls::with_today(&closed, &closed, &closed, today()).expect("a server");
        let error = server
            .call(
                "product_recalls",
                &json!({ "field": "title", "words": "baby" }),
            )
            .await
            .expect_err("nobody answers");
        assert_eq!(error, "The CPSC could not be reached");
        let error = server
            .call("vehicle_recalls", &honda())
            .await
            .expect_err("nobody answers");
        assert_eq!(error, "NHTSA could not be reached");
    }
}
