use std::{
    ffi::{OsStr, OsString},
    fmt::Display,
};
use std::{path::PathBuf, str::FromStr};

use llvm_option_parser::ParsedArguments;

#[derive(Debug, Clone)]
pub enum Architecture {
    ARM64,
}

impl Display for Architecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Architecture::ARM64 => write!(f, "arm64"),
        }
    }
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

impl Args {
    pub fn from_env() -> Result<Self, String> {
        let options = llvm_command_parser::llvm_13_options("lld-macho").unwrap();
        let mut args = std::env::args_os();
        // Fist arg is the name of the executable.
        args.next();
        let lld_args: ParsedArguments = options
            .parse_arguments(args)
            .map_err(|e| e.to_string())?
            .resolve_aliases(&options)
            .unwrap();
        log::trace!("parsed args: {lld_args:#?}");

        let mut object_files: Vec<PathBuf> = vec![];
        let mut libraries: Vec<String> = vec![];
        let mut sys_lib_root: Option<PathBuf> = None;
        let mut dynamic = false;
        let mut no_deduplicate = false;
        let mut demangle = false;
        let mut output_file = None;
        let mut platform_version: Option<PlatformVersion> = None;
        let mut library_search_paths: Vec<PathBuf> = vec![];
        let mut arch: Option<Architecture> = None;
        for lld_arg in lld_args.parsed() {
            use llvm_option_parser::ParsedArgument::*;
            match lld_arg {
                Unknown(flag) => {
                    log::warn!("Unknown flag {}", flag.to_string_lossy())
                }
                Positional(value) => object_files.push(value.into()),

                Flag(option) => {
                    if option.matches_exact(OsStr::new("-help")) {
                        usage();
                        std::process::exit(1)
                    } else if option.matches_exact(OsStr::new("-dynamic")) {
                        dynamic = true;
                    } else if option.matches_exact(OsStr::new("-no_deduplicate")) {
                        no_deduplicate = true;
                    } else if option.matches_exact(OsStr::new("-demangle")) {
                        demangle = true;
                    } else {
                        log::warn!("Flag {} not handled", option.name)
                    }
                }
                SingleValue(option, value) => {
                    if option.matches_exact(OsStr::new("-o")) {
                        output_file = Some(PathBuf::from(value));
                    } else if option.matches_exact(OsStr::new("-arch")) {
                        arch = Some(value.to_str().unwrap().parse()?);
                    } else if option.matches_exact(OsStr::new("-lto_library")) {
                    } else if option.matches_exact(OsStr::new("-syslibroot")) {
                        sys_lib_root = Some(value.into());
                    } else if option.matches_exact(OsStr::new("-L")) {
                        library_search_paths.push(value.into());
                    } else if option.matches_exact(OsStr::new("-l")) {
                        libraries.push(value.to_os_string().into_string().unwrap());
                    } else {
                        log::warn!(
                            "Flag {} with value {} not handled",
                            option.name,
                            value.to_string_lossy(),
                        )
                    }
                }
                SingleValueKeyed(option, key, value) => {
                    log::warn!(
                        "Single keyed value flag {} with {}={} not handled",
                        option.name,
                        key.to_string_lossy(),
                        value.to_string_lossy()
                    )
                }
                CommaValues(option, comma_separated_values) => {
                    log::warn!(
                        "Comma separated flag {} with value {} not handled",
                        option.name,
                        comma_separated_values.to_string_lossy()
                    );
                }
                MultipleValues(option, values) => {
                    if option.matches_exact(OsStr::new("-platform_version")) {
                        let s: Vec<String> = values
                            .iter()
                            .map(|os| os.to_os_string().into_string())
                            .collect::<Result<Vec<String>, OsString>>()
                            .unwrap();
                        platform_version = Some(s.join(" ").parse()?);
                    } else {
                        log::warn!(
                            "Multi value flag {} with value {:?} not handled",
                            option.name,
                            values
                        )
                    }
                }
                MultipleValuesKeyed(option, key, comma_separated_values) => {
                    log::warn!(
                        "Multi value keyed flag {} with value {}={:?} not handled",
                        option.name,
                        key.to_string_lossy(),
                        comma_separated_values
                    )
                }
            }
        }

        if arch.is_none() {
            return Err("-arch must be provided".into());
        }
        let arch = arch.unwrap();

        if output_file.is_none() {
            return Err("-output_file must be provided".into());
        }
        let output_file = output_file.unwrap();

        Ok(Args {
            arch,
            library_search_paths,
            libraries,
            output_file,
            object_files,
            sys_lib_root,
            demangle,
            deduplicate: !no_deduplicate,
            dynamic,
            platform_version,
        })
    }
}

fn usage() {
    eprintln!(
        r#"
machop

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
