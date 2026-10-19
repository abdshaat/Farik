//! Which ready-made team fits a folder, from the paths git tracks in it
//! (`docs/design/team-picker.md` section 5).

use serde::Serialize;

/// What put a team first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// Source code or a manifest.
    Code,
    /// Spreadsheets and ledgers.
    Money,
    /// A shop's files.
    Shop,
    /// Nothing the folder shows fits a team yet.
    New,
}

/// One ready-made team, as the folder fits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestedTeam {
    /// The built-in team's slug.
    pub slug: String,
    /// Why it is where it is.
    pub signal: Signal,
    /// The language the scan found, if any.
    pub language: Option<String>,
    /// Whether the scan found tests.
    pub tests: bool,
}

/// The built-in teams the folder's signals pick between, with their slugs, in the order they win
/// a tie. Built-in teams not named here are the caller's to append, last.
const KNOWN: [(&str, Signal); 3] = [
    ("software-product", Signal::Code),
    ("online-store", Signal::Shop),
    ("financial-analysis", Signal::Money),
];

const CODE_EXTENSIONS: [&str; 24] = [
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "rb", "php", "c", "cc",
    "cpp", "h", "hpp", "cs", "swift", "scala", "dart", "ex", "vue",
];
const MANIFESTS: [&str; 10] = [
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "Gemfile",
    "pom.xml",
    "build.gradle",
    "composer.json",
    "mix.exs",
    "pubspec.yaml",
];
const MONEY_EXTENSIONS: [&str; 9] = [
    "csv",
    "xlsx",
    "xls",
    "ods",
    "numbers",
    "qbo",
    "ofx",
    "beancount",
    "ledger",
];
const MONEY_WORDS: [&str; 5] = ["ledger", "budget", "invoice", "portfolio", "finance"];
const SHOP_WORDS: [&str; 6] = ["product", "inventory", "catalog", "shop", "store", "order"];

/// Which team the ready-made teams are ordered for, by how many tracked files fit each signal.
/// Each file counts once: as code by its extension or manifest, otherwise as a shop's by its
/// extension or name, otherwise as money's.
#[must_use]
pub fn suggest_teams(
    tracked: &[String],
    language: Option<&str>,
    tests: bool,
) -> Vec<SuggestedTeam> {
    let (mut code, mut shop, mut money) = (0_usize, 0_usize, 0_usize);
    for path in tracked {
        match file_signal(path) {
            Some(Signal::Code) => code += 1,
            Some(Signal::Shop) => shop += 1,
            Some(Signal::Money) => money += 1,
            _ => {}
        }
    }
    // A language the scan found is code even when the extension table has not met it.
    if language.is_some() {
        code = code.max(1);
    }
    let count = |signal: Signal| match signal {
        Signal::Code => code,
        Signal::Shop => shop,
        Signal::Money => money,
        Signal::New => 0,
    };
    let mut ranked: Vec<(&str, Signal)> = KNOWN.to_vec();
    // A stable sort keeps the tie order of `KNOWN`: code first.
    ranked.sort_by_key(|(_, signal)| std::cmp::Reverse(count(*signal)));
    ranked
        .into_iter()
        .map(|(slug, signal)| SuggestedTeam {
            slug: slug.to_string(),
            signal: if count(signal) == 0 {
                Signal::New
            } else {
                signal
            },
            language: language.map(ToString::to_string),
            tests,
        })
        .collect()
}

/// What one tracked path says about the folder, if anything.
fn file_signal(path: &str) -> Option<Signal> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, ext)| ext);
    if MANIFESTS.contains(&name) || extension.is_some_and(|ext| CODE_EXTENSIONS.contains(&ext)) {
        return Some(Signal::Code);
    }
    if extension == Some("liquid") || SHOP_WORDS.iter().any(|word| lower.contains(word)) {
        return Some(Signal::Shop);
    }
    if extension.is_some_and(|ext| MONEY_EXTENSIONS.contains(&ext))
        || MONEY_WORDS.iter().any(|word| lower.contains(word))
    {
        return Some(Signal::Money);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Signal, suggest_teams};

    fn order(tracked: &[&str], language: Option<&str>) -> Vec<(String, Signal)> {
        let tracked: Vec<String> = tracked.iter().map(ToString::to_string).collect();
        suggest_teams(&tracked, language, true)
            .into_iter()
            .map(|team| (team.slug, team.signal))
            .collect()
    }

    fn slugs(order: &[(String, Signal)]) -> Vec<&str> {
        order.iter().map(|(slug, _)| slug.as_str()).collect()
    }

    #[test]
    fn orders_the_ready_made_teams_by_what_the_folder_holds() {
        let code = order(
            &["src/main.rs", "src/lib.rs", "Cargo.toml", "budget.csv"],
            Some("Rust"),
        );
        assert_eq!(
            slugs(&code),
            ["software-product", "financial-analysis", "online-store"],
            "the rest follow by how much of the folder they fit"
        );
        assert_eq!(code[0].1, Signal::Code);

        let money = order(&["books/2026.xlsx", "ledger.beancount", "notes.md"], None);
        assert_eq!(slugs(&money)[0], "financial-analysis");
        assert_eq!(money[0].1, Signal::Money);

        let shop = order(
            &["layout/theme.liquid", "sections/hero.liquid", "a.csv"],
            None,
        );
        assert_eq!(slugs(&shop)[0], "online-store");
        assert_eq!(shop[0].1, Signal::Shop);

        let tie = order(&["a.csv", "b.rs"], Some("Rust"));
        assert_eq!(slugs(&tie)[0], "software-product", "code wins a tie");

        for empty in [order(&[], None), order(&["README.md"], None)] {
            assert_eq!(slugs(&empty)[0], "software-product");
            assert_eq!(empty[0].1, Signal::New);
            assert_eq!(empty.len(), 3, "every team once");
        }

        let wire = suggest_teams(&["a.rs".to_string()], Some("Rust"), true);
        assert_eq!(
            serde_json::to_value(&wire[0]).expect("a team is JSON"),
            serde_json::json!({
                "slug": "software-product", "signal": "code", "language": "Rust", "tests": true
            })
        );
    }
}
