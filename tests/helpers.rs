use rint::analysis::AnalysisErrorKind;
use rint::driver::{CompileError, compile, run as compile_and_run};
use rint::lexer::LexErrorKind;
use rint::parser::ParseErrorKind;

pub fn run(source: &str) -> i32 {
    compile_and_run(source).expect("program should compile and run")
}

pub fn compile_err(source: &str) -> CompileError {
    compile(source).expect_err("program should fail to compile")
}

pub fn assert_analysis_error(source: &str, expected: AnalysisErrorKind) {
    assert!(matches!(
        compile_err(source),
        CompileError::Analysis(actual) if actual.kind == expected
    ));
}

pub fn assert_lex_error(source: &str, expected: LexErrorKind) {
    assert!(matches!(
        compile_err(source),
        CompileError::Lex(actual) if actual.iter().any(|error| error.kind == expected)
    ));
}

pub fn assert_parse_error(source: &str, expected: ParseErrorKind) {
    assert!(matches!(
        compile_err(source),
        CompileError::Parse(actual) if actual.iter().any(|error| error.kind == expected)
    ));
}
