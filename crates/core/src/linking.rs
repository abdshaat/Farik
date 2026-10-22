//! The proof that two found projects work together, from their committed files, and the hints that
//! only suggest it (`docs/SPEC.md` 4.4; ADR 0059). Pure: the files come in as strings. A hint is
//! never a proof, and only a proof the owner says Yes to makes a link.

use std::collections::BTreeSet;

use chrono::NaiveDate;

/// The corpus of tempting pairs the proofs are held to.
pub mod fixtures;

/// A project and the committed files of its repository that proofs are found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFiles {
    /// The project's key.
    pub key: String,
    /// The name shown.
    pub name: String,
    /// The repository's normalised remote, when it has one.
    pub remote_key: Option<String>,
    /// The project's folder; `None` for a project with no folder on this computer.
    pub root: Option<String>,
    /// `(path relative to the root, text)` of each file read at `HEAD`.
    pub files: Vec<(String, String)>,
}

/// What step 04's index holds of a claude.ai Project, as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeAiProjectText {
    /// Its id, which is its key.
    pub uuid: String,
    /// Its name.
    pub name: String,
    /// Its description.
    pub description: Option<String>,
    /// Its documents' file names.
    pub file_names: Vec<String>,
}

/// What kind of proof, in the order the screen prefers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProofKind {
    /// One uses the other's code through a local path.
    PathDependency,
    /// One includes the other as a submodule.
    Submodule,
    /// One uses a package the other's manifest names and claims.
    DeclaredPackage,
    /// One deploys the other.
    Deploys,
    /// One's workflow works on the other's repository.
    Workflow,
    /// One names the domain the other deploys to.
    Domain,
    /// A claude.ai Project's description or file names hold the other's address.
    ClaudeAiNames,
}

/// One end of a proof: where it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofEnd {
    /// The project whose file it is.
    pub project: String,
    /// The file, relative to the project's root; for a claude.ai Project `description` or a file
    /// name.
    pub file: String,
    /// The line, counted from 1.
    pub line: usize,
    /// The line, trimmed and cut to `TEXT_LIMIT` characters.
    pub text: String,
}

/// One proof that `a` and `b` work together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    /// The kind.
    pub kind: ProofKind,
    /// The project that uses, includes, deploys or names the other (a key).
    pub a: String,
    /// The project that is used, included, deployed or named (a key).
    pub b: String,
    /// One or two lines it was read from.
    pub ends: Vec<ProofEnd>,
    /// The proof in one plain sentence.
    pub sentence: String,
}

/// How many characters of a line a proof's end keeps.
pub const TEXT_LIMIT: usize = 160;

/// The most a proof reads: a line of text, with its number counted from 1.
type Line<'a> = (usize, &'a str);

fn lines_of(text: &str) -> Vec<Line<'_>> {
    text.lines()
        .enumerate()
        .map(|(at, line)| (at + 1, line))
        .collect()
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|found| {
            extensions
                .iter()
                .any(|extension| found.eq_ignore_ascii_case(extension))
        })
}

fn is_yaml(path: &str) -> bool {
    has_extension(path, &["yml", "yaml"])
}

fn is_compose(path: &str) -> bool {
    let name = base_name(path);
    (name.starts_with("docker-compose") || name.starts_with("compose")) && is_yaml(name)
}

fn is_workflow(path: &str) -> bool {
    path.strip_prefix(".github/workflows/")
        .is_some_and(|name| !name.contains('/') && is_yaml(name))
}

fn is_cluster_file(path: &str) -> bool {
    (path.starts_with("k8s/") || path.starts_with("deploy/")) && is_yaml(path)
}

/// `base` joined with `relative` (or `relative` alone when it is absolute), with `.` and `..`
/// resolved by text alone: no folder is looked at. `None` when it climbs out of the root.
fn resolve(root: &str, dir: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let joined: Vec<&str> = if relative.starts_with('/') {
        vec![relative]
    } else {
        vec![root, dir, relative]
    };
    for piece in joined {
        for part in piece.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                other => parts.push(other),
            }
        }
    }
    Some(format!("/{}", parts.join("/")))
}

fn is_within(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn end_of(project: &str, file: &str, line: Line<'_>) -> ProofEnd {
    ProofEnd {
        project: project.to_string(),
        file: file.to_string(),
        line: line.0,
        text: line.1.trim().chars().take(TEXT_LIMIT).collect(),
    }
}

/// `"key": "value"` at the start of a line, as JSON writes a string member.
fn json_pair(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim().strip_prefix('"')?;
    let (key, rest) = rest.split_once('"')?;
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let (value, _) = rest.split_once('"')?;
    Some((key, value))
}

/// `key = "value"` or `key: value` as TOML or YAML write it, the value unquoted and cut at a
/// comment.
fn key_value(line: &str, separator: char) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    let (key, value) = trimmed.split_once(separator)?;
    let value = value.trim();
    let value = match value.split_once(" #") {
        Some((before, _)) => before.trim(),
        None => value,
    };
    let value = value.trim_matches(|c| c == '"' || c == '\'');
    let key = key.trim().trim_matches('"');
    (!key.is_empty() && !key.contains(' ')).then_some((key, value))
}

fn quoted_strings(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(stop) = after.find('"') else { break };
        found.push(&after[..stop]);
        rest = &after[stop + 1..];
    }
    found
}

