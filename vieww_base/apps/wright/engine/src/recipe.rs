//! The recipe gallery, CLI-facing (ADR-028): list, install, sign, verify.
//!
//! The pure format lives in `wright_core::recipe`; this module is where
//! the CLI meets the filesystem: which folders count as the gallery,
//! what install means (a fresh project folder from a verified
//! blueprint), and the refusal an unsigned recipe gets.
//!
//! The gallery is two folders, in order: `./recipes` (a checkout can
//! ship one) and `$WRIGHT_HOME/recipes` (the machine's own). An
//! installed recipe lands as a NEW project directory, never merged into
//! an existing one — a recipe is a beginning, not a transplant.

use std::path::{Path, PathBuf};

use wright_core::recipe::Recipe;

/// Where recipes live, in lookup order: the current folder's gallery
/// (a checkout ships one) and the machine's own.
pub fn gallery_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("recipes"),
        wright_runtime::config::wright_home().join("recipes"),
    ]
}

/// One gallery entry: the file and its (parsed) recipe.
#[derive(Debug, Clone)]
pub struct Listed {
    pub path: PathBuf,
    pub recipe: Recipe,
}

/// All gallery recipes that parse. Broken files are reported, not hidden.
pub fn list() -> (Vec<Listed>, Vec<String>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for dir in gallery_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".recipe.kdl") {
                continue;
            }
            let path = e.path();
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| Recipe::parse(&t))
            {
                Ok(r) => {
                    if seen.insert(r.id.clone()) {
                        out.push(Listed { path, recipe: r });
                    }
                }
                Err(e) => problems.push(format!("{}: {e}", path.display())),
            }
        }
    }
    out.sort_by(|a, b| a.recipe.id.cmp(&b.recipe.id));
    (out, problems)
}

/// Install a recipe file into a NEW project directory. Refuses before
/// touching anything if the recipe does not verify (ADR-028) or its
/// blueprint does not validate, and refuses an existing non-empty
/// directory — a recipe is a beginning, not a transplant.
pub fn install(file: &Path, target_dir: &Path) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let recipe = Recipe::parse(&text)?;
    recipe.verify()?;
    let bp = wright_core::blueprint::parse::blueprint(&recipe.blueprint)
        .map_err(|e| format!("the embedded blueprint does not parse: {e}"))?;
    wright_core::blueprint::validate::validate(&bp).map_err(|errs| {
        format!(
            "the embedded blueprint is not valid:\n  - {}",
            errs.join("\n  - ")
        )
    })?;
    let dir = target_dir.join(&bp.meta.name);
    if dir.exists() {
        return Err(format!("'{}' already exists; a recipe installs into a fresh folder, never over one that exists", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(dir.join("blueprint.kdl"), &recipe.blueprint)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// Sign a recipe file with the machine's publishing key.
pub fn sign(file: &Path) -> Result<(), String> {
    let key = wright_runtime::signing::SigningKey::resolve()?;
    let seed = key.seed_for_signing();
    wright_core::recipe::sign_file(file, &seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wright-gallery-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const BLUEPRINT: &str = "meta { name \"Weekly demo\" }\nintent { summary \"weekly\" confirmed \"v1\" }\nagent Reporter { goal \"report\" out \"report.md\" }\ndeliverable kind=\"report\" { check \"one page\" }\n";

    #[test]
    fn the_gallery_lists_only_parsable_recipes_and_names_the_broken() {
        let dir = tmp("list");
        let recipes = dir.join("recipes");
        std::fs::create_dir_all(&recipes).unwrap();
        let good = wright_core::recipe::Recipe {
            id: "weekly".into(),
            version: "1.0".into(),
            publisher: "test".into(),
            description: "A weekly report.".into(),
            blueprint: BLUEPRINT.into(),
            publisher_key: String::new(),
            signature: None,
        };
        std::fs::write(recipes.join("weekly.recipe.kdl"), good.to_kdl()).unwrap();
        std::fs::write(recipes.join("broken.recipe.kdl"), "recipe { }").unwrap();
        std::fs::write(recipes.join("not-a-recipe.txt"), "hello").unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let (listed, problems) = list();
        assert_eq!(listed.len(), 1, "only the parsable recipe is listed");
        assert_eq!(listed[0].recipe.id, "weekly");
        assert_eq!(
            problems.len(),
            1,
            "the broken file is reported: {problems:?}"
        );
        assert!(problems[0].contains("broken.recipe.kdl"));
    }

    #[test]
    fn an_unsigned_recipe_is_refused_install_with_the_adr_sentence() {
        let dir = tmp("install-unsigned");
        let r = wright_core::recipe::Recipe {
            id: "weekly".into(),
            version: "1.0".into(),
            publisher: "test".into(),
            description: "A weekly report.".into(),
            blueprint: BLUEPRINT.into(),
            publisher_key: String::new(),
            signature: None,
        };
        let f = dir.join("weekly.recipe.kdl");
        std::fs::write(&f, r.to_kdl()).unwrap();
        let err = install(&f, &dir).unwrap_err();
        assert!(err.contains("no signature"), "{err}");
        assert!(err.contains("Unsigned recipes are not installed"), "{err}");
        assert!(!dir.join("Weekly demo").exists(), "nothing was created");
    }

    #[test]
    fn a_signed_recipe_installs_into_a_fresh_folder() {
        let dir = tmp("install-signed");
        let mut r = wright_core::recipe::Recipe {
            id: "weekly".into(),
            version: "1.0".into(),
            publisher: "test".into(),
            description: "A weekly report.".into(),
            blueprint: BLUEPRINT.into(),
            publisher_key: String::new(),
            signature: None,
        };
        r.sign_with(&[5u8; 32]);
        let f = dir.join("weekly.recipe.kdl");
        std::fs::write(&f, r.to_kdl()).unwrap();
        let installed = install(&f, &dir).unwrap();
        assert!(installed.join("blueprint.kdl").exists());
        // twice refuses: the folder exists now
        let err = install(&f, &dir).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }
}
