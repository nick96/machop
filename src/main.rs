use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt::{Debug, Display},
    fs::Permissions,
    os::unix::prelude::PermissionsExt,
    path::PathBuf,
};

use goblin::mach::{
    cputype::CPU_TYPE_ARM64,
    header::{filetype_to_str, MH_DYLIB, MH_EXECUTE},
    symbols::Nlist,
    MachO, SingleArch,
};
use machop::{
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

enum Dylib<'a> {
    MachO(&'a MachO<'a>),
    Tbd(&'a tbd::TbdDylib),
}

struct Symbol<'a> {
    name: &'a str,
    nlist: Nlist,
    object: Dylib<'a>,
}

impl<'a> Debug for Symbol<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Symbol")
            // .field("name", &self.name)
            .field("nlist", &self.nlist)
            .field("type_str()", &self.nlist.type_str())
            .field("is_global()", &self.nlist.is_global())
            .field("is_weak()", &self.nlist.is_weak())
            .field("is_undefined()", &self.nlist.is_undefined())
            .field("is_stab()", &self.nlist.is_stab())
            .finish()
    }
}

fn main() {
    env_logger::init();
    let mut args = Args::from_env().unwrap();
    args.library_search_paths
        .append(&mut vec!["/usr/lib".into(), "/usr/local/lib".into()]);
    // Dedupe only removes consecutive duplicates so we need to sort
    // it first. Maybe it'd be better to just use a set?
    args.library_search_paths.sort();
    args.library_search_paths.dedup();
    log::debug!("Arg: {:#?}", args);
    // args.object_files = vec![args.object_files.first().unwrap().to_owned()];
    // args.libraries = vec![];
    let mut object_files = vec![];
    // let (cpu_type, cpu_subtype) = get_arch_from_flag(&args.arch.to_string())
    //     .unwrap_or_else(|| panic!("no arch found for {}", args.arch));
    object_files.append(&mut args.object_files.clone());
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
    log::trace!("Object files: {:?}", object_files);
    let object_contents = object_files
        .iter()
        .map(|object_file_path| std::fs::read(&object_file_path).map_err(|e| e.to_string()))
        .collect::<Result<Vec<Vec<u8>>, String>>()
        .unwrap();
    let objects = object_contents
        .iter()
        .enumerate()
        .map(|(i, object_content)| {
            Object::parse(object_content.as_slice())
                .map_err(|e| e.to_string() + &format!(" xxx {}", i))
                .unwrap()
        })
        .collect::<Vec<Object>>();
    log::debug!("Linking {} objects", objects.len());
    // log::debug!("Objects: {objects:#?}");

    let mut dylibs = vec![];
    let mut objs: Vec<MachO> = vec![];
    let mut unowned_objs: Vec<&MachO> = vec![];

    for (i, object) in objects.iter().enumerate() {
        match object {
            Object::Elf(_) => todo!(),
            Object::PE(_) => todo!(),
            Object::Mach(mach) => match mach {
                goblin::mach::Mach::Fat(fat) => {
                    let arch_position = fat
                        .iter_arches()
                        .position(|arch| {
                            let arch = arch.unwrap();
                            arch.cputype() & CPU_TYPE_ARM64 != 0
                        })
                        .unwrap();
                    match fat.get(arch_position) {
                        Ok(entry) => match entry {
                            SingleArch::MachO(macho) => {
                                if macho.is_object_file() {
                                    objs.push(macho);
                                }
                            }
                            SingleArch::Archive(archive) => {
                                let content = &object_contents[i];
                                let arch = fat.iter_arches().nth(arch_position).unwrap().unwrap();
                                let start = arch.offset as usize;
                                let end = (arch.offset + arch.size) as usize;
                                let bytes = &content[start..end];
                                for member_name in archive.members() {
                                    let member_bytes = archive.extract(member_name, bytes).unwrap();
                                    let macho = MachO::parse(member_bytes, 0).unwrap();
                                    if macho.is_object_file() {
                                        objs.push(macho);
                                    }
                                }
                            }
                        },
                        Err(e) => panic!("{}", e),
                    }
                }
                goblin::mach::Mach::Binary(macho) => {
                    if macho.is_object_file() {
                        unowned_objs.push(macho);
                    } else {
                        match macho.header.filetype {
                            MH_EXECUTE | MH_DYLIB => dylibs.push(Dylib::MachO(macho)),
                            _ => panic!(
                                "unhandled macho filetype {}",
                                filetype_to_str(macho.header.filetype)
                            ),
                        }
                    }
                }
            },
            Object::Archive(archive) => {
                let bytes = &object_contents[i];
                for member_name in archive.members() {
                    let member_bytes = archive.extract(member_name, bytes).unwrap();
                    let macho = MachO::parse(member_bytes, 0).unwrap();
                    if macho.is_object_file() {
                        objs.push(macho);
                    }
                }
            }
            Object::Tbd(tbd) => dylibs.push(Dylib::Tbd(tbd)),
        }
    }

    // let mut executable = ArtifactBuilder::new(target_lexicon::Triple {
    //     architecture: target_lexicon::Architecture::Arm(ArmArchitecture::Arm),
    //     vendor: target_lexicon::Vendor::Unknown,
    //     operating_system: target_lexicon::OperatingSystem::Unknown,
    //     environment: target_lexicon::Environment::Unknown,
    //     binary_format: target_lexicon::BinaryFormat::Macho,
    // })
    // .name(
    //     args.output_file
    //         .file_name()
    //         .unwrap()
    //         .to_str()
    //         .unwrap()
    //         .to_string(),
    // )
    // .finish();

    let mut symbols: HashMap<String, Symbol> = HashMap::new();
    let mut undefined_symbols: HashSet<String> = HashSet::new();

    for obj in &objs {
        for symbol in obj.symbols() {
            let (name, nlist) = symbol.unwrap();
            // println!(
            //     "{}:\t{:?}, type={}, global={}, weak={}, undefined={}, stab={}",
            //     name,
            //     Nlist64::from(nlist.clone()),
            //     nlist.type_str(),
            //     nlist.is_global(),
            //     nlist.is_weak(),
            //     nlist.is_undefined(),
            //     nlist.is_stab(),
            // );
            let symbol = Symbol {
                nlist,
                object: Dylib::MachO(obj),
                name,
            };

            // Keep track of undefined symbols so that we can check
            // them at the end. If we encounter a definition of the
            // symbol it'll be removed from the set.
            if symbol.nlist.is_undefined() {
                undefined_symbols.insert(name.to_string());
                continue;
            }

            // Insert the symbol, whatever is, if we've never seen it
            // before. Otherwise, only insert it if the new symbol is
            // not weak. If there are only weak symbols then we just
            // take the first one.
            //
            // Having two "strong" symbols is not allowed (through we
            // don't return an error - maybe we should?).
            if let Some(existing_symbol) = symbols.get(name) {
                if existing_symbol.nlist.is_weak() && !symbol.nlist.is_weak() {
                    // The old symbol was weak but this one isn't - replace it.
                    symbols.insert(name.to_string(), symbol);
                    undefined_symbols.remove(name);
                } else if !existing_symbol.nlist.is_weak() && !symbol.nlist.is_weak() {
                    log::warn!(
                        "Non-weak symbol {} already exists. Ignoring it but this is malformed.\nHave={:?}\ngot={:?}",
                        name,
                        existing_symbol,
                        symbol
                    )
                } else {
                    log::trace!("Weak symbol {} already seen, ignoring it", name)
                }
            } else {
                symbols.insert(name.to_string(), symbol);
                undefined_symbols.remove(name);
            }
        }
    }

    for obj in unowned_objs {
        for symbol in obj.symbols() {
            let (name, nlist) = symbol.unwrap();
            let symbol = Symbol {
                name,
                nlist,
                object: Dylib::MachO(obj),
            };
            if let Some(existing_symbol) = symbols.get(name) {
                if existing_symbol.nlist.is_weak() && !symbol.nlist.is_weak() {
                    // The old symbol was weak but this one isn't - replace it.
                    symbols.insert(name.to_string(), symbol);
                    undefined_symbols.remove(name);
                } else if !existing_symbol.nlist.is_weak() && !symbol.nlist.is_weak() {
                    log::warn!(
                        "Non-weak symbol {} already exists. Ignoring it but this is malformed.\nHave={:?}\ngot={:?}",
                        name,
                        existing_symbol,
                        symbol
                    )
                } else {
                    log::trace!("Weak symbol {} already seen, ignoring it", name)
                }
            } else {
                symbols.insert(name.to_string(), symbol);
                undefined_symbols.remove(name);
            }
        }
    }

    for dylib in dylibs {
        match dylib {
            Dylib::MachO(_) => todo!(),
            Dylib::Tbd(tbd) => {
                for export in &tbd.exports {
                    if undefined_symbols.contains(export) {
                        log::trace!("{export} will be defined by {}", tbd.install_name.display());
                        undefined_symbols.remove(export);
                    }
                }
            }
        }
    }

    let mut segments: HashMap<String, HashMap<String, HashMap<String, &Symbol>>> = HashMap::new();
    for symbol in symbols.values() {
        let section_number = symbol.nlist.n_sect;
        if section_number != 0 {
            // Sections numbers, as given in the Mach-O binary, are
            // 1-based.
            let section_index = section_number - 1;
            match &symbol.object {
                Dylib::MachO(macho) => {
                    let (section, _section_data) = macho
                        .segments
                        .sections()
                        .flatten()
                        .nth(section_index)
                        .unwrap()
                        .unwrap();
                    let section_name = section.name().unwrap();
                    let segment_name = section.segname().unwrap();
                    if let Some(sections) = segments.get_mut(segment_name) {
                        if let Some(section) = sections.get_mut(section_name) {
                            section.insert(symbol.name.to_string(), symbol);
                        } else {
                            let mut section = HashMap::new();
                            section.insert(symbol.name.to_string(), symbol);
                            sections.insert(section_name.to_string(), section);
                        };
                    } else {
                        let mut sections = HashMap::new();
                        let mut section = HashMap::new();
                        section.insert(symbol.name.to_string(), symbol);
                        sections.insert(section_name.to_string(), section);
                        segments.insert(segment_name.to_string(), sections);
                    }
                }
                Dylib::Tbd(_) => todo!(),
            }
        }
    }

    for (segment_name, sections) in segments {
        println!("{}", segment_name);
        for (section_name, symbols) in sections {
            println!("\t{}", section_name);
            for (symbol_name, _) in symbols {
                println!("\t\t{}", symbol_name);
            }
        }
    }

    if !undefined_symbols.is_empty() {
        for symbol in undefined_symbols {
            log::error!("{symbol} is undefined")
        }
        std::process::exit(1)
    }

    let fh = std::fs::File::create(&args.output_file).unwrap();
    // Make rwx by all.
    fh.set_permissions(Permissions::from_mode(0o777)).unwrap();
    // executable.write(fh).unwrap();
}

fn discover_library_path(locations: &[PathBuf], library_name: &str) -> Option<PathBuf> {
    log::trace!("Discovering library {library_name}");
    let extensions = ["tbd", "dylib", "a"];
    for prefix in locations {
        for extension in extensions {
            log::trace!(
                "Looking for library {library_name} with extension {extension} in {}",
                prefix.display()
            );
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