/// The last line of the YAML step that begins at `from` (exclusive): the step ends at the next
/// item of the list it is in, or when the indent falls below it.
fn step_end(lines: &[Line<'_>], from: usize) -> usize {
    let base = indent_of(lines[from].1);
    for (at, (_, line)) in lines.iter().enumerate().skip(from + 1) {
        if line.trim().is_empty() {
            continue;
        }
        let indent = indent_of(line);
        if indent < base || (indent == base && line.trim_start().starts_with("- ")) {
            return at;
        }
    }
    lines.len()
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Whether `needle` stands alone in `haystack` (both lower case) at `at`: not inside a longer
/// name. A `.git` after a repository address is allowed.
fn stands_alone(haystack: &str, at: usize, needle: &str, allow_git: bool) -> bool {
    let before = haystack[..at].chars().next_back();
    if before.is_some_and(|c| is_identifier_char(c) || c == '.') {
        return false;
    }
    let mut rest = &haystack[at + needle.len()..];
    if allow_git {
        rest = rest.strip_prefix(".git").unwrap_or(rest);
    }
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) if is_identifier_char(c) => false,
        Some('.') => chars
            .next()
            .is_none_or(|next| !next.is_ascii_alphanumeric()),
        None | Some(_) => true,
    }
}

fn mentions(haystack: &str, needle: &str, allow_git: bool) -> bool {
    let haystack = haystack.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = haystack[from..].find(needle) {
        let at = from + found;
        if stands_alone(&haystack, at, needle, allow_git) {
            return true;
        }
        from = at + 1;
    }
    false
}

/// A project that has a folder, and the others it is read against.
struct Context<'a> {
    projects: &'a [ProjectFiles],
}

impl Context<'_> {
    /// The project whose root holds `path` most closely, when it is not `except`.
    fn holder(&self, path: &str, except: usize) -> Option<usize> {
        let holder = self
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| {
                project
                    .root
                    .as_deref()
                    .is_some_and(|root| is_within(path, root))
            })
            .max_by_key(|(_, project)| project.root.as_deref().map_or(0, str::len))?
            .0;
        (holder != except).then_some(holder)
    }

    fn proof(&self, kind: ProofKind, a: usize, b: usize, ends: Vec<ProofEnd>) -> Proof {
        let (a, b) = (&self.projects[a], &self.projects[b]);
        Proof {
            kind,
            a: a.key.clone(),
            b: b.key.clone(),
            ends,
            sentence: sentence(kind, &a.name, &b.name),
        }
    }
}

fn sentence(kind: ProofKind, a: &str, b: &str) -> String {
    match kind {
        ProofKind::PathDependency => format!("{a} uses {b}'s code."),
        ProofKind::Submodule => format!("{a} includes {b}."),
        ProofKind::DeclaredPackage => format!("{a} uses {b}'s package."),
        ProofKind::Deploys => format!("{a} deploys {b}."),
        ProofKind::Workflow => format!("{a}'s workflow works on {b}."),
        ProofKind::Domain => format!("{a} talks to {b}'s site."),
        ProofKind::ClaudeAiNames => format!("Your claude.ai Project {a} is about {b}."),
    }
}

/// The local paths a manifest points at, each with its line, and the manifest it would be the
/// folder of.
fn local_paths<'a>(file: &str, lines: &[Line<'a>]) -> Vec<(Line<'a>, String, &'static str)> {
    let mut found = Vec::new();
    match base_name(file) {
        "package.json" => {
            for &line in lines {
                if let Some((_, value)) = json_pair(line.1)
                    && let Some(path) = value
                        .strip_prefix("file:")
                        .or_else(|| value.strip_prefix("link:"))
                {
                    found.push((line, path.to_string(), "package.json"));
                }
            }
        }
        "Cargo.toml" => {
            let mut members = false;
            for &line in lines {
                let text = line.1.trim();
                if members || text.starts_with("members") {
                    for path in quoted_strings(text)
                        .into_iter()
                        .filter(|path| !path.contains('*'))
                    {
                        found.push((line, path.to_string(), "Cargo.toml"));
                    }
                    members = !text.contains(']');
                    continue;
                }
                let mut rest = text;
                while let Some(at) = rest.find("path") {
                    let before = rest[..at].chars().next_back();
                    let after = rest[at + 4..].trim_start();
                    if before.is_none_or(|c| c == ' ' || c == '{' || c == ',')
                        && let Some(after) = after.strip_prefix('=')
                        && let Some(value) = quoted_strings(after.trim_start()).first()
                    {
                        found.push((line, (*value).to_string(), "Cargo.toml"));
                    }
                    rest = &rest[at + 4..];
                }
            }
        }
        "go.mod" => {
            for &line in lines {
                if let Some((_, target)) = line.1.split_once("=>")
                    && let Some(path) = target.split_whitespace().next()
                    && (path.starts_with("./") || path.starts_with("../") || path.starts_with('/'))
                {
                    found.push((line, path.to_string(), "go.mod"));
                }
            }
        }
        _ => {}
    }
    found
}

/// The line of `manifest` that names a project (its `name`, or a Go module), when it has one.
fn naming_line<'a>(project: &'a ProjectFiles, path: &str) -> Option<Line<'a>> {
    let (_, text) = project.files.iter().find(|(file, _)| file == path)?;
    lines_of(text)
        .into_iter()
        .find(|(_, line)| match base_name(path) {
            "package.json" => json_pair(line).is_some_and(|(key, _)| key == "name"),
            "Cargo.toml" => key_value(line, '=').is_some_and(|(key, _)| key == "name"),
            _ => line.trim_start().starts_with("module "),
        })
}

