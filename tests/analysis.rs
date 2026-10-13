mod helpers;

use helpers::{compile_err, run};
use rint::analysis::{AnalysisError, AnalysisErrorKind, Env, RType, semantic_analysis};
use rint::driver::CompileError;
use rint::elaboration::elaborate;
use rint::lexer::Lexer;
use rint::parser::Parser;

use crate::helpers::assert_analysis_error;
#[test]
fn uninitialized_return() {
    let source = r#"
    int main() {
        int x;
        return x;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UseUninitializedVariable);
}

#[test]
fn uninitialized_rhs() {
    let source = r#"
    int main() {
        int x;
        int y;
        x = y;
        return x;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UseUninitializedVariable);
}

#[test]
fn if_initialized_one_branch() {
    let source = r#"
    int main() {
        int x;

        if (true) {
            x = 10;
        }
        return x;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UseUninitializedVariable);
}

#[test]
fn while_initialized_inside() {
    let source = r#"
    int main() {
        int x;

        while (false) {
            x = 10;
        }

        return x;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UseUninitializedVariable);
}

#[test]
fn uninitialized_condition() {
    let source = r#"
    int main() {
        int x;

        while (x < 10) {
            x = x + 1;
        }

        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UseUninitializedVariable);
}

#[test]
fn terminating_return() {
    let source = r#"
    int main() {
        return 4;
        int x = 2;
        return x;
    }
    "#;

    assert_eq!(run(source), 4);
}

#[test]
fn no_return() {
    let source = r#"
    int main() {
        int x = 2;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::NoReturn);
}

#[test]
fn if_missing_return() {
    let source = r#"
    int main() {
        if (true) {
            return 1;
        }
        int x = 2;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::NoReturn);
}

#[test]
fn loop_does_not_guarantee_return() {
    let source = r#"
    int main() {
        int x = 0;
        while (x < 10) {
            x = x + 1;
        }
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::NoReturn);
}

