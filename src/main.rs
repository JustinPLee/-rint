use clap::Parser as ClapParser;
use log::info;
use rint::analysis::typecheck;
use rint::codegen::codegen;
use rint::context::Context;
use rint::driver::{Options, compile, run};
use rint::elaborate::elaborate;
use rint::ir_ast_3ac::translate;
use rint::lexer::Lexer;
use rint::location::unloc;
use rint::parser::Parser;
use rint::propagate::copy_const_prop;
use rint::registers::allocate;
use rint::temps::TempGen;
use rint::token::Token;
use rint::x86::emit_assembly;
use std::fs;
use std::process::Command;

// TODO: create a wrapper that calls/compiles

#[derive(ClapParser, Debug)]
#[command(version, about)]
struct Args {
    filepath: String,
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

fn main() {
    let args = Args::parse();
    env_logger::Builder::new()
        .filter_level(args.verbosity.into())
        .init();
    info!("start main");

    let file_contents = fs::read_to_string(args.filepath).expect("read input file");
    let source = file_contents.as_bytes();
    println!("{}", file_contents);

    let options = Options::default();
    let asm = compile(source, &options);
    let result = run(&asm);
    // // // result is truncated to 8 bits
    println!("program returned {}", result);
}