fn path_dependencies(cx: &Context<'_>, a: usize, found: &mut Vec<Proof>) {
    let project = &cx.projects[a];
    let Some(root) = project.root.as_deref() else {
        return;
    };
    for (file, text) in &project.files {
        for (line, path, manifest) in local_paths(file, &lines_of(text)) {
            let Some(resolved) = resolve(root, dir_of(file), &path) else {
                continue;
            };
            let Some(b) = cx.holder(&resolved, a) else {
                continue;
            };
            let target = &cx.projects[b];
            let mut ends = vec![end_of(&project.key, file, line)];
            if let Some(target_root) = target.root.as_deref() {
                let inside = resolved
                    .strip_prefix(target_root.trim_end_matches('/'))
                    .unwrap_or("")
                    .trim_matches('/');
                let manifest_path = if inside.is_empty() {
                    manifest.to_string()
                } else {
                    format!("{inside}/{manifest}")
                };
                if let Some(named) = naming_line(target, &manifest_path) {
                    ends.push(end_of(&target.key, &manifest_path, named));
                }
            }
            found.push(cx.proof(ProofKind::PathDependency, a, b, ends));
        }
    }
}

fn submodules(cx: &Context<'_>, a: usize, found: &mut Vec<Proof>) {
    let project = &cx.projects[a];
    for (file, text) in &project.files {
        if file != ".gitmodules" {
            continue;
        }
        for line in lines_of(text) {
            let Some(("url", address)) = key_value(line.1, '=') else {
                continue;
            };
            let Some(remote) = crate::claude_history::normalise_remote(address) else {
                continue;
            };
            for (b, other) in cx.projects.iter().enumerate() {
                if b != a && other.remote_key.as_deref() == Some(remote.as_str()) {
                    found.push(cx.proof(
                        ProofKind::Submodule,
                        a,
                        b,
                        vec![end_of(&project.key, file, line)],
                    ));
                }
            }
        }
    }
}

/// A repository address as a manifest writes it: `github:o/n`, `o/n`, or a URL.
fn repository_key(address: &str) -> Option<String> {
    let address = address.trim();
    let address = address.strip_prefix("git+").unwrap_or(address);
    if let Some(short) = address.strip_prefix("github:") {
        return crate::claude_history::normalise_remote(&format!("https://github.com/{short}"));
    }
    let shorthand = address.split('/').count() == 2
        && !address.contains(':')
        && !address.contains(' ')
        && !address.starts_with('.');
    if shorthand {
        return crate::claude_history::normalise_remote(&format!("https://github.com/{address}"));
    }
    crate::claude_history::normalise_remote(address)
}

/// A package a project's manifest names, and whether its repository is the project's own.
struct Claim {
    name: String,
    file: String,
    line: usize,
    text: String,
    owned: bool,
}

fn claims_of(project: &ProjectFiles) -> Vec<Claim> {
    let mut claims = Vec::new();
    for (file, text) in &project.files {
        let lines = lines_of(text);
        let (name, repository) = match base_name(file) {
            "package.json" => {
                let top = lines
                    .iter()
                    .find(|(_, line)| line.trim_start().starts_with('"'))
                    .map_or(0, |(_, line)| indent_of(line));
                let name = lines.iter().find(|(_, line)| {
                    indent_of(line) == top && json_pair(line).is_some_and(|(key, _)| key == "name")
                });
                let mut repository = None;
                for (at, (_, line)) in lines.iter().enumerate() {
                    if indent_of(line) != top || !line.trim_start().starts_with("\"repository\"") {
                        continue;
                    }
                    repository = match json_pair(line) {
                        Some((_, value)) => Some(value.to_string()),
                        None => lines[at + 1..]
                            .iter()
                            .take_while(|(_, next)| indent_of(next) > top)
                            .find_map(|(_, next)| json_pair(next).filter(|(key, _)| *key == "url"))
                            .map(|(_, value)| value.to_string()),
                    };
                    break;
                }
                (
                    name.copied()
                        .and_then(|line| json_pair(line.1).map(|(_, v)| (v.to_string(), line))),
                    repository,
                )
            }
            "Cargo.toml" => {
                let mut in_package = false;
                let (mut name, mut repository) = (None, None);
                for &line in &lines {
                    let text = line.1.trim();
                    if text.starts_with('[') {
                        in_package = text == "[package]";
                    } else if in_package && let Some((key, value)) = key_value(text, '=') {
                        if key == "name" && name.is_none() {
                            name = Some((value.to_string(), line));
                        } else if key == "repository" {
                            repository = Some(value.to_string());
                        }
                    }
                }
                (name, repository)
            }
            _ => continue,
        };
        let Some((name, line)) = name else { continue };
        let owned = repository
            .as_deref()
            .and_then(repository_key)
            .is_some_and(|key| project.remote_key.as_deref() == Some(key.as_str()));
        claims.push(Claim {
            name,
            file: file.clone(),
            line: line.0,
            text: line.1.trim().chars().take(TEXT_LIMIT).collect(),
            owned,
        });
    }
    claims
}

