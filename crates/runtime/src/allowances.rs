//! A kit connector's allowances (`docs/SPEC.md` 6.7, ADR 0037): how many calls each period an
//! agent makes of a spending tool without asking.

use std::collections::BTreeMap;

use farik_core::governor::permissions::MAX_ALLOWANCE;
use farik_roles::{Kit, KitConnector};
use serde_json::Value;

/// The allowances a connector entry is written with: the kit's defaults for the service `name`,
/// overlaid by `asked`.
///
/// # Errors
///
/// `allowance_not_offered` for a tool the kit gives no allowance, and `allowance_out_of_range`
/// above 1,000, each with its sentence. A service the kit lacks gives no defaults and offers
/// nothing, so any `asked` is not offered.
pub fn checked_allowances(
    kit: &Kit,
    name: &str,
    asked: &BTreeMap<String, u32>,
) -> Result<BTreeMap<String, u32>, String> {
    let offered = kit.connectors.iter().find_map(|connector| match connector {
        KitConnector::Server {
            entry, allowances, ..
        } if entry.name.as_str() == name => Some(allowances),
        _ => None,
    });
    let mut checked: BTreeMap<String, u32> = offered
        .into_iter()
        .flatten()
        .map(|(tool, offer)| (tool.clone(), offer.calls))
        .collect();
    for (tool, calls) in asked {
        if !checked.contains_key(tool) {
            return Err(format!(
                "allowance_not_offered: the kit gives {tool} no allowance, so it asks every time"
            ));
        }
        if *calls > MAX_ALLOWANCE {
            return Err(format!(
                "allowance_out_of_range: {calls} is more than {MAX_ALLOWANCE}; give {tool} from 0 \
                 to {MAX_ALLOWANCE}"
            ));
        }
        checked.insert(tool.clone(), *calls);
    }
    Ok(checked)
}

/// The allowances a request asked for, each tool with a whole number of calls.
///
/// # Errors
///
/// `allowance_out_of_range` for a value that is not a whole number from 0 up to `u32::MAX`.
pub fn asked_allowances(value: &Value) -> Result<BTreeMap<String, u32>, String> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(tool, calls)| {
            calls
                .as_u64()
                .and_then(|calls| u32::try_from(calls).ok())
                .map(|calls| (tool.clone(), calls))
                .ok_or_else(|| {
                    format!(
                        "allowance_out_of_range: give {tool} a whole number of calls from 0 to \
                         {MAX_ALLOWANCE}"
                    )
                })
        })
        .collect()
}
