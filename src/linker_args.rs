pub(crate) use std::{path::PathBuf, str::FromStr};

#[derive(Debug)]
pub enum Architecture {
    ARM64,
}

#[derive(Debug)]
pub struct Args {
    arch: Architecture,
    library_search_paths: Vec<PathBuf>,
    // TODO: Make this an enum so we're explicit about what libs are
    // handled.
    libraries: Vec<String>,
    output_file: PathBuf,
    object_files: Vec<PathBuf>,
}

impl FromStr for Architecture {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Architecture::*;
        match &s.to_lowercase()[..] {
            "arm64" => Ok(ARM64),
            _ => Err(format!("Unknown architecture {s}")),
        }
    }
}

impl Args {
    pub fn from_env() -> Result<Self, String> {
        let mut cli_args = crate::arg_parser::Arguments::from_env();
        if cli_args.contains("-help") {
            usage();
            std::process::exit(1)
        }
        let arch = cli_args
            .value_from_str("-arch")
            .map_err(|e| e.to_string())?;
        let library_search_paths = cli_args.values_from_str("-L").map_err(|e| e.to_string())?;
        let libraries = cli_args.values_from_str("-l").map_err(|e| e.to_string())?;
        let output_file = cli_args.value_from_str("-o").map_err(|e| e.to_string())?;
        let object_files = cli_args
            .finish()
            .iter()
            .filter_map(|value| {
                let p: PathBuf = value.into();
                match p.extension().map(|e| e.to_str().unwrap()) {
                    Some("o") => Some(p),
                    Some("rlib") => Some(p),
                    _ => {
                        log::debug!("{:?} not handled", value);
                        None
                    }
                }
            })
            .collect();

        Ok(Args {
            arch,
            library_search_paths,
            libraries,
            output_file,
            object_files,
        })
    }
}

fn usage() {
    eprintln!(
        r#"
nicks-linker

Options:

-help    Print this message
-arch    Specify the target architecture
-L <DIR> Add directory to library search path
-l <LIB> Search for library
-o <FILE> Set the output file

Any other arguments are treated as the input object files. Those that
don't end in the extension .rlib or .o will be ignored.
"# ) }