fn declared_packages(cx: &Context<'_>, found: &mut Vec<Proof>) {
    // A name counts when exactly one project claims it, with its own repository.
    let mut claimed: Vec<(String, usize, Claim)> = Vec::new();
    let mut all_names: Vec<String> = Vec::new();
    for (at, project) in cx.projects.iter().enumerate() {
        for claim in claims_of(project) {
            all_names.push(claim.name.clone());
            if claim.owned {
                claimed.push((claim.name.clone(), at, claim));
            }
        }
    }
    for (name, b, claim) in &claimed {
        if all_names.iter().filter(|other| *other == name).count() != 1 {
            continue;
        }
        for (a, project) in cx.projects.iter().enumerate() {
            if a == *b {
                continue;
            }
            for (file, text) in &project.files {
                let uses = lines_of(text)
                    .into_iter()
                    .find(|line| match base_name(file) {
                        "package.json" => {
                            json_pair(line.1).is_some_and(|(key, _)| key == name)
                                || line.1.trim_start().starts_with(&format!("\"{name}\":"))
                        }
                        "Cargo.toml" => {
                            let text = line.1.trim();
                            let key = text.split(['=', '.']).next().unwrap_or("").trim();
                            key == name
                                || text
                                    .trim_matches(['[', ']'])
                                    .ends_with(&format!("dependencies.{name}"))
                        }
                        _ => false,
                    });
                if let Some(line) = uses {
                    let own = ProofEnd {
                        project: cx.projects[*b].key.clone(),
                        file: claim.file.clone(),
                        line: claim.line,
                        text: claim.text.clone(),
                    };
                    found.push(cx.proof(
                        ProofKind::DeclaredPackage,
                        a,
                        *b,
                        vec![end_of(&project.key, file, line), own],
                    ));
                    break;
                }
            }
        }
    }
}

/// An image reference without its tag or digest, when it names a registry or owner.
fn image_name(reference: &str) -> Option<String> {
    let reference = reference
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == ',');
    let reference = reference.split('@').next().unwrap_or(reference);
    let slash = reference.rfind('/')?;
    let name = match reference[slash..].find(':') {
        Some(colon) => &reference[..slash + colon],
        None => reference,
    };
    (!name.contains('$') && !name.contains('{') && !name.starts_with('/'))
        .then(|| name.to_ascii_lowercase())
}

/// The images a project's workflows publish, with the line that does.
fn published_images(project: &ProjectFiles) -> Vec<(String, String, usize, String)> {
    let mut images = Vec::new();
    for (file, text) in &project.files {
        if !is_workflow(file) {
            continue;
        }
        let lines = lines_of(text);
        for (at, &line) in lines.iter().enumerate() {
            let mut tagged: Vec<Line<'_>> = Vec::new();
            if line.1.contains("docker/build-push-action") {
                let end = step_end(&lines, at);
                let mut inside = lines[at..end].iter().copied().peekable();
                while let Some(candidate) = inside.next() {
                    let Some(("tags", value)) = key_value(candidate.1, ':') else {
                        continue;
                    };
                    let base = indent_of(candidate.1);
                    tagged.extend(
                        value
                            .split(',')
                            .filter(|part| !part.trim().is_empty())
                            .map(|part| (candidate.0, part)),
                    );
                    while let Some(next) =
                        inside.next_if(|next| indent_of(next.1) > base || next.1.trim().is_empty())
                    {
                        let item = next.1.trim().trim_start_matches("- ").trim();
                        tagged.extend(
                            item.split(',')
                                .filter(|part| !part.trim().is_empty())
                                .map(|part| (next.0, part)),
                        );
                    }
                }
            } else if let Some((_, after)) = line.1.split_once("docker push ")
                && let Some(reference) =
                    after.split_whitespace().find(|word| !word.starts_with('-'))
            {
                tagged.push((line.0, reference));
            }
            for (number, reference) in tagged {
                if let Some(name) = image_name(reference) {
                    images.push((name, file.clone(), number, lines[number - 1].1.to_string()));
                }
            }
        }
    }
    images
}

/// The module address in a Terraform `source`, as a repository key.
fn module_remote(source: &str) -> Option<String> {
    let source = source.strip_prefix("git::").unwrap_or(source);
    let source = source.split('?').next().unwrap_or(source);
    let source = if source.starts_with("github.com/") || source.starts_with("bitbucket.org/") {
        format!("https://{source}")
    } else {
        source.to_string()
    };
    let (scheme, rest) = match source.split_once("://") {
        Some((scheme, rest)) => (format!("{scheme}://"), rest),
        None => (String::new(), source.as_str()),
    };
    let rest = rest.split("//").next().unwrap_or(rest);
    crate::claude_history::normalise_remote(&format!("{scheme}{rest}"))
}

fn deploys(cx: &Context<'_>, a: usize, found: &mut Vec<Proof>) {
    let project = &cx.projects[a];
    let published: Vec<_> = cx.projects.iter().map(published_images).collect();
    for (file, text) in &project.files {
        let lines = lines_of(text);
        if is_compose(file)
            && let Some(root) = project.root.as_deref()
        {
            for &line in &lines {
                let Some((key, value)) = key_value(line.1, ':') else {
                    continue;
                };
                if (key != "build" && key != "context")
                    || value.is_empty()
                    || value.contains("://")
                    || value.starts_with("git@")
                    || value.contains('$')
                {
                    continue;
                }
                if let Some(resolved) = resolve(root, dir_of(file), value)
                    && let Some(b) = cx.holder(&resolved, a)
                {
                    found.push(cx.proof(
                        ProofKind::Deploys,
                        a,
                        b,
                        vec![end_of(&project.key, file, line)],
                    ));
                }
            }
        }
        if has_extension(file, &["tf"]) {
            for &line in &lines {
                let Some(("source", value)) = key_value(line.1, '=') else {
                    continue;
                };
                let Some(remote) = module_remote(value) else {
                    continue;
                };
                for (b, other) in cx.projects.iter().enumerate() {
                    if b != a && other.remote_key.as_deref() == Some(remote.as_str()) {
                        found.push(cx.proof(
                            ProofKind::Deploys,
                            a,
                            b,
                            vec![end_of(&project.key, file, line)],
                        ));
                    }
                }
            }
        }
        if is_compose(file) || is_cluster_file(file) {
            for &line in &lines {
                let Some(("image", value)) = key_value(line.1, ':') else {
                    continue;
                };
                let Some(name) = image_name(value) else {
                    continue;
                };
                for (b, images) in published.iter().enumerate() {
                    if b == a {
                        continue;
                    }
                    if let Some((_, path, number, text)) =
                        images.iter().find(|image| image.0 == name)
                    {
                        let theirs = ProofEnd {
                            project: cx.projects[b].key.clone(),
                            file: path.clone(),
                            line: *number,
                            text: text.trim().chars().take(TEXT_LIMIT).collect(),
                        };
                        found.push(cx.proof(
                            ProofKind::Deploys,
                            a,
                            b,
                            vec![end_of(&project.key, file, line), theirs],
                        ));
                    }
                }
            }
        }
    }
}

