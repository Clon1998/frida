//! Environment files in `<dir>/environments/*.yaml` (PLAN.md §4.4). `dir` is the workspace
//! folder for global environments, or a collection folder for collection environments.
//! Environments are identified by their `name`; the file name is its slug.

use std::path::{Path, PathBuf};

use super::paths::{ENVIRONMENTS_DIR, EntryKind, YAML_EXT, unique_name};
use crate::error::{CoreError, Result};
use crate::model::Environment;
use crate::yaml::{read_yaml, write_yaml};

/// All readable environments in `dir`, sorted by name. Broken files are skipped with a warning,
/// so one bad file doesn't hide the others.
pub fn list_environments(dir: &Path) -> Result<Vec<Environment>> {
    let mut environments: Vec<Environment> = env_files(dir)?
        .iter()
        .filter_map(|path| match read_yaml::<Environment>(path) {
            Ok(env) => Some(env),
            Err(err) => {
                tracing::warn!(%err, "skipping unreadable environment file");
                None
            }
        })
        .collect();
    environments.sort_by_key(|env| env.name.to_lowercase());
    Ok(environments)
}

/// The environment called `name`, or `None` if there is none.
pub fn find_environment(dir: &Path, name: &str) -> Result<Option<Environment>> {
    Ok(list_environments(dir)?
        .into_iter()
        .find(|env| env.name == name))
}

/// Creates an empty environment. Fails if one with that name exists.
pub fn create_environment(dir: &Path, name: &str) -> Result<Environment> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput(
            "the environment name must not be empty".into(),
        ));
    }
    if find_environment(dir, name)?.is_some() {
        return Err(CoreError::InvalidInput(format!(
            "an environment called {name:?} already exists"
        )));
    }
    let env = Environment::new(name);
    let envs_dir = dir.join(ENVIRONMENTS_DIR);
    let file = envs_dir.join(unique_name(&envs_dir, name, EntryKind::Request, None));
    write_yaml(&file, &env.without_secret_values())?;
    Ok(env)
}

/// Copies the environment `name` as "<name> copy" (or "<name> copy 2", ...). Returns the copy.
/// Secret values live outside the file; the caller copies them (`secrets::copy_env_secrets`).
pub fn duplicate_environment(dir: &Path, name: &str) -> Result<Environment> {
    let Some(original) = find_environment(dir, name)? else {
        return Err(CoreError::NotFound {
            path: dir.join(ENVIRONMENTS_DIR).join(name),
        });
    };
    let mut copy_name = format!("{name} copy");
    let mut n = 2;
    while find_environment(dir, &copy_name)?.is_some() {
        copy_name = format!("{name} copy {n}");
        n += 1;
    }
    let created = create_environment(dir, &copy_name)?;
    let copy = Environment {
        name: created.name,
        ..original
    };
    save_environment(dir, &copy.name, &copy)?;
    Ok(copy)
}

/// Saves `env`, which was loaded as `original_name`. A changed name renames the file.
/// Secret values are always stripped before writing (PLAN.md §6).
pub fn save_environment(dir: &Path, original_name: &str, env: &Environment) -> Result<()> {
    let name = env.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput(
            "the environment name must not be empty".into(),
        ));
    }
    let old_file = file_of(dir, original_name)?;
    if name != original_name && find_environment(dir, name)?.is_some() {
        return Err(CoreError::InvalidInput(format!(
            "an environment called {name:?} already exists"
        )));
    }

    let envs_dir = dir.join(ENVIRONMENTS_DIR);
    let new_file = envs_dir.join(unique_name(
        &envs_dir,
        name,
        EntryKind::Request,
        Some(&old_file),
    ));
    write_yaml(&new_file, &env.without_secret_values())?;
    if new_file != old_file {
        std::fs::remove_file(&old_file).map_err(|e| CoreError::io(&old_file, e))?;
    }
    Ok(())
}

