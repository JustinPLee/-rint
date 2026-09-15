use std::{
    fs::{self, OpenOptions},
    io::Write,
    process::Command,
};

use crate::{
    analysis::typecheck, codegen::codegen, context::Context, elaborate::elaborate,
    ir_ast_3ac::translate, lexer::Lexer, parser::Parser, registers::allocate, temps::TempGen,
    token::Token, x86::emit_assembly,
};

pub struct Options {
    pub skip_reg_alloc_use_stack: bool,
    pub optimizations: bool, // specify levels later

    pub show_locations: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            skip_reg_alloc_use_stack: false,
            optimizations: true,
            show_locations: false,
        }
    }
}
pub fn compile(source: &[u8], options: &Options) -> String {
    let mut logfile = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("log.txt")
        .expect("file opened");
    let mut ctx = Context::new(source);

    let tokens = Lexer::new(&mut ctx, source).lex();
    ctx.report_diags();
    let untokens: Vec<Token> = tokens.iter().map(|lt| &lt.0).cloned().collect();
    writeln!(logfile, "Lexed Tokens:\n{:?}", untokens).expect("file write");

    let cst = Parser::new(&mut ctx, tokens.as_slice()).parse().unwrap();
    ctx.report_diags();
    writeln!(logfile, "\nCST:\n{}", cst).expect("file write");

    let ast = elaborate(cst);
    writeln!(logfile, "\nAST:\n{}", ast).expect("file write");

    typecheck(&ast).expect("no analysis errors");

    let mut temps = TempGen::new();
    let three_address = translate(&ast, &mut temps);
    writeln!(logfile, "\n(IR) Three address code:\n{}", three_address).expect("file write");

    let aasm = codegen(&three_address, &mut temps);
    writeln!(logfile, "\n(IR) Abstract assembly:\n{}", aasm).expect("file write");

    let allocations = allocate(&aasm);
    // writeln!(logfile, "{:?}", allocations).expect("file write");

    let asm = emit_assembly(&aasm, &allocations, "main");
    writeln!(logfile, "Emitted assembly:\n{}", asm).expect("file write");

    asm
}

pub fn run(asm: &str) -> i32 {
    fs::write("out.s", &asm).expect("writes");

    let status = Command::new("gcc")
        .args(["out.s", "-o", "out"])
        .status()
        .expect("failed to run gcc");

    assert!(status.success(), "assembly failed to compile");

    let result = Command::new("./out").status().expect("runs");
    result.code().unwrap_or(-1)
}
