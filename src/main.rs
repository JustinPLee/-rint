use clap::Parser as ClapParser;
use rint::driver::run_with_file;
use std::fs;

#[derive(ClapParser, Debug)]
#[command(version, about)]
struct Args {
    filepath: String,
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

fn main() {
    let args = Args::parse();
    let filepath = args.filepath;

    let file_contents = fs::read_to_string(&filepath).expect("read input file");
    // let options = Options::default();
    run_with_file(&filepath, &file_contents).expect("no errors");
}
