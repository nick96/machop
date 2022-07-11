use nicks_linker::linker_args::Args;

fn main() {
    env_logger::init();
    let args = Args::from_env().unwrap();
    log::debug!("Args: {:#?}", args)
}