pub fn delete_environment(dir: &Path, name: &str) -> Result<()> {
    let file = file_of(dir, name)?;
    std::fs::remove_file(&file).map_err(|e| CoreError::io(&file, e))
}

/// The file holding the environment called `name`.
fn file_of(dir: &Path, name: &str) -> Result<PathBuf> {
    env_files(dir)?
        .into_iter()
        .find(|path| read_yaml::<Environment>(path).is_ok_and(|env| env.name == name))
        .ok_or_else(|| CoreError::NotFound {
            path: dir
                .join(ENVIRONMENTS_DIR)
                .join(format!("{name}.{YAML_EXT}")),
        })
}

fn env_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let envs_dir = dir.join(ENVIRONMENTS_DIR);
    if !envs_dir.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&envs_dir).map_err(|e| CoreError::io(&envs_dir, e))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| CoreError::io(&envs_dir, e))?.path();
        if path.extension().is_some_and(|ext| ext == YAML_EXT) {
            files.push(path);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EnvVariable;

    fn var(name: &str, value: Option<&str>, secret: bool) -> EnvVariable {
        EnvVariable {
            name: name.into(),
            value: value.map(Into::into),
            secret,
            source: None,
        }
    }

    #[test]
    fn create_list_save_rename_delete() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        create_environment(root, "Dev").unwrap();
        create_environment(root, "prod").unwrap();
        assert!(create_environment(root, "Dev").is_err());
        assert!(root.join("environments/dev.yaml").is_file());

        let names: Vec<String> = list_environments(root)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["Dev", "prod"]);

        let mut dev = find_environment(root, "Dev").unwrap().unwrap();
        dev.name = "Development".into();
        dev.variables = vec![var("baseUrl", Some("http://localhost"), false)];
        save_environment(root, "Dev", &dev).unwrap();
        assert!(!root.join("environments/dev.yaml").exists());
        assert!(root.join("environments/development.yaml").is_file());
        assert_eq!(
            find_environment(root, "Development")
                .unwrap()
                .unwrap()
                .variables
                .len(),
            1
        );

        // Renaming onto an existing name is refused.
        let mut clash = find_environment(root, "Development").unwrap().unwrap();
        clash.name = "prod".into();
        assert!(save_environment(root, "Development", &clash).is_err());

        delete_environment(root, "prod").unwrap();
        assert_eq!(list_environments(root).unwrap().len(), 1);
    }

    #[test]
    fn secret_values_are_never_written() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut env = create_environment(root, "dev").unwrap();
        env.variables = vec![var("token", Some("super-secret-value"), true)];
        save_environment(root, "dev", &env).unwrap();

        let text = std::fs::read_to_string(root.join("environments/dev.yaml")).unwrap();
        assert!(!text.contains("super-secret-value"), "{text}");
        assert!(text.contains("secret: true"));
    }

    #[test]
    fn missing_environments_folder_means_no_environments() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_environments(dir.path()).unwrap().is_empty());
    }
}

#[cfg(test)]
mod duplicate_tests {
    use super::*;

    #[test]
    fn duplicates_get_unique_names_and_the_same_variables() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(ENVIRONMENTS_DIR)).unwrap();
        let mut env = create_environment(dir.path(), "dev").unwrap();
        env.color = Some("red".into());
        env.variables.push(crate::model::EnvVariable {
            name: "host".into(),
            value: Some("x".into()),
            secret: false,
            source: None,
        });
        save_environment(dir.path(), "dev", &env).unwrap();

        let first = duplicate_environment(dir.path(), "dev").unwrap();
        let second = duplicate_environment(dir.path(), "dev").unwrap();
        assert_eq!(
            (first.name.as_str(), second.name.as_str()),
            ("dev copy", "dev copy 2")
        );
        let loaded = find_environment(dir.path(), "dev copy").unwrap().unwrap();
        assert_eq!(loaded.variables, env.variables);
        assert_eq!(loaded.color.as_deref(), Some("red"));
    }
}
