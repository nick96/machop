use std::collections::HashMap;

/// Parse .tbd files.
use crate::linker_args::Architecture;

#[derive(Debug)]
pub enum Error {
    ParseError(String),
    NoValidDocument,
}

impl std::error::Error for Error {}

impl From<text_stub_library::ParseError> for Error {
    fn from(e: text_stub_library::ParseError) -> Self {
        Error::ParseError(e.to_string())
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(_: std::str::Utf8Error) -> Self {
        todo!()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ParseError(s) => write!(f, "{}", s),
            Error::NoValidDocument => write!(f, "No valid documents found"),
        }
    }
}

#[derive(Debug)]
pub struct TbdDylib {
    pub install_name: String,
    pub reexported_libraries: Vec<String>,
    pub exports: Vec<String>,
    pub weak_exports: Vec<String>,
}

fn match_arch(arch: &Architecture, triple: &str) -> bool {
    let arch = arch.to_string();
    arch == triple || triple.starts_with(&format!("{arch}-"))
}

impl TbdDylib {
    pub fn parse(arch: Architecture, content: &[u8]) -> Result<Self, Error> {
        let text = std::str::from_utf8(content)?;
        let mut tbds: Vec<TbdDylib> = text_stub_library::parse_str(text)?
            .into_iter()
            .filter_map(|tbd| match Self::parse_one(&arch, tbd) {
                Ok(Some(v)) => Some(Ok(v)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<TbdDylib>, Error>>()?;
        if tbds.is_empty() {
            return Err(Error::NoValidDocument);
        };
        // We know there is at least one element in tbds so this won't
        // panic.
        let mut main = tbds.remove(0);
        let tbds_by_install_name: HashMap<&String, &TbdDylib> =
            tbds.iter().map(|tbd| (&tbd.install_name, tbd)).collect();
        let mut exports = vec![];
        let mut weak_exports = vec![];
        let mut reexported_libs = vec![];
        let mut visit = |tbd: &TbdDylib| {
            for lib in &tbd.reexported_libraries {
                if let Some(child) = tbds_by_install_name.get(&lib) {
                    let mut child_exports = child.exports.clone();
                    exports.append(&mut child_exports);
                    let mut child_weak_exports = child.weak_exports.clone();
                    weak_exports.append(&mut child_weak_exports);
                } else {
                    reexported_libs.push(lib.to_owned())
                }
            }
        };
        visit(&main);
        main.exports.append(&mut exports);
        main.weak_exports.append(&mut weak_exports);
        main.reexported_libraries.append(&mut reexported_libs);
        Ok(main)
    }

    fn parse_one(
        arch: &Architecture,
        tbd: text_stub_library::TbdVersionedRecord,
    ) -> Result<Option<Self>, Error> {
        let tbd = match tbd {
            text_stub_library::TbdVersionedRecord::V1(_)
            | text_stub_library::TbdVersionedRecord::V2(_)
            | text_stub_library::TbdVersionedRecord::V3(_) => return Ok(None),
            text_stub_library::TbdVersionedRecord::V4(v4) => {
                if v4.targets.iter().any(|triple| match_arch(arch, triple)) {
                    v4
                } else {
                    return Ok(None);
                }
            }
        };
        let reexported_libraries = tbd
            .reexported_libraries
            .into_iter()
            .flat_map(|reexport| {
                if reexport
                    .targets
                    .iter()
                    .any(|triple| match_arch(arch, triple))
                {
                    reexport.libraries
                } else {
                    vec![]
                }
            })
            .collect();
        let mut all_exports = vec![];
        let mut all_weak_exports = vec![];
        for exports in tbd.exports {
            if exports
                .targets
                .iter()
                .any(|triple| match_arch(arch, triple))
            {
                all_exports.append(&mut exports.symbols.clone());
                all_weak_exports.append(&mut exports.weak_symbols.clone());
            }
        }
        for reexport in tbd.re_exports {
            if reexport
                .targets
                .iter()
                .any(|triple| match_arch(arch, triple))
            {
                all_exports.append(&mut reexport.symbols.clone());
                all_weak_exports.append(&mut reexport.weak_symbols.clone());
            }
        }

        // TODO: ObjC symbols

        Ok(Some(TbdDylib {
            install_name: tbd.install_name,
            reexported_libraries,
            exports: all_exports,
            weak_exports: all_weak_exports,
        }))
    }
}
