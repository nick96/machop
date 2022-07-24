pub(crate) use std::{path::PathBuf, str::FromStr};

#[derive(Debug)]
pub enum Architecture {
    ARM64,
}

#[derive(Debug)]
pub struct PlatformVersion {
    // TODO: This would be better represented as a enum taking a
    // number or one of the predefined strings.
    pub platform: String,
    // TODO: Thse should be parsed to some version representation
    // (major.minor[.patch]).
    pub min_version: String,
    pub sdk_version: String,
}

impl FromStr for PlatformVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(' ').collect();
        if parts.len() != 3 {
            return Err(format!("Expected 3 parts, found {}", parts.len()));
        }
        Ok(Self {
            platform: parts[0].to_string(),
            min_version: parts[1].to_string(),
            sdk_version: parts[2].to_string(),
        })
    }
}

#[derive(Debug)]
pub struct Args {
    pub arch: Architecture,
    pub library_search_paths: Vec<PathBuf>,
    // TODO: Make this an enum so we're explicit about what libs are
    // handled.
    pub libraries: Vec<String>,
    pub output_file: PathBuf,
    pub object_files: Vec<PathBuf>,
    pub sys_lib_root: Option<PathBuf>,
    pub demangle: bool,
    // Note: Defaults to true. I've inverted it from the flag
    // (-no_demangle) because I think that will make the code easier
    // to read. Lets see if this is the case.
    pub deduplicate: bool,
    pub dynamic: bool,
    pub platform_version: Option<PlatformVersion>,
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

impl ToString for Architecture {
    fn to_string(&self) -> String {
        match self {
            Architecture::ARM64 => "arm64".to_string(),
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
        // Grab the ambiguous arguments we don't care about and throw
        // them away.
        let _: String = cli_args
            .value_from_str("-lto_library")
            .map_err(|e| e.to_string())?;

        let arch = cli_args
            .value_from_str("-arch")
            .map_err(|e| e.to_string())?;

        let sys_lib_root = cli_args
            .opt_value_from_str("-syslibroot")
            .map_err(|e| e.to_string())?;

        // Make sure the search paths specified via the CLI have
        // higher precedence over the default ones.
        let mut library_search_paths = cli_args.values_from_str("-L").map_err(|e| e.to_string())?;
        let mut default_library_search_paths = vec!["/usr/lib".into(), "/usr/local/lib".into()];

        library_search_paths.append(&mut default_library_search_paths);
        let libraries = cli_args.values_from_str("-l").map_err(|e| e.to_string())?;
        let output_file = cli_args.value_from_str("-o").map_err(|e| e.to_string())?;
        let demangle = cli_args.contains("-demangle");
        let deduplicate = !cli_args.contains("-no_deduplicate");
        let dynamic = cli_args.contains("-dynamic");
        let platform_version: Option<PlatformVersion> = cli_args
            .opt_value_set_from_str("-platform_version", 3)
            .map_err(|e| e.to_string())?;
        let object_files = cli_args
            .finish()
            .iter()
            .filter_map(|value| {
                let p: PathBuf = value.into();
                match p.extension().map(|e| e.to_str().unwrap()) {
                    Some("o") | Some("rlib") | Some("a") => Some(p),
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
            sys_lib_root,
            demangle,
            deduplicate,
            dynamic,
            platform_version,
        })
    }
}

fn usage() {
    eprintln!(
        r#"
nicks-linker

Options:

-help                         Print this message
-arch <ARCH>                  Specify the target architecture
-L <DIR>                      Add directory to library search path
-l <LIB>                      Search for library
-o <FILE>                     Set the output file
-lto_library <FILE>
-syslibroot <DIR>
-platform_version <PLATFORM> <MIN_VERSION> <SDK_VERSION>



Any other arguments are treated as the input object files. Those that
don't end in the extension .rlib or .o will be ignored.
"#
    )
}
