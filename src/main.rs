use std::{error::Error, fmt::Display, path::PathBuf};

use nicks_linker::{
    linker_args::{Architecture, Args},
    tbd::{self, TbdDylib},
};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Object<'a> {
    /// An ELF32/ELF64!
    Elf(goblin::elf::Elf<'a>),
    /// A PE32/PE32+!
    PE(goblin::pe::PE<'a>),
    /// A 32/64-bit Mach-o binary _OR_ it is a multi-architecture binary container!
    Mach(goblin::mach::Mach<'a>),
    /// A Unix archive
    Archive(goblin::archive::Archive<'a>),
    /// A text stub file
    Tbd(tbd::TbdDylib),
}

impl<'a> From<TbdDylib> for Object<'a> {
    fn from(tbd: TbdDylib) -> Self {
        Object::Tbd(tbd)
    }
}

impl<'a> Object<'a> {
    pub fn parse(s: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        let goblin_obj = goblin::Object::parse(s)?;
        if let goblin::Object::Unknown(_) = goblin_obj {
            Ok(tbd::TbdDylib::parse(Architecture::ARM64, s).unwrap().into())
        } else {
            Ok(goblin_obj.try_into().unwrap())
        }
    }
}

#[derive(Debug)]
pub struct ObjectConversionError(());
impl Error for ObjectConversionError {}
impl Display for ObjectConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to convert object")
    }
}

impl<'a> TryFrom<goblin::Object<'a>> for Object<'a> {
    type Error = ObjectConversionError;

    fn try_from(o: goblin::Object<'a>) -> Result<Self, Self::Error> {
        match o {
            goblin::Object::Elf(elf) => Ok(Object::Elf(elf)),
            goblin::Object::PE(pe) => Ok(Object::PE(pe)),
            goblin::Object::Mach(mach) => Ok(Object::Mach(mach)),
            goblin::Object::Archive(archive) => Ok(Object::Archive(archive)),
            goblin::Object::Unknown(_) => Err(ObjectConversionError(())),
        }
    }
}

fn main() {
    env_logger::init();
    let mut args = Args::from_env().unwrap();
    let mut object_files = vec![];
    object_files.append(&mut args.object_files);
    let library_search_paths = if let Some(ref root) = args.sys_lib_root {
        args.library_search_paths
            .iter()
            .map(|path| {
                let non_abs_path = if path.starts_with("/") {
                    path.strip_prefix("/").unwrap()
                } else {
                    path
                };
                root.join(non_abs_path)
            })
            .collect()
    } else {
        args.library_search_paths.clone()
    };
    log::trace!("Using library search paths: {:?}", library_search_paths);
    for library in &args.libraries {
        let maybe_path = discover_library_path(&library_search_paths, library);
        if let Some(path) = maybe_path {
            object_files.push(path);
        } else {
            log::warn!("Unable to find libary {}", library);
        }
    }
    let object_contents = object_files
        .iter()
        .map(|object_file_path| std::fs::read(&object_file_path).map_err(|e| e.to_string()))
        .collect::<Result<Vec<Vec<u8>>, String>>()
        .unwrap();
    let objects = object_contents
        .iter()
        .map(|object_content| Object::parse(object_content.as_slice()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<Object>, String>>()
        .unwrap();
    
    log::debug!("Args: {args:?}");
    log::debug!("Linking {} objects", objects.len());
    log::debug!("Objects: {objects:#?}");
}

fn discover_library_path(locations: &[PathBuf], library_name: &str) -> Option<PathBuf> {
    let extensions = ["tbd", "dylib", "a"];
    for prefix in locations {
        for extension in extensions {
            let candidate = prefix
                .join(&format!("lib{}", library_name))
                .with_extension(extension);
            log::trace!(
                "Trying candidate {} for library {library_name}",
                candidate.display()
            );
            if candidate.exists() {
                log::trace!(
                    "Using candidate {} for library {library_name}",
                    candidate.display()
                );
                return Some(candidate);
            }
        }
    }
    None
}
