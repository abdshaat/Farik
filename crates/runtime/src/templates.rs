//! Saved team templates (ADR 0026 C): `<state folder>/templates/<slug>.yaml`, kept on this machine
//! and outside every project, so saving, renaming or deleting one records no project event.

use std::fmt;
use std::io::ErrorKind;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::PathBuf;

use farik_core::contract::ValidationError;
use farik_core::team::{TeamTemplate, template_slug, validate_template};
use farik_store::files::{template_yaml, yaml_value};

/// The folder saved templates are kept in, one file per template, named by its slug.
pub struct Templates {
    dir: PathBuf,
}

/// Why a template could not be saved, read, renamed or deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// A template with this slug is saved already, under this name.
    Exists {
        /// The saved template's name.
        name: String,
    },
    /// The name is not one a template may have: blank, too long, or with nothing to slug.
    Name,
    /// No template is saved under this slug.
    NotFound {
        /// The slug asked for.
        slug: String,
    },
    /// The file is there and is not a template.
    Unreadable {
        /// Its slug.
        slug: String,
    },
    /// The operating system refused.
    Io {
        /// What it said.
        detail: String,
    },
}

impl fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exists { name } => write!(
                formatter,
                "A saved team is called {name} already. Replace it, or choose another name."
            ),
            Self::Name => write!(
                formatter,
                "A saved team needs a name with a letter or digit."
            ),
            Self::NotFound { slug } => write!(formatter, "There is no saved team {slug}."),
            Self::Unreadable { slug } => write!(formatter, "The saved team {slug} cannot be read."),
            Self::Io { detail } => write!(formatter, "Saved teams could not be used: {detail}"),
        }
    }
}

impl std::error::Error for TemplateError {}

/// `template_from_team`'s refusals. For a team `validate_team` accepts, the only thing it can
/// refuse is the name, at `/name`; so every refusal is `Name`.
impl From<Vec<ValidationError>> for TemplateError {
    fn from(_: Vec<ValidationError>) -> Self {
        Self::Name
    }
}

/// What the folder holds: each readable template with its slug, by name, case-insensitive; and the
/// slug of each file that is not a template.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateListing {
    /// `(slug, template)`.
    pub templates: Vec<(String, TeamTemplate)>,
    /// Slugs.
    pub unreadable: Vec<String>,
}

impl Templates {
    /// The templates kept in `dir`, the state folder's `templates/`. Nothing is made until a save.
    #[must_use]
    pub fn new(dir: PathBuf) -> Templates {
        Templates { dir }
    }