/// The `owner/name` repositories a line of a workflow calls a dispatch on.
fn dispatch_targets(line: &str) -> Vec<String> {
    let segment = |text: &str| -> String {
        text.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect()
    };
    let mut targets = Vec::new();
    if line.contains("/dispatches")
        && let Some((_, after)) = line.split_once("repos/")
        && let Some((owner, rest)) = after.split_once('/')
    {
        targets.push(format!("{}/{}", segment(owner), segment(rest)));
    }
    if line.contains("gh workflow run") || line.contains("gh run") {
        for flag in ["--repo ", "--repo=", "-R "] {
            if let Some((_, after)) = line.split_once(flag)
                && let Some((owner, rest)) = after.trim_start().split_once('/')
            {
                targets.push(format!("{}/{}", segment(owner), segment(rest)));
            }
        }
    }
    targets.retain(|target| target.len() > 1 && !target.starts_with('/') && !target.ends_with('/'));
    targets
}

fn workflows(cx: &Context<'_>, a: usize, found: &mut Vec<Proof>) {
    let project = &cx.projects[a];
    for (file, text) in &project.files {
        if !is_workflow(file) {
            continue;
        }
        let lines = lines_of(text);
        let mut named: Vec<(Line<'_>, String)> = Vec::new();
        for (at, &line) in lines.iter().enumerate() {
            if line.1.contains("uses: actions/checkout")
                || line.1.contains("uses: peter-evans/repository-dispatch")
            {
                let end = step_end(&lines, at);
                for &inside in &lines[at..end] {
                    if let Some(("repository", value)) = key_value(inside.1, ':') {
                        named.push((inside, value.to_ascii_lowercase()));
                    }
                }
            }
            for target in dispatch_targets(line.1) {
                named.push((line, target.to_ascii_lowercase()));
            }
        }
        for (line, target) in named {
            let remote = format!("github.com/{target}");
            for (b, other) in cx.projects.iter().enumerate() {
                if b != a && other.remote_key.as_deref() == Some(remote.as_str()) {
                    found.push(cx.proof(
                        ProofKind::Workflow,
                        a,
                        b,
                        vec![end_of(&project.key, file, line)],
                    ));
                }
            }
        }
    }
}

/// Whether `host` is a domain name of at least two labels.
fn is_domain(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && labels
            .last()
            .is_some_and(|last| last.len() >= 2 && last.chars().all(|c| c.is_ascii_alphabetic()))
}

/// Hosts that a manifest's `homepage` names without being where the project is deployed.
const SHARED_HOSTS: [&str; 6] = [
    "github.com",
    "gitlab.com",
    "bitbucket.org",
    "npmjs.com",
    "www.npmjs.com",
    "crates.io",
];

/// The domains a project deploys to, each with its line.
fn deployed_domains(project: &ProjectFiles) -> Vec<(String, String, Line<'_>)> {
    let mut domains = Vec::new();
    for (file, text) in &project.files {
        let lines = lines_of(text);
        match base_name(file) {
            "CNAME" => {
                for &line in &lines {
                    let host = line
                        .1
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if !host.starts_with('#') && is_domain(&host) {
                        domains.push((host, file.clone(), line));
                    }
                }
            }
            "vercel.json" => {
                let mut inside = false;
                for &line in &lines {
                    let text = line.1;
                    if text.trim_start().starts_with("\"alias\"") {
                        inside = !text.contains(']');
                    } else if !inside {
                        continue;
                    } else if text.contains(']') {
                        inside = false;
                    }
                    for host in quoted_strings(text) {
                        let host = host.to_ascii_lowercase();
                        if is_domain(&host) {
                            domains.push((host, file.clone(), line));
                        }
                    }
                }
            }
            "netlify.toml" => {
                for &line in &lines {
                    if let Some((key, value)) = key_value(line.1, '=')
                        && (key == "custom_domain" || key == "domain")
                        && is_domain(&value.to_ascii_lowercase())
                    {
                        domains.push((value.to_ascii_lowercase(), file.clone(), line));
                    }
                }
            }
            "package.json" => {
                for &line in &lines {
                    if let Some(("homepage", value)) = json_pair(line.1)
                        && let Some((_, rest)) = value.split_once("://")
                    {
                        let host = rest
                            .split(['/', ':', '?', '#'])
                            .next()
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        if is_domain(&host) && !SHARED_HOSTS.contains(&host.as_str()) {
                            domains.push((host, file.clone(), line));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    domains
}

fn is_config_file(path: &str) -> bool {
    let name = base_name(path);
    matches!(
        name,
        "package.json" | "Cargo.toml" | "go.mod" | ".gitmodules" | "vercel.json" | "netlify.toml"
    ) || is_compose(path)
        || is_workflow(path)
        || is_cluster_file(path)
        || has_extension(name, &["tf"])
}

fn domains(cx: &Context<'_>, found: &mut Vec<Proof>) {
    let deployed: Vec<_> = cx.projects.iter().map(deployed_domains).collect();
    for (b, theirs) in deployed.iter().enumerate() {
        for (domain, their_file, their_line) in theirs {
            for (a, project) in cx.projects.iter().enumerate() {
                if a == b || deployed[a].iter().any(|(own, ..)| own == domain) {
                    continue;
                }
                for (file, text) in &project.files {
                    if !is_config_file(file) {
                        continue;
                    }
                    if let Some(line) = lines_of(text)
                        .into_iter()
                        .find(|line| mentions(line.1, domain, false))
                    {
                        let end = ProofEnd {
                            project: cx.projects[b].key.clone(),
                            file: their_file.clone(),
                            line: their_line.0,
                            text: their_line.1.trim().chars().take(TEXT_LIMIT).collect(),
                        };
                        found.push(cx.proof(
                            ProofKind::Domain,
                            a,
                            b,
                            vec![end_of(&project.key, file, line), end],
                        ));
                        break;
                    }
                }
            }
        }
    }
}

fn claude_ai_names(
    projects: &[ProjectFiles],
    claude_ai: &[ClaudeAiProjectText],
    found: &mut Vec<Proof>,
) {
    for text in claude_ai {
        for project in projects {
            let Some(remote) = project.remote_key.as_deref() else {
                continue;
            };
            let Some((_, owner_name)) = remote.split_once('/') else {
                continue;
            };
            let needles = [remote, owner_name];
            let described = text
                .description
                .as_deref()
                .map(lines_of)
                .unwrap_or_default();
            let named: Vec<Line<'_>> = text
                .file_names
                .iter()
                .map(|name| (1, name.as_str()))
                .collect();
            let hit = described
                .iter()
                .map(|line| ("description", *line))
                .chain(named.iter().map(|line| (line.1, *line)))
                .find(|(_, line)| needles.iter().any(|needle| mentions(line.1, needle, true)));
            if let Some((file, line)) = hit {
                found.push(Proof {
                    kind: ProofKind::ClaudeAiNames,
                    a: text.uuid.clone(),
                    b: project.key.clone(),
                    ends: vec![end_of(&text.uuid, file, line)],
                    sentence: sentence(ProofKind::ClaudeAiNames, &text.name, &project.name),
                });
            }
        }
    }
}

/// Every proof among `projects` and `claude_ai`, in the order of the kinds, then of the keys. A
/// pair proved by the same kind in several places is proved once, by the first.
#[must_use]
pub fn find_proofs(projects: &[ProjectFiles], claude_ai: &[ClaudeAiProjectText]) -> Vec<Proof> {
    let cx = Context { projects };
    let mut found = Vec::new();
    for a in 0..projects.len() {
        path_dependencies(&cx, a, &mut found);
        submodules(&cx, a, &mut found);
        deploys(&cx, a, &mut found);
        workflows(&cx, a, &mut found);
    }
    declared_packages(&cx, &mut found);
    domains(&cx, &mut found);
    claude_ai_names(projects, claude_ai, &mut found);
    let mut seen = BTreeSet::new();
    found.retain(|proof| seen.insert((proof.kind, proof.a.clone(), proof.b.clone())));
    found.sort_by(|x, y| (x.kind, &x.a, &x.b).cmp(&(y.kind, &y.a, &y.b)));
    found
}

/// Whether `now` still holds `earlier`: a proof of the same kind between the same two projects,
/// read from the same files.
#[must_use]
pub fn still_holds(now: &[Proof], earlier: &Proof) -> bool {
    let files = |proof: &Proof| -> Vec<(String, String)> {
        proof
            .ends
            .iter()
            .map(|end| (end.project.clone(), end.file.clone()))
            .collect()
    };
    now.iter().any(|proof| {
        proof.kind == earlier.kind
            && proof.a == earlier.a
            && proof.b == earlier.b
            && files(proof) == files(earlier)
    })
}

/// What is known of a project for the hints, which are only suggestions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHintInput {
    /// The project's key.
    pub key: String,
    /// The name shown.
    pub name: String,
    /// The repository's normalised remote, when it has one.
    pub remote_key: Option<String>,
    /// The days (UTC) with sessions in it.
    pub active_days: BTreeSet<NaiveDate>,
    /// The sessions that wrote a file under its root.
    pub written_in: BTreeSet<String>,
}

/// Why two projects might be related without proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HintKind {
    /// The same remote owner.
    SameOwner,
    /// At least 3 days with sessions in both, within 30 days.
    SameDays,
    /// A shared word of at least 4 letters in their names.
    SimilarNames,
    /// A session wrote files under both.
    WroteBoth,
}

/// Two projects with a hint, `a` before `b` by key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    /// The lesser key.
    pub a: String,
    /// The greater key.
    pub b: String,
    /// Why, in the order of `HintKind`.
    pub kinds: Vec<HintKind>,
}

/// The words of a name at least four letters long, lower case: names split at `-` and `_` and
/// where a capital follows a small letter.
fn name_words(name: &str) -> BTreeSet<String> {
    let mut words = BTreeSet::new();
    let mut word = String::new();
    let mut last: Option<char> = None;
    let mut finish = |word: &mut String| {
        if word.chars().count() >= 4 && word.chars().all(char::is_alphabetic) {
            words.insert(word.to_lowercase());
        }
        word.clear();
    };
    for c in name.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && last.is_some_and(char::is_lowercase) {
                finish(&mut word);
            }
            word.push(c);
        } else {
            finish(&mut word);
        }
        last = Some(c);
    }
    finish(&mut word);
    words
}

fn owner_of(remote: Option<&str>) -> Option<(&str, &str)> {
    let mut parts = remote?.split('/');
    let (host, owner) = (parts.next()?, parts.next()?);
    parts.next()?;
    Some((host, owner))
}

/// Whether three of `days` fall within 30 days.
fn has_three_within_a_month(days: &BTreeSet<NaiveDate>) -> bool {
    let days: Vec<&NaiveDate> = days.iter().collect();
    days.windows(3)
        .any(|three| (*three[2] - *three[0]).num_days() <= 30)
}

/// The hints among `inputs` for pairs that `proofs` do not prove.
#[must_use]
pub fn find_hints(inputs: &[ProjectHintInput], proofs: &[Proof]) -> Vec<Hint> {
    let proven: BTreeSet<(&str, &str)> = proofs
        .iter()
        .map(|proof| {
            if proof.a < proof.b {
                (proof.a.as_str(), proof.b.as_str())
            } else {
                (proof.b.as_str(), proof.a.as_str())
            }
        })
        .collect();
    let mut hints = Vec::new();
    for (at, x) in inputs.iter().enumerate() {
        for y in &inputs[at + 1..] {
            let (a, b) = if x.key < y.key { (x, y) } else { (y, x) };
            if a.key == b.key || proven.contains(&(a.key.as_str(), b.key.as_str())) {
                continue;
            }
            let mut kinds = Vec::new();
            if owner_of(a.remote_key.as_deref())
                .is_some_and(|owner| owner_of(b.remote_key.as_deref()) == Some(owner))
            {
                kinds.push(HintKind::SameOwner);
            }
            let shared: BTreeSet<NaiveDate> = a
                .active_days
                .intersection(&b.active_days)
                .copied()
                .collect();
            if has_three_within_a_month(&shared) {
                kinds.push(HintKind::SameDays);
            }
            if name_words(&a.name)
                .intersection(&name_words(&b.name))
                .next()
                .is_some()
            {
                kinds.push(HintKind::SimilarNames);
            }
            if a.written_in.intersection(&b.written_in).next().is_some() {
                kinds.push(HintKind::WroteBoth);
            }
            if !kinds.is_empty() {
                hints.push(Hint {
                    a: a.key.clone(),
                    b: b.key.clone(),
                    kinds,
                });
            }
        }
    }
    hints.sort_by(|x, y| (&x.a, &x.b).cmp(&(&y.a, &y.b)));
    hints
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::NaiveDate;

    use super::fixtures::tempting_corpus;
    use super::*;

    fn pair_of(a: &str, b: &str) -> (String, String) {
        if a < b {
            (a.to_string(), b.to_string())
        } else {
            (b.to_string(), a.to_string())
        }
    }

    #[test]
    fn proves_each_related_pair_and_no_tempting_one() {
        let corpus = tempting_corpus();
        let proofs = find_proofs(&corpus.projects, &corpus.claude_ai);
        let mut failures: Vec<String> = Vec::new();

        for (kind, a, b) in &corpus.related {
            let (a_key, b_key) = (corpus.key_of(a), corpus.key_of(b));
            if !proofs
                .iter()
                .any(|proof| proof.kind == *kind && proof.a == a_key && proof.b == b_key)
            {
                failures.push(format!("{a} and {b} give no {kind:?} proof"));
            }
        }
        for (a, b, temptation) in &corpus.unrelated {
            let pair = pair_of(&corpus.key_of(a), &corpus.key_of(b));
            for proof in &proofs {
                if pair_of(&proof.a, &proof.b) == pair {
                    failures.push(format!(
                        "{a} and {b} ({temptation}) give a {:?} proof: {}",
                        proof.kind, proof.sentence
                    ));
                }
            }
        }
        // Nothing else is proved either: only the related pairs, and nothing across them.
        let related: BTreeSet<(String, String)> = corpus
            .related
            .iter()
            .map(|(_, a, b)| pair_of(&corpus.key_of(a), &corpus.key_of(b)))
            .collect();
        for proof in &proofs {
            if !related.contains(&pair_of(&proof.a, &proof.b)) {
                failures.push(format!("a proof outside the corpus: {}", proof.sentence));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one corpus read once, then each kind's proof checked against it"
    )]
    fn finds_each_proof_s_ends() {
        let corpus = tempting_corpus();
        let proofs = find_proofs(&corpus.projects, &corpus.claude_ai);
        let (web, api) = (corpus.key_of("tidepool-web"), corpus.key_of("tidepool-api"));
        let proof = proofs
            .iter()
            .find(|proof| proof.kind == ProofKind::PathDependency && proof.a == web)
            .expect("the path dependency is proved");
        assert_eq!(proof.b, api);
        assert_eq!(proof.sentence, "tidepool-web uses tidepool-api's code.");
        assert_eq!(
            proof.ends,
            vec![
                ProofEnd {
                    project: web,
                    file: "web/package.json".to_string(),
                    line: 14,
                    text: "\"@tidepool/client\": \"file:../../tidepool-api/api/client\""
                        .to_string(),
                },
                ProofEnd {
                    project: api,
                    file: "api/client/package.json".to_string(),
                    line: 2,
                    text: "\"name\": \"@tidepool/client\",".to_string(),
                },
            ]
        );

        // The image one deploys through is the image the other's workflow publishes, in the step
        // that builds it and not in the trigger's `tags:`.
        let (cluster, relay) = (
            corpus.key_of("cluster-config"),
            corpus.key_of("relay-worker"),
        );
        let deploys = proofs
            .iter()
            .find(|proof| proof.kind == ProofKind::Deploys && proof.a == cluster)
            .expect("the image is proved");
        assert_eq!(deploys.b, relay);
        assert_eq!(deploys.sentence, "cluster-config deploys relay-worker.");
        let ends: Vec<(&str, usize)> = deploys
            .ends
            .iter()
            .map(|end| (end.file.as_str(), end.line))
            .collect();
        assert_eq!(
            ends,
            vec![
                ("k8s/relay.yaml", 10),
                (".github/workflows/publish.yml", 13)
            ]
        );

        // The kinds the corpus holds once each, by their sentences.
        let sentence = |kind, a: &str| {
            proofs
                .iter()
                .find(|proof| proof.kind == kind && proof.a == corpus.key_of(a))
                .map(|proof| proof.sentence.as_str())
        };
        assert_eq!(
            sentence(ProofKind::Submodule, "harbor-site"),
            Some("harbor-site includes harbor-theme.")
        );
        assert_eq!(
            sentence(ProofKind::DeclaredPackage, "ledger-app"),
            Some("ledger-app uses ledger-money's package.")
        );
        assert_eq!(
            sentence(ProofKind::Workflow, "release-bot"),
            Some("release-bot's workflow works on docs-hub.")
        );
        assert_eq!(
            sentence(ProofKind::Domain, "pixel-api"),
            Some("pixel-api talks to pixel-garden's site.")
        );
        assert_eq!(
            sentence(ProofKind::ClaudeAiNames, "Launch notes"),
            Some("Your claude.ai Project Launch notes is about orbit-cli.")
        );

        // A Terraform module whose source is the other's repository, and a workflow that
        // dispatches to it, are proofs too.
        let project = |name: &str, root: &str, remote: &str, files: &[(&str, &str)]| ProjectFiles {
            key: remote.to_string(),
            name: name.to_string(),
            remote_key: Some(remote.to_string()),
            root: Some(root.to_string()),
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
                .collect(),
        };
        let infra = project(
            "infra",
            "/work/infra",
            "github.com/net-co/infra",
            &[
                (
                    "main.tf",
                    "module \"net\" {\n  source = \"git::https://github.com/net-co/network.git//vpc?ref=v2\"\n}\n",
                ),
                (
                    ".github/workflows/ping.yml",
                    "jobs:\n  ping:\n    steps:\n      - run: gh api repos/net-co/status-page/dispatches -f event_type=ping\n",
                ),
            ],
        );
        let network = project("network", "/work/network", "github.com/net-co/network", &[]);
        let status = project(
            "status-page",
            "/work/status-page",
            "github.com/net-co/status-page",
            &[],
        );
        let found = find_proofs(&[infra, network, status], &[]);
        let kinds: Vec<(ProofKind, &str)> = found
            .iter()
            .map(|proof| (proof.kind, proof.b.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (ProofKind::Deploys, "github.com/net-co/network"),
                (ProofKind::Workflow, "github.com/net-co/status-page"),
            ]
        );
    }

    #[test]
    fn hints_without_proof() {
        let corpus = tempting_corpus();
        let proofs = find_proofs(&corpus.projects, &corpus.claude_ai);
        let day = |n: u32| NaiveDate::from_ymd_opt(2026, 10, n).expect("a day");
        let input = |name: &str, days: &[u32], written: &[&str]| {
            let project = corpus.project(name);
            ProjectHintInput {
                key: project.key.clone(),
                name: project.name.clone(),
                remote_key: project.remote_key.clone(),
                active_days: days.iter().map(|n| day(*n)).collect(),
                written_in: written.iter().map(|id| (*id).to_string()).collect(),
            }
        };
        let inputs = vec![
            // Three shared days within a month; zeta-cli and alpha-site share an owner only.
            input("mint-notes", &[2, 9, 16, 20], &[]),
            input("quartz-tools", &[9, 16, 20, 21], &[]),
            input("alpha-site", &[1], &[]),
            input("zeta-cli", &[30], &[]),
            input("price-watch", &[], &[]),
            input("watch-tower", &[], &[]),
            // Two shared days are not enough, a session that wrote in both is a hint.
            input("river-blog", &[3, 4], &[]),
            input("comet-lab", &[3, 4], &[]),
            input("dune-app", &[], &["s-1", "s-2"]),
            input("fern-app", &[], &["s-2"]),
            // The same owner, days and names as a proof pair give no hint.
            input("tidepool-web", &[1, 2, 3, 4], &["s-9"]),
            input("tidepool-api", &[1, 2, 3, 4], &["s-9"]),
        ];
        let hints = find_hints(&inputs, &proofs);
        let kinds_of = |a: &str, b: &str| {
            let pair = pair_of(&corpus.key_of(a), &corpus.key_of(b));
            hints
                .iter()
                .find(|hint| (hint.a.clone(), hint.b.clone()) == pair)
                .map(|hint| hint.kinds.clone())
        };
        assert_eq!(
            kinds_of("mint-notes", "quartz-tools"),
            Some(vec![HintKind::SameDays])
        );
        assert_eq!(
            kinds_of("alpha-site", "zeta-cli"),
            Some(vec![HintKind::SameOwner])
        );
        assert_eq!(
            kinds_of("price-watch", "watch-tower"),
            Some(vec![HintKind::SimilarNames])
        );
        assert_eq!(
            kinds_of("dune-app", "fern-app"),
            Some(vec![HintKind::WroteBoth])
        );
        assert_eq!(kinds_of("river-blog", "comet-lab"), None, "two days");
        assert_eq!(
            kinds_of("tidepool-web", "tidepool-api"),
            None,
            "a proof pair is not a hint pair"
        );
        assert_eq!(hints.len(), 4, "{hints:?}");
    }
}