#[test]
fn invalid_assignment_type() {
    let source = r#"
    int main() {
        int x = 2;
        x = true;
        return x;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn invalid_binary_operand_types() {
    let source = r#"
    int main() {
        return true + 2;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn invalid_binary_rhs_type() {
    let source = r#"
    int main() {
        return 2 + true;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn invalid_function_return_type() {
    let source = r#"
    int helper() {
        return true;
    }

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionInvalidReturnType);
}

#[test]
fn void_value_is_rejected() {
    let source = r#"
    void helper() {
        return;
    }

    int main() {
        int value = helper();
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VoidUsedAsValue);
}

#[test]
fn function_value_cannot_return_as_int() {
    let source = r#"
    int helper() {
        return 1;
    }

    int main() {
        return helper;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionInvalidReturnType);
}

fn analyze_only(source: &str) -> Result<Env, AnalysisError> {
    let tokens = Lexer::new(source.as_bytes()).lex();
    let mut parser = Parser::new(&tokens);
    let parsed = parser.parse();
    assert!(parser.errors().is_none(), "{:?}", parser.errors());
    semantic_analysis(&elaborate(parsed))
}

#[test]
fn first_class_function_types_and_calls() {
    let source = r#"
    int inc(int x) {
        return x + 1;
    }

    fn(int) -> int identity(fn(int) -> int f) {
        return f;
    }

    int main() {
        fn(int) -> int f = identity(inc);
        return identity(f)(1);
    }
    "#;

    let env = analyze_only(source).expect("function values should type-check");
    let function_type = RType::Function {
        params: vec![RType::Int],
        ret_typ: Box::new(RType::Int),
    };
    assert_eq!(
        env.function_type("identity"),
        (vec![function_type.clone()], function_type)
    );
}

#[test]
fn function_value_call_checks_arguments() {
    let source = r#"
    int main() {
        fn(int) -> int f = null;
        return f(true);
    }
    "#;

    assert_eq!(
        analyze_only(source).unwrap_err().kind,
        AnalysisErrorKind::FunctionArgsTypeMismatch
    );
}

#[test]
fn computed_non_function_cannot_be_called() {
    let source = "int main() { return (1 + 2)(); }";
    assert_eq!(
        analyze_only(source).unwrap_err().kind,
        AnalysisErrorKind::CallNonFunction
    );
}

#[test]
fn variable_used_as_function_is_rejected() {
    let source = r#"
    int main() {
        int helper = 1;
        return helper();
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableCalledAsFunction);
}

#[test]
fn variable_name_cannot_shadow_function() {
    let source = r#"
    int helper() {
        return 1;
    }

    int main() {
        int helper;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableShadowsFunction);
}

#[test]
fn invalid_function_argument_type() {
    let source = r#"
    int helper(int value) {
        return value;
    }

    int main() {
        return helper(true);
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionArgsTypeMismatch);
}

#[test]
fn invalid_ternary_branch_types() {
    let source = r#"
    int main() {
        return true ? 1 : false;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn invalid_equality_operand_types() {
    let source = r#"
    int main() {
        if (true == 1) {
            return 1;
        }
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn invalid_condition_type() {
    let source = r#"
    int main() {
        if (2) {
            return 1;
        }
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn invalid_break() {
    let source = r#"
    int main() {
        break;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::InvalidBreak);
}

#[test]
fn invalid_continue() {
    let source = r#"
    int main() {
        continue;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::InvalidContinue);
}

#[test]
fn variable_not_found_in_expression() {
    let source = r#"
    int main() {
        return missing;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableNotFound);
}

#[test]
fn variable_not_found_in_assignment_target() {
    let source = r#"
    int main() {
        missing = 1;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableNotFound);
}

#[test]
fn typedef_used_as_value_is_not_a_variable() {
    let source = r#"
    typedef int Value;

    int main() {
        return Value;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableIsType);
}

#[test]
fn variable_name_is_typedef() {
    let source = r#"
    typedef int Value;

    int main() {
        int Value;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableIsType);
}

#[test]
fn unknown_type_is_rejected() {
    let source = r#"
    int b(a x) { return 0; }
    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UnknownType);
}

#[test]
fn empty_nonvoid_function_does_not_return() {
    let source = r#"
    int main() {}
    "#;

    assert_analysis_error(source, AnalysisErrorKind::NoReturn);
}

#[test]
fn declaration_then_definition_is_valid() {
    let source = r#"
    int helper();
    int helper();

    int helper() {
        return 7;
    }

    int main() {
        return helper();
    }
    "#;

    assert_eq!(run(source), 7);
}

#[test]
fn function_redeclaration() {
    let source = r#"
    int f() {
        return 1;
    }

    int f();

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionRedeclaration);
}

#[test]
fn function_redefinition() {
    let source = r#"
    int f() {
        return 1;
    }

    int f() {
        return 2;
    }

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionRedefinition);
}

#[test]
fn function_repeated_parameter_name() {
    let source = r#"
    int f(int x, int x) {
        return x;
    }

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionRepeatedParameterName);
}

#[test]
fn function_parameter_name_is_typedef() {
    let source = r#"
    typedef int T;

    int f(int T) {
        return 0;
    }

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableIsType);
}

#[test]
fn function_return_type_differs() {
    let source = r#"
    int f();
    bool f();

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionReturnTypeMismatch);
}

#[test]
fn function_parameter_lengths_differ() {
    let source = r#"
    int f(int x);
    int f();

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionParamCountMismatch);
}

#[test]
fn function_parameter_types_differ() {
    let source = r#"
    int f(int x);
    int f(bool x);

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionParamTypeMismatch);
}

#[test]
fn typedef_alias_is_function() {
    let source = r#"
    int f();
    typedef int f;

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypedefAliasIsFunction);
}

#[test]
fn function_name_is_typedef() {
    let source = r#"
    typedef int f;

    int f() {
        return 0;
    }

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypedefAliasIsFunction);
}

#[test]
fn typedef_repeated_typedef() {
    let source = r#"
    typedef int T;
    typedef bool T;

    int main() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypedefRepeatedTypedef);
}

#[test]
fn typedef_type_not_primitive() {
    let source = r#"
        typedef int A;
        typedef A B;
        "#;

    assert_analysis_error(source, AnalysisErrorKind::TypedefTypeNotPrimitive);
}

#[test]
fn initialization_after_both_branches() {
    let source = r#"
    int main() {
        int x;

        if (true) {
            x = 10;
        } else {
            x = 20;
        }

        return x;
    }
    "#;

    assert_eq!(run(source), 10);
}

#[test]
fn uninitialized_variable_in_ternary() {
    let source = r#"
    int main() {
        int x;
        return true ? x : 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UseUninitializedVariable);
}

#[test]
fn invalid_assert_type() {
    let source = r#"
    int main() {
        assert(1);
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn duplicate_local() {
    let source = r#"
    int main() {
        int x = 1;
        int x = 2;
        return x;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::VariableRedeclaration);
}

#[test]
fn main_is_required() {
    let source = r#"
    int helper() {
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::MainMissing);
}

#[test]
fn main_must_return_int() {
    let source = r#"
    void main() {
        return;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::MainReturnTypeNotInt);
}

#[test]
fn main_must_not_have_parameters() {
    let source = r#"
    int main(int value) {
        return value;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::MainInvalidParameters);
}

#[test]
fn main_must_be_defined() {
    let source = r#"
    int main();
    "#;

    assert_analysis_error(source, AnalysisErrorKind::MainNotDefined);
}