    /// The folder the templates are kept in.
    #[must_use]
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Every template saved, and every file that is not one.
    ///
    /// # Errors
    ///
    /// `Io` when the folder cannot be read. A folder that is not there yet holds nothing.
    pub fn list(&self) -> Result<TemplateListing, TemplateError> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(TemplateListing {
                    templates: Vec::new(),
                    unreadable: Vec::new(),
                });
            }
            Err(error) => return Err(io(&error)),
        };
        let mut listing = TemplateListing {
            templates: Vec::new(),
            unreadable: Vec::new(),
        };
        for entry in entries {
            let file = entry.map_err(|error| io(&error))?.file_name();
            // ponytail: a file whose stem is not a slug cannot be named on the wire, so it is not listed.
            let Some(slug) = file.to_str().and_then(|name| name.strip_suffix(".yaml")) else {
                continue;
            };
            if !is_slug(slug) {
                continue;
            }
            match self.read(slug) {
                Ok(template) => listing.templates.push((slug.to_string(), template)),
                Err(TemplateError::Unreadable { slug }) => listing.unreadable.push(slug),
                Err(TemplateError::NotFound { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        listing
            .templates
            .sort_by_key(|(slug, template)| (template.name.to_lowercase(), slug.clone()));
        listing.unreadable.sort();
        Ok(listing)
    }

    /// The template saved under `slug`.
    ///
    /// # Errors
    ///
    /// `NotFound` when there is none, or `slug` is not a slug; `Unreadable` when the file is not a
    /// template; `Io` when it cannot be read.
    pub fn read(&self, slug: &str) -> Result<TeamTemplate, TemplateError> {
        let path = self.path(slug)?;
        let text = std::fs::read_to_string(&path).map_err(|error| match error.kind() {
            ErrorKind::NotFound => TemplateError::NotFound {
                slug: slug.to_string(),
            },
            _ => io(&error),
        })?;
        let unreadable = || TemplateError::Unreadable {
            slug: slug.to_string(),
        };
        let value = yaml_value(&text, &format!("{slug}.yaml")).map_err(|_| unreadable())?;
        validate_template(&value).map_err(|_| unreadable())
    }

    /// Saves `template` under its name's slug, which it answers. The state folder and `templates/`
    /// are made 0700 when they are not there, and the file is written 0600, beside and then
    /// renamed into place, so a reader never sees half a file.
    ///
    /// # Errors
    ///
    /// `Name` when the name has no slug; `Exists` when the slug is taken and `replace` is false;
    /// `Io` when the folder or the file cannot be written.
    pub fn save(&self, template: &TeamTemplate, replace: bool) -> Result<String, TemplateError> {
        let slug = template_slug(&template.name).ok_or(TemplateError::Name)?;
        if !replace {
            self.refuse_taken(&slug, &template.name)?;
        }
        self.write(&slug, template)?;
        Ok(slug)
    }

    /// Gives the template under `slug` the name `name`, keeping its `saved_at`, and answers the new
    /// slug. A name that only changes case keeps the file.
    ///
    /// # Errors
    ///
    /// `read`'s; `Name` when the name is refused; `Exists` when another template holds the new slug.
    pub fn rename(&self, slug: &str, name: &str) -> Result<String, TemplateError> {
        let mut wire = serde_json::to_value(self.read(slug)?).map_err(|error| io(&error))?;
        wire["name"] = name.trim().into();
        let renamed = validate_template(&wire).map_err(TemplateError::from)?;
        let new_slug = template_slug(&renamed.name).ok_or(TemplateError::Name)?;
        if new_slug != slug {
            self.refuse_taken(&new_slug, &renamed.name)?;
        }
        self.write(&new_slug, &renamed)?;
        if new_slug != slug {
            std::fs::remove_file(self.path(slug)?).map_err(|error| io(&error))?;
        }
        Ok(new_slug)
    }

    /// Removes the file under `slug`, readable or not.
    ///
    /// # Errors
    ///
    /// `NotFound` when there is none, or `slug` is not a slug; `Io` when it cannot be removed.
    pub fn delete(&self, slug: &str) -> Result<(), TemplateError> {
        std::fs::remove_file(self.path(slug)?).map_err(|error| match error.kind() {
            ErrorKind::NotFound => TemplateError::NotFound {
                slug: slug.to_string(),
            },
            _ => io(&error),
        })
    }

    /// The file `slug` names. A slug is what `template_slug` makes, so anything else, `..` or a
    /// path among them, names no template and cannot climb out of the folder.
    fn path(&self, slug: &str) -> Result<PathBuf, TemplateError> {
        if is_slug(slug) {
            Ok(self.dir.join(format!("{slug}.yaml")))
        } else {
            Err(TemplateError::NotFound {
                slug: slug.to_string(),
            })
        }
    }

    /// `Exists`, naming the template saved under `slug`, when there is one; its file's own name
    /// when it cannot be read, so the refusal still says something.
    fn refuse_taken(&self, slug: &str, name: &str) -> Result<(), TemplateError> {
        match self.read(slug) {
            Err(TemplateError::NotFound { .. }) => Ok(()),
            Ok(saved) => Err(TemplateError::Exists {
                name: saved.name.to_string(),
            }),
            Err(TemplateError::Unreadable { .. }) => Err(TemplateError::Exists {
                name: name.to_string(),
            }),
            Err(error) => Err(error),
        }
    }

    /// Writes `template` under `slug`: the folders made 0700 where they are not there, the text
    /// written 0600 beside the file and renamed over it.
    fn write(&self, slug: &str, template: &TeamTemplate) -> Result<(), TemplateError> {
        let text = template_yaml(template).map_err(|error| TemplateError::Io {
            detail: error.to_string(),
        })?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)
            .map_err(|error| io(&error))?;
        let beside = self.dir.join(format!(".{slug}.yaml.saving"));
        crate::write_private(&beside, text.as_bytes())
            .and_then(|()| std::fs::rename(&beside, self.dir.join(format!("{slug}.yaml"))))
            .map_err(|error| {
                let _ = std::fs::remove_file(&beside);
                io(&error)
            })
    }
}

/// Whether `text` is a slug `template_slug` could have made: itself, slugged.
fn is_slug(text: &str) -> bool {
    template_slug(text).as_deref() == Some(text)
}

fn io(error: &dyn std::error::Error) -> TemplateError {
    TemplateError::Io {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};

    use farik_core::team::fixtures::a_template_wire;
    use farik_core::team::{TeamTemplate, validate_template};

    use super::{TemplateError, Templates};

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("farik-templates-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        dir
    }

    fn named(name: &str) -> TeamTemplate {
        let mut wire = a_template_wire();
        wire["name"] = name.into();
        validate_template(&wire).expect("the fixture is a template")
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("it is there")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn saves_privately() {
        let state = scratch("private").join("farik");
        let templates = Templates::new(state.join("templates"));
        let template = named("My usual team");
        assert_eq!(
            templates.save(&template, false),
            Ok("my-usual-team".to_string())
        );
        assert_eq!(mode(&state), 0o700, "the state folder");
        assert_eq!(mode(&state.join("templates")), 0o700, "templates/");
        let file = state.join("templates/my-usual-team.yaml");
        assert_eq!(mode(&file), 0o600, "the file");
        assert_eq!(templates.read("my-usual-team"), Ok(template));
        let left: Vec<_> = std::fs::read_dir(state.join("templates"))
            .expect("listed")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        assert_eq!(
            left,
            vec!["my-usual-team.yaml"],
            "nothing is left beside it"
        );
    }

    #[test]
    fn refuses_a_taken_name_unless_replacing() {
        let templates = Templates::new(scratch("taken").join("templates"));
        templates
            .save(&named("My usual team"), false)
            .expect("saved");
        let again = named("my usual  TEAM!");
        assert_eq!(
            templates.save(&again, false),
            Err(TemplateError::Exists {
                name: "My usual team".to_string()
            })
        );
        assert_eq!(
            templates.read("my-usual-team").map(|t| t.name),
            Ok(named("My usual team").name)
        );
        assert_eq!(
            templates.save(&again, true),
            Ok("my-usual-team".to_string())
        );
        assert_eq!(templates.read("my-usual-team"), Ok(again));
    }

    #[test]
    fn refuses_a_name_with_no_slug() {
        let dir = scratch("no-slug").join("templates");
        let templates = Templates::new(dir.clone());
        assert_eq!(
            templates.save(&named("!!!"), false),
            Err(TemplateError::Name)
        );
        assert!(!dir.exists(), "nothing is made");
        templates.save(&named("Pair"), false).expect("saved");
        assert_eq!(templates.rename("pair", "  ?? "), Err(TemplateError::Name));
        assert!(dir.join("pair.yaml").exists());
    }

    #[test]
    fn maps_a_refused_name_from_the_team() {
        let team = farik_core::team::validate_team(&farik_core::team::fixtures::a_team_wire())
            .expect("a team");
        let at = "2026-09-30T12:00:00Z".parse().expect("a date-time");
        let refused = farik_core::team::template_from_team(&team, &"a".repeat(61), at)
            .map_err(TemplateError::from);
        assert_eq!(refused.err(), Some(TemplateError::Name));
    }

    #[test]
    fn refuses_a_path_through_a_name_or_a_slug() {
        let root = scratch("traversal");
        let dir = root.join("farik/templates");
        let templates = Templates::new(dir.clone());
        assert_eq!(
            templates.save(&named("../../outside"), false),
            Ok("outside".to_string())
        );
        assert!(dir.join("outside.yaml").exists());
        assert!(!root.join("outside.yaml").exists());
        std::fs::write(root.join("farik/kept.yaml"), "kept").expect("written");
        for slug in ["../kept", "..", "", "a/b", "/etc/passwd", "Outside"] {
            let missing = Err(TemplateError::NotFound {
                slug: slug.to_string(),
            });
            assert_eq!(templates.read(slug).map(|_| ()), missing, "read {slug}");
            assert_eq!(templates.delete(slug), missing, "delete {slug}");
            assert_eq!(
                templates.rename(slug, "x").map(|_| ()),
                missing,
                "rename {slug}"
            );
        }
        assert!(
            root.join("farik/kept.yaml").exists(),
            "nothing outside is touched"
        );
        assert_eq!(
            templates.rename("outside", "../../kept"),
            Ok("kept".to_string())
        );
        assert!(dir.join("kept.yaml").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("farik/kept.yaml")).ok(),
            Some("kept".into())
        );
    }

    #[test]
    fn lists_by_name_and_names_unreadable_files() {
        let dir = scratch("list").join("templates");
        let templates = Templates::new(dir.clone());
        assert_eq!(
            templates.list().map(|l| l.templates.len()),
            Ok(0),
            "no folder yet"
        );
        for name in ["beta", "alpha", "Gamma"] {
            templates.save(&named(name), false).expect("saved");
        }
        std::fs::write(dir.join("broken.yaml"), "{").expect("written");
        std::fs::write(dir.join("half.yaml"), "name: Half\n").expect("written");
        std::fs::write(dir.join("notes.txt"), "not a template").expect("written");
        let listing = templates.list().expect("listed");
        let names: Vec<(&str, &str)> = listing
            .templates
            .iter()
            .map(|(slug, template)| (slug.as_str(), template.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![("alpha", "alpha"), ("beta", "beta"), ("gamma", "Gamma")]
        );
        assert_eq!(
            listing.unreadable,
            vec!["broken".to_string(), "half".to_string()]
        );
        assert_eq!(
            templates.read("broken"),
            Err(TemplateError::Unreadable {
                slug: "broken".to_string()
            })
        );
    }

    #[test]
    fn renames_and_deletes() {
        let dir = scratch("rename").join("templates");
        let templates = Templates::new(dir.clone());
        let pair = named("Pair");
        templates.save(&pair, false).expect("saved");
        templates.save(&named("Trio"), false).expect("saved");

        assert_eq!(
            templates.rename("pair", "  Two of us "),
            Ok("two-of-us".to_string())
        );
        assert!(!dir.join("pair.yaml").exists(), "the old file is gone");
        let renamed = templates.read("two-of-us").expect("read");
        assert_eq!(renamed.name.as_str(), "Two of us");
        assert_eq!(renamed.saved_at, pair.saved_at);

        assert_eq!(
            templates.rename("two-of-us", "TWO OF US"),
            Ok("two-of-us".to_string())
        );
        assert_eq!(
            templates.read("two-of-us").map(|t| t.name.to_string()),
            Ok("TWO OF US".into())
        );

        assert_eq!(
            templates.rename("two-of-us", "trio"),
            Err(TemplateError::Exists {
                name: "Trio".to_string()
            })
        );
        assert!(dir.join("two-of-us.yaml").exists());

        std::fs::write(dir.join("broken.yaml"), "{").expect("written");
        assert_eq!(templates.delete("broken"), Ok(()));
        assert_eq!(templates.delete("trio"), Ok(()));
        assert!(!dir.join("trio.yaml").exists());

        let missing = |slug: &str| TemplateError::NotFound {
            slug: slug.to_string(),
        };
        assert_eq!(templates.read("trio"), Err(missing("trio")));
        assert_eq!(templates.rename("trio", "x"), Err(missing("trio")));
        assert_eq!(templates.delete("trio"), Err(missing("trio")));
    }
}
