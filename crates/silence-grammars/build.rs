use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

type BuildResult<T> = Result<T, Box<dyn Error>>;

fn crate_module(crate_name: &str) -> String {
    crate_name.replace('-', "_")
}

fn verify_embed_deps(manifest: &str) -> BuildResult<()> {
    for spec in silence_langs::LANGS {
        let Some(pack) = spec.pack else {
            continue;
        };
        let needle = format!("{} = ", pack.crate_name);
        if !manifest.contains(&needle) {
            return Err(format!(
                "embed-optional missing Cargo.toml dep for {}",
                pack.crate_name
            )
            .into());
        }
    }
    Ok(())
}

fn write_embedded(out: &PathBuf) -> BuildResult<()> {
    let mut arms = String::new();
    for spec in silence_langs::LANGS {
        let Some(pack) = spec.pack else {
            continue;
        };
        let module = crate_module(pack.crate_name);
        arms.push_str(&format!(
            "            Lang::{:?} => Some({module}::LANGUAGE.into()),\n",
            spec.lang
        ));
    }

    let src = format!(
        "use silence_langs::Lang;\n\
use tree_sitter::Language;\n\
\n\
pub fn embedded_optional(lang: Lang) -> Option<Language> {{\n\
    #[cfg(feature = \"embed-optional\")]\n\
    {{\n\
        match lang {{\n\
{arms}            _ => None,\n\
        }}\n\
    }}\n\
    #[cfg(not(feature = \"embed-optional\"))]\n\
    {{\n\
        let _ = lang;\n\
        None\n\
    }}\n\
}}\n"
    );
    fs::write(out, src)?;
    Ok(())
}

fn main() -> BuildResult<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let manifest =
        fs::read_to_string(PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("Cargo.toml"))?;
    verify_embed_deps(&manifest)?;
    write_embedded(&out_dir.join("embedded_optional.rs"))?;
    println!("cargo:rerun-if-changed=../silence-langs/src/registry.rs");
    Ok(())
}
