use colored::Colorize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::Command;

use crate::analysis::{AnalysisError, semantic_analysis};
use crate::cfg::analyze_cfg;
use crate::diagnostic::report_diags;
use crate::elaboration::elaborate;
use crate::ir_function;
use crate::ir_linear::translate;
use crate::lexer::{LexError, Lexer};
use crate::parser::{ParseError, Parser};
use crate::token::Token;
use crate::utils::{LabelGen, TempGen};
use crate::x86::emit_assembly;

// pub struct Options {  do optimizations? dump specific phases? }

#[derive(Debug)]
pub enum CompileError {
    Lex(Vec<LexError>),
    Parse(Vec<ParseError>),
    Analysis(AnalysisError),
}

pub fn compile(source: &str) -> Result<String, CompileError> {
    compile_source("<input>", source)
}

const DUMPFILE: &str = "dump.txt";
pub fn compile_source(filename: &str, source: &str) -> Result<String, CompileError> {
    let mut dumpfile = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(DUMPFILE)
        .expect("file opened");

    writeln!(dumpfile, "# Source program:\n{}", source).expect("file write");
    let source = source.as_bytes();

    // Lex
    let mut lexer = Lexer::with_file(source, filename);
    let tokens = lexer.lex();
    if let Some(errors) = lexer.errors() {
        report_diags(source, errors);
        return Err(CompileError::Lex(errors.to_vec()));
    }
    let raw_tokens: Vec<Token> = tokens.iter().map(|lt| &lt.data).cloned().collect();
    writeln!(dumpfile, "# Lexed Tokens:\n{:?}", raw_tokens).expect("file write");

    // Parse
    let mut parser = Parser::new(tokens.as_slice());
    let ast_parse = parser.parse();
    if let Some(errors) = parser.errors() {
        report_diags(source, errors);
        return Err(CompileError::Parse(errors.to_vec()));
    }
    writeln!(dumpfile, "\n# Parse AST:\n{}", ast_parse).expect("file write");

    // Elaborate
    let ast = elaborate(ast_parse);
    writeln!(dumpfile, "\n# AST:\n{}", ast).expect("file write");

    // Semantic Analysis
    if let Err(err) = semantic_analysis(&ast) {
        report_diags(source, &[err.clone()]);
        return Err(CompileError::Analysis(err));
    }

    // IR linear code
    let mut temps = TempGen::new();
    let mut labels = LabelGen::new();
    let module = translate(&ast.data, &mut temps, &mut labels);
    writeln!(dumpfile, "\n# (IR) linear:\n{}", module).expect("file write");

    // IR function code
    let function_ir = ir_function::translate(&module);
    writeln!(
        dumpfile,
        "\n# (IR) function with calling convention:\n{}",
        function_ir
    )
    .expect("file write");

    // CFG analysis checking
    // Initialization and returns checks
    if let Err(err) = analyze_cfg(&function_ir) {
        report_diags(source, &[err.clone()]);
        return Err(CompileError::Analysis(err));
    }

    // Optimization passes
    // let linear = copy_const_prop(straightline);
    // writeln!(
    //     logfile,
    //     "\n# (IR) Optimization: copy/const propagation:\n{}",
    //     linear // )
    // .expect("file write");

    // Emit assembly
    let asm = emit_assembly(&function_ir, &mut labels);
    writeln!(dumpfile, "# Emitted assembly:\n{}", asm).expect("file write");

    Ok(asm)
}

pub fn run(source: &str) -> Result<i32, CompileError> {
    run_with_file("<input>", source)
}

pub fn run_with_file(filename: &str, source: &str) -> Result<i32, CompileError> {
    let asm = compile_source(filename, source);
    if let Ok(asm) = asm {
        fs::write("out.s", &asm).expect("writes");

        let status = Command::new("gcc")
            .args(["out.s", "-o", "out"])
            .status()
            .expect("failed to run gcc");

        assert!(status.success(), "assembly failed to compile");

        let result = Command::new("./out").status().expect("runs");
        // append to dumpfile
        let mut dumpfile = OpenOptions::new()
            .write(true)
            .append(true)
            .open(DUMPFILE)
            .expect("file opened");

        if let Some(code) = result.code() {
            // result is truncated to 8 bits
            println!("{}", format!("program returned {}", result).green());
            println!("{}", format!("see dumped output in dump.txt").blue());
            write!(dumpfile, "# Result:\n{}", code).expect("file write");
            return Ok(code);
        } else {
            println!("{}", format!("program returned {}", result).red());
            write!(dumpfile, "# Result:\n Error",).expect("file write");
            return Ok(-1);
        }
    }
    Ok(-1)
}
