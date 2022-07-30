use std::{env::args, fs::read};

use goblin::{mach::cputype::get_arch_name_from_types, Object};

fn main() {
    let path = args().nth(1).unwrap();
    let bytes = read(path).unwrap();
    let object = Object::parse(&bytes).unwrap();
    if let Object::Mach(goblin::mach::Mach::Fat(fat)) = object {
        for (i, arch) in fat.iter_arches().enumerate() {
            let arch = arch.unwrap();
            let arch_name = get_arch_name_from_types(arch.cputype(), arch.cpusubtype()).unwrap();
            println!("Parsing entry {i} for arch {arch_name}");
            if let Err(e) = fat.get(i) {
                println!("Failed to get archive entry {i}: {}", e);
            }
        }
    } else {
        panic!("Expected multi-arch mach-o binary");
    }
}
