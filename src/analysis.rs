// ast -> ast

use std::collections::{HashMap, HashSet};
use std::iter::zip;

use crate::ast::{
    BinOp, Expr, GlobalDecl, LAsnOp, LBinOp, LBlock, LExpr, LIdent, LLValue, LParam, LProgram,
    LStmt, LStructField, LTyp, LValue, Program, Stmt, Typ,
};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::location::Location;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisErrorKind {
    UnknownType,
    TypeMismatch,
    NoReturn,
    InvalidBreak,
    InvalidContinue,

    VoidUsedAsValue,
    VoidUsedAsVariableType,
    VariableCalledAsFunction,
    VariableShadowsFunction,

    FunctionNotFound,
    FunctionRedeclaration,
    FunctionRedefinition,
    FunctionRepeatedParameterName,
    FunctionInvalidReturnType,
    FunctionReturnTypeMismatch,
    FunctionParamCountMismatch,
    FunctionParamTypeMismatch,
    FunctionArgsCountMismatch,
    FunctionArgsTypeMismatch,
    FunctionIsValue,

    VariableRedeclaration,
    VariableIsType,
    VariableNotFound,
    UseUninitializedVariable,

    TypedefAliasIsFunction,
    TypedefTypeNotPrimitive,
    TypedefRepeatedTypedef,

    StructRedefinition,
    StructFieldRedeclaration,
    StructFieldNotFound,
    StructEmpty,
    FieldAccessOnNonStruct,
    UndefinedStruct,
    StructValueNotAllowed,

    DereferenceNonPointer,
    ArraysNotImplemented,

    MainMissing,
    MainNotDefined,
    MainReturnTypeNotInt,
    MainInvalidParameters,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisError {
    pub kind: AnalysisErrorKind,
    pub description: String,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InitStatus {
    Declared,
    Defined,
}

// resolved types
// no aliases
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RType {
    Int,
    Bool,
    Void,
    Null,
    Struct(String),
    Pointer(Box<RType>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FunctionInfo {
    name: LIdent,
    params: Vec<LParam>,
    ret_typ: LTyp,
    status: InitStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StructInfo {
    fields: HashMap<String, RType>,
    offsets: HashMap<String, i32>,
    size: i32,
    align: i32,
    status: InitStatus,
}

/// aligns size to the next multiple of align
fn align_to(size: i32, align: i32) -> i32 {
    (size + align - 1) / align * align
}

#[derive(Clone, Debug)]
pub struct Env {
    functions: HashMap<String, FunctionInfo>,
    structs: HashMap<String, StructInfo>,
    typedefs: HashMap<String, RType>,
    variables: HashMap<String, RType>,
    expected_ret_type: RType,
}

pub fn semantic_analysis(program: &LProgram) -> Result<Env, AnalysisError> {
    let mut env = Env::new();
    process_gdecls(&program.data, &mut env)?; // keep this first!

    check_main(&env, program.location.clone())?;
    check_break_and_continue(&program.data)?;
    typecheck(&program.data, &env)?;

    Ok(env)
}

fn check_main(env: &Env, location: Location) -> Result<(), AnalysisError> {
    let Some(main) = env.functions.get("main") else {
        return Err(AnalysisError::new(AnalysisErrorKind::MainMissing, location));
    };

    if main.status != InitStatus::Defined {
        return Err(AnalysisError::new(
            AnalysisErrorKind::MainNotDefined,
            main.name.location.clone(),
        ));
    }

    if env.resolve_type(&main.ret_typ.data, main.ret_typ.location.clone())? != RType::Int {
        return Err(AnalysisError::new(
            AnalysisErrorKind::MainReturnTypeNotInt,
            main.ret_typ.location.clone(),
        ));
    }

    if !main.params.is_empty() {
        return Err(AnalysisError::new(
            AnalysisErrorKind::MainInvalidParameters,
            main.name.location.clone(),
        ));
    }

    Ok(())
}

fn process_gdecls(program: &Program, env: &mut Env) -> Result<(), AnalysisError> {
    // typedefs and definitions are processed in order
    for gdecl in &program.0 {
        match &gdecl.data {
            GlobalDecl::Typedef { typ, alias } => {
                env.add_typedef(typ, alias)?;
            }
            GlobalDecl::FunDef {
                ret_typ,
                name,
                params,
                body,
            } => {
                let function = FunctionInfo {
                    name: name.clone(),
                    params: params.clone(),
                    ret_typ: ret_typ.clone(),
                    status: InitStatus::Declared,
                };

                if body.is_some() {
                    env.add_function_def(function)?;
                } else {
                    env.add_function_decl(function)?;
                }
            }
            GlobalDecl::StructDef { name, body } => match body {
                Some(fields) => env.add_struct_def(name, fields)?,
                None => env.add_struct_decl(name), // structure declarations are always valid
            },
        }
    }

    Ok(())
}

fn typecheck(program: &Program, env: &Env) -> Result<(), AnalysisError> {
    for gdecl in &program.0 {
        match &gdecl.data {
            GlobalDecl::FunDef {
                ret_typ,
                params,
                body,
                ..
            } => tc_fundef(ret_typ, params, body, env)?,
            GlobalDecl::StructDef { .. } | GlobalDecl::Typedef { .. } => continue, // already processed
        }
    }

    Ok(())
}

fn tc_fundef(
    ret_typ: &LTyp,
    params: &[LParam],
    body: &Option<LBlock>,
    env: &Env,
) -> Result<(), AnalysisError> {
    let Some(body) = body else {
        return Ok(());
    };

    // OPTIMIZE: inefficient, but works
    let mut function_env = env.clone();
    function_env.expected_ret_type = env.resolve_type(&ret_typ.data, ret_typ.location.clone())?;

    for param in params {
        function_env.add_variable(
            param.data.name.data.clone(),
            &param.data.typ,
            param.data.name.location.clone(),
        )?;
    }

    tc_block(body, &function_env)
}

fn tc_block(block: &LBlock, env: &Env) -> Result<(), AnalysisError> {
    for stmt in &block.data.0 {
        tc_stmt(stmt, env)?;
    }

    Ok(())
}

fn tc_stmt(stmt: &LStmt, env: &Env) -> Result<(), AnalysisError> {
    // just delegate
    match &stmt.data {
        Stmt::Decl { typ, name, block } => tc_decl(typ, name, block, env),
        Stmt::Assign { name, value } => tc_assign(name, value, env),
        Stmt::CompoundAssign { name, op, value } => tc_compound_assign(name, op, value, env),
        Stmt::Postfix { name, op: _ } => tc_postfix(name, env),
        Stmt::Return(expr) => tc_return(expr.as_ref(), &stmt.location, env),
        Stmt::If {
            cond,
            true_block,
            false_block,
        } => tc_if(cond, true_block, false_block.as_ref(), env),
        Stmt::While { cond, body } => tc_while(cond, body, env),
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => tc_for(init.as_deref(), cond, step.as_deref(), body, env),
        Stmt::Expr(expr) => {
            // typecheck the expression, but don't return its type
            tc_expr(expr, env)?;
            Ok(())
        }
        Stmt::Assert(expr) => check_returns_bool(expr, env),
        Stmt::Block(block) => tc_block(block, env),
        Stmt::Break | Stmt::Continue => Ok(()),
    }
}

fn tc_decl(typ: &LTyp, name: &LIdent, block: &LBlock, env: &Env) -> Result<(), AnalysisError> {
    let mut inner_env = env.clone();
    inner_env.add_variable(name.data.clone(), typ, name.location.clone())?;

    tc_block(block, &inner_env)
}

fn tc_assign(name: &LLValue, value: &LExpr, env: &Env) -> Result<(), AnalysisError> {
    let value_type = tc_value_expr(value, env)?;
    let variable_type = tc_lvalue(name, env)?;

    require_type(&value_type, &variable_type, value.location.clone())
}

fn tc_compound_assign(
    name: &LLValue,
    op: &LAsnOp,
    value: &LExpr,
    env: &Env,
) -> Result<(), AnalysisError> {
    let target_type = tc_lvalue(name, env)?;
    let value_type = tc_value_expr(value, env)?;
    // bools are ints, can't do operations on pointers or structs
    require_type(&target_type, &RType::Int, op.location.clone())?;
    require_type(&value_type, &RType::Int, value.location.clone())
}

fn tc_postfix(name: &LLValue, env: &Env) -> Result<(), AnalysisError> {
    let target_type = tc_lvalue(name, env)?;
    // x++ and x-- are the only postfixes for now
    require_type(&target_type, &RType::Int, name.location.clone())
}

fn tc_lvalue(value: &LLValue, env: &Env) -> Result<RType, AnalysisError> {
    match &value.data {
        LValue::Ident(name) => tc_ident(&name.data, &name.location, env),
        LValue::Field { base, field } => {
            let base_type = tc_lvalue(base, env)?;
            env.field_type(&base_type, field)
        }
        LValue::Deref(pointer) => match tc_value_expr(pointer, env)? {
            RType::Pointer(pointee) => Ok(*pointee),
            _ => Err(AnalysisError::new(
                AnalysisErrorKind::DereferenceNonPointer,
                pointer.location.clone(),
            )),
        },
        LValue::Index { .. } => Err(AnalysisError::new(
            AnalysisErrorKind::ArraysNotImplemented,
            value.location.clone(),
        )),
    }
}

fn tc_if(
    cond: &LExpr,
    true_block: &LBlock,
    false_block: Option<&LBlock>,
    env: &Env,
) -> Result<(), AnalysisError> {
    check_returns_bool(cond, env)?;

    tc_block(true_block, env)?;

    if let Some(false_block) = false_block {
        tc_block(false_block, env)?;
    }

    Ok(())
}

fn tc_while(cond: &LExpr, body: &LBlock, env: &Env) -> Result<(), AnalysisError> {
    check_returns_bool(cond, env)?;
    tc_block(body, env)
}

fn tc_for(
    init: Option<&LStmt>,
    cond: &LExpr,
    step: Option<&LStmt>,
    body: &LBlock,
    env: &Env,
) -> Result<(), AnalysisError> {
    if let Some(init) = init {
        tc_stmt(init, env)?;
    }

    check_returns_bool(cond, env)?;

    tc_block(body, env)?;

    if let Some(step) = step {
        tc_stmt(step, env)?;
    }

    Ok(())
}

pub fn tc_expr(expr: &LExpr, env: &Env) -> Result<RType, AnalysisError> {
    // delegates again
    match &expr.data {
        Expr::Int(_) => Ok(RType::Int),
        Expr::True | Expr::False => Ok(RType::Bool),
        Expr::Null => Ok(RType::Null),
        Expr::Ident(name) => tc_ident(&name.data, &name.location, env),
        Expr::LValue(value) => tc_lvalue(value, env),
        Expr::Field { ident, field } => {
            let base_type = tc_expr(ident, env)?;
            env.field_type(&base_type, field)
        }
        Expr::Deref(pointer) => match tc_value_expr(pointer, env)? {
            RType::Pointer(pointee) => Ok(*pointee),
            _ => Err(AnalysisError::new(
                AnalysisErrorKind::DereferenceNonPointer,
                pointer.location.clone(),
            )),
        },
        Expr::Alloc(typ) => {
            let allocated_type = env.resolve_non_void_type(&typ.data, typ.location.clone())?;
            env.check_type_is_defined(&allocated_type, typ.location.clone())?;
            Ok(RType::Pointer(Box::new(allocated_type)))
        }
        Expr::Index { .. } | Expr::AllocArray { .. } => Err(AnalysisError::new(
            AnalysisErrorKind::ArraysNotImplemented,
            expr.location.clone(),
        )),
        Expr::BinOp { op, lhs, rhs } => tc_binop(op, lhs, rhs, env),
        Expr::TernOp {
            cond,
            true_expr,
            false_expr,
        } => tc_ternop(cond, true_expr, false_expr, env),
        Expr::FunCall { name, args } => tc_call(name, args, env),
    }
}

fn tc_ident(name: &str, location: &Location, env: &Env) -> Result<RType, AnalysisError> {
    if env.typedefs.contains_key(name) {
        return Err(AnalysisError::new(
            AnalysisErrorKind::VariableIsType,
            location.clone(),
        ));
    }

    if env.functions.contains_key(name) {
        return Err(AnalysisError::new(
            AnalysisErrorKind::FunctionIsValue,
            location.clone(),
        ));
    }

    let typ = env
        .variables
        .get(name)
        .ok_or_else(|| AnalysisError::new(AnalysisErrorKind::VariableNotFound, location.clone()))?;

    Ok(typ.clone())
}

fn tc_binop(op: &LBinOp, lhs: &LExpr, rhs: &LExpr, env: &Env) -> Result<RType, AnalysisError> {
    let lhs_type = tc_value_expr(lhs, env)?;
    let rhs_type = tc_value_expr(rhs, env)?;

    match &op.data {
        BinOp::Plus
        | BinOp::Minus
        | BinOp::Times
        | BinOp::Div
        | BinOp::Mod
        | BinOp::BitAnd
        | BinOp::BitOr
        | BinOp::BitXor
        | BinOp::LShift
        | BinOp::RShift => {
            require_type(&lhs_type, &RType::Int, lhs.location.clone())?;
            require_type(&rhs_type, &RType::Int, rhs.location.clone())?;
            Ok(RType::Int)
        }
        BinOp::Greater | BinOp::GreaterEq | BinOp::Less | BinOp::LessEq => {
            require_type(&lhs_type, &RType::Int, lhs.location.clone())?;
            require_type(&rhs_type, &RType::Int, rhs.location.clone())?;
            Ok(RType::Bool)
        }
        BinOp::LogicAnd | BinOp::LogicOr => {
            require_type(&lhs_type, &RType::Bool, lhs.location.clone())?;
            require_type(&rhs_type, &RType::Bool, rhs.location.clone())?;
            Ok(RType::Bool)
        }
        BinOp::EqualEq | BinOp::NotEq => {
            // struct comparison is not implemented yet
            if matches!(&lhs_type, RType::Struct(_)) || matches!(&rhs_type, RType::Struct(_)) {
                return Err(AnalysisError::new(
                    AnalysisErrorKind::StructValueNotAllowed,
                    lhs.location.clone(),
                ));
            }
            require_type(&lhs_type, &rhs_type, rhs.location.clone())?;
            Ok(RType::Bool)
        }
    }
}

fn tc_ternop(
    cond: &LExpr,
    true_expr: &LExpr,
    false_expr: &LExpr,
    env: &Env,
) -> Result<RType, AnalysisError> {
    let cond_type = tc_value_expr(cond, env)?;
    let true_type = tc_value_expr(true_expr, env)?;
    let false_type = tc_value_expr(false_expr, env)?;

    require_type(&cond_type, &RType::Bool, cond.location.clone())?;
    let result_type = least_common_parent_type(&true_type, &false_type).ok_or_else(|| {
        AnalysisError::new(AnalysisErrorKind::TypeMismatch, false_expr.location.clone())
    })?;
    Ok(result_type)
}

/// must evaluate to bool
fn check_returns_bool(expr: &LExpr, env: &Env) -> Result<(), AnalysisError> {
    let expr_type = tc_value_expr(expr, env)?;
    require_type(&expr_type, &RType::Bool, expr.location.clone())
}

fn tc_return(
    expr: Option<&LExpr>,
    stmt_location: &Location,
    env: &Env,
) -> Result<(), AnalysisError> {
    let ret_typ = match expr {
        Some(expr) => tc_expr(expr, env)?,
        None => RType::Void,
    };

    // if returning an expr, set the location to the expr's location
    // otherwise, set the location to the return statement
    let location = expr
        .map(|expr| expr.location.clone())
        .unwrap_or_else(|| stmt_location.clone());

    if !types_are_comparable_eq(&ret_typ, &env.expected_ret_type) {
        return Err(AnalysisError::new(
            AnalysisErrorKind::FunctionInvalidReturnType,
            location,
        ));
    }

    Ok(())
}

fn tc_call(name: &LIdent, args: &[Box<LExpr>], env: &Env) -> Result<RType, AnalysisError> {
    if env.variables.contains_key(&name.data) {
        return Err(AnalysisError::new(
            AnalysisErrorKind::VariableCalledAsFunction,
            name.location.clone(),
        ));
    }

    let function = env.functions.get(&name.data).ok_or_else(|| {
        AnalysisError::new(AnalysisErrorKind::FunctionNotFound, name.location.clone())
    })?;

    if function.params.len() != args.len() {
        return Err(AnalysisError::new(
            AnalysisErrorKind::FunctionArgsCountMismatch,
            name.location.clone(),
        ));
    }

    // check each arg and param type in step
    for (arg, param) in zip(args, &function.params) {
        let arg_type = tc_value_expr(arg, env)?;
        let param_type =
            env.resolve_non_void_type(&param.data.typ.data, param.data.typ.location.clone())?;
        if !types_are_comparable_eq(&arg_type, &param_type) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::FunctionArgsTypeMismatch,
                arg.location.clone(),
            ));
        }
    }

    env.resolve_type(&function.ret_typ.data, function.ret_typ.location.clone())
}

fn tc_value_expr(expr: &LExpr, env: &Env) -> Result<RType, AnalysisError> {
    // value type is basically non-void
    let typ = tc_expr(expr, env)?;

    if typ == RType::Void {
        return Err(AnalysisError::new(
            AnalysisErrorKind::VoidUsedAsValue,
            expr.location.clone(),
        ));
    }

    Ok(typ)
}

fn check_break_and_continue(program: &Program) -> Result<(), AnalysisError> {
    // basic traversal with variable for loop nesting depth
    for gdecl in &program.0 {
        match &gdecl.data {
            GlobalDecl::FunDef {
                body: Some(body), ..
            } => check_break_and_continue_block(body, 0)?,
            GlobalDecl::Typedef { .. }
            | GlobalDecl::FunDef { .. }
            | GlobalDecl::StructDef { .. } => continue,
        }
    }

    Ok(())
}

fn check_break_and_continue_block(block: &LBlock, loop_depth: usize) -> Result<(), AnalysisError> {
    for stmt in &block.data.0 {
        check_break_and_continue_stmt(stmt, loop_depth)?;
    }

    Ok(())
}

fn check_break_and_continue_stmt(stmt: &LStmt, loop_depth: usize) -> Result<(), AnalysisError> {
    match &stmt.data {
        Stmt::Break => {
            if loop_depth == 0 {
                Err(AnalysisError::new(
                    AnalysisErrorKind::InvalidBreak,
                    stmt.location.clone(),
                ))
            } else {
                Ok(())
            }
        }

        Stmt::Continue => {
            if loop_depth == 0 {
                Err(AnalysisError::new(
                    AnalysisErrorKind::InvalidContinue,
                    stmt.location.clone(),
                ))
            } else {
                Ok(())
            }
        }
        Stmt::Decl { block, .. } => check_break_and_continue_block(block, loop_depth),
        Stmt::Assign { .. }
        | Stmt::CompoundAssign { .. }
        | Stmt::Postfix { .. }
        | Stmt::Return(_)
        | Stmt::Expr(_)
        | Stmt::Assert(_) => Ok(()),
        Stmt::If {
            true_block,
            false_block,
            ..
        } => {
            check_break_and_continue_block(true_block, loop_depth)?;

            if let Some(false_block) = false_block {
                check_break_and_continue_block(false_block, loop_depth)?;
            }

            Ok(())
        }
        // increment loop depth for for and while loops
        Stmt::While { body, .. } => check_break_and_continue_block(body, loop_depth + 1),
        Stmt::For {
            init, body, step, ..
        } => {
            if let Some(init) = init {
                check_break_and_continue_stmt(init, loop_depth)?;
            }
            check_break_and_continue_block(body, loop_depth + 1)?;
            if let Some(step) = step {
                check_break_and_continue_stmt(step, loop_depth)?;
            }
            Ok(())
        }
        Stmt::Block(block) => check_break_and_continue_block(block, loop_depth),
    }
}

fn require_type(a: &RType, b: &RType, location: Location) -> Result<(), AnalysisError> {
    if !types_are_comparable_eq(a, b) {
        return Err(AnalysisError::new(
            AnalysisErrorKind::TypeMismatch,
            location,
        ));
    }

    Ok(())
}

fn types_are_comparable_eq(a: &RType, b: &RType) -> bool {
    match (a, b) {
        // null pointers can be any type of pointer
        (RType::Null, RType::Null)
        | (RType::Null, RType::Pointer(_))
        | (RType::Pointer(_), RType::Null) => true,
        _ => a == b,
    }
}

fn least_common_parent_type(a: &RType, b: &RType) -> Option<RType> {
    if a == b {
        Some(a.clone())
    } else {
        match (a, b) {
            (RType::Null, RType::Pointer(_)) => Some(b.clone()),
            (RType::Pointer(_), RType::Null) => Some(a.clone()),
            _ => None,
        }
    }
}

impl AnalysisError {
    pub fn new(kind: AnalysisErrorKind, location: Location) -> Self {
        // TODO: reorder nicely
        let description = match kind {
            AnalysisErrorKind::TypeMismatch => "type mismatch",
            AnalysisErrorKind::UseUninitializedVariable => "use of uninitialized variable",
            AnalysisErrorKind::NoReturn => "function does not return on all paths",
            AnalysisErrorKind::InvalidBreak => "break used outside of a loop",
            AnalysisErrorKind::InvalidContinue => "continue used outside of a loop",
            AnalysisErrorKind::FunctionInvalidReturnType => {
                "function returned a value of the wrong type"
            }
            AnalysisErrorKind::VoidUsedAsValue => "void value used where a value is required",
            AnalysisErrorKind::FunctionIsValue => "function used as a value",
            AnalysisErrorKind::VariableCalledAsFunction => "variable used as a function",
            AnalysisErrorKind::VariableShadowsFunction => "variable name shadows a function",
            AnalysisErrorKind::FunctionRedeclaration => "function is redeclared",
            AnalysisErrorKind::FunctionRedefinition => "function is redefined",
            AnalysisErrorKind::FunctionRepeatedParameterName => {
                "function has repeated parameter name"
            }
            AnalysisErrorKind::FunctionReturnTypeMismatch => {
                "function return type differs from previous declaration"
            }
            AnalysisErrorKind::FunctionParamCountMismatch => {
                "function parameter count differs from previous declaration"
            }
            AnalysisErrorKind::FunctionParamTypeMismatch => {
                "function parameter type differs from previous declaration"
            }
            AnalysisErrorKind::MainMissing => "program requires a main function",
            AnalysisErrorKind::MainNotDefined => "main must be defined",
            AnalysisErrorKind::MainReturnTypeNotInt => "main must return int",
            AnalysisErrorKind::MainInvalidParameters => "main must not have parameters",
            AnalysisErrorKind::FunctionArgsCountMismatch => {
                "function call has the wrong number of arguments"
            }
            AnalysisErrorKind::FunctionArgsTypeMismatch => {
                "function call has an argument of the wrong type"
            }
            AnalysisErrorKind::VariableRedeclaration => "variable redeclared",
            AnalysisErrorKind::VariableIsType => "variable name conflicts with a typedef",
            AnalysisErrorKind::VariableNotFound => "variable not found",
            AnalysisErrorKind::FunctionNotFound => "function not found",
            AnalysisErrorKind::UnknownType => "type not found",
            AnalysisErrorKind::VoidUsedAsVariableType => "void cannot be a parameter type",
            AnalysisErrorKind::TypedefAliasIsFunction => "typedef alias conflicts with a function",
            AnalysisErrorKind::TypedefTypeNotPrimitive => "typedef must name a primitive type",
            AnalysisErrorKind::TypedefRepeatedTypedef => "typedef redeclared",
            AnalysisErrorKind::StructRedefinition => "struct is redefined",
            AnalysisErrorKind::StructFieldRedeclaration => "struct field is redeclared",
            AnalysisErrorKind::StructFieldNotFound => "struct field not found",
            AnalysisErrorKind::StructEmpty => "struct must have at least one field",
            AnalysisErrorKind::FieldAccessOnNonStruct => "field access requires a struct value",
            AnalysisErrorKind::UndefinedStruct => "struct is undefined",
            AnalysisErrorKind::DereferenceNonPointer => "cannot dereference a non-pointer value",
            AnalysisErrorKind::StructValueNotAllowed => "struct values cannot be copied by value",
            AnalysisErrorKind::ArraysNotImplemented => {
                "arrays are not supported by semantic analysis yet"
            }
        };

        Self {
            kind,
            description: description.to_string(),
            location,
        }
    }
}

impl From<AnalysisError> for Diagnostic {
    fn from(err: AnalysisError) -> Self {
        Diagnostic {
            description: err.description,
            kind: DiagnosticKind::Error,
            location: err.location,
        }
    }
}

impl Env {
    pub fn variable_type(&self, name: &str) -> RType {
        self.variables.get(name).expect("checked variable").clone()
    }

    pub fn assign_type(&mut self, name: String, typ: &LTyp) {
        let resolved = self
            .resolve_type(&typ.data, typ.location.clone())
            .expect("checked type");
        self.variables.insert(name, resolved);
    }

    pub fn erase_variable(&mut self, name: &str) {
        self.variables.remove(name);
    }

    pub fn type_of(&self, typ: &LTyp) -> RType {
        self.resolve_type(&typ.data, typ.location.clone()).unwrap()
    }

    /// returns (fn parameter types, fn return type)
    pub fn function_type(&self, name: &str) -> (Vec<RType>, RType) {
        let function = self.functions.get(name).unwrap();
        let params = function
            .params
            .iter()
            .map(|param| self.type_of(&param.data.typ))
            .collect();
        (params, self.type_of(&function.ret_typ))
    }

    /// returns (size, alignment)
    pub fn layout(&self, typ: &RType) -> (i32, i32) {
        match typ {
            RType::Bool | RType::Int => (4, 4),
            RType::Pointer(_) | RType::Null => (8, 8),
            RType::Struct(name) => {
                let sinfo = self.structs.get(name).unwrap();
                (sinfo.size, sinfo.align)
            }
            RType::Void => panic!("void has no layout"),
        }
    }

    pub fn field_offset(&self, typ: &RType, field: &str) -> i32 {
        let RType::Struct(name) = typ else {
            panic!("checked field base")
        };
        // ok with panic
        self.structs[name].offsets[field]
    }

    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            structs: HashMap::new(),
            typedefs: HashMap::new(),
            variables: HashMap::new(),
            expected_ret_type: RType::Void,
        }
    }

    fn add_struct_decl(&mut self, name: &LIdent) {
        self.structs
            .entry(name.data.clone())
            .or_insert_with(|| StructInfo {
                fields: HashMap::new(),
                offsets: HashMap::new(),
                size: 0,
                align: 1,
                status: InitStatus::Declared,
            });
    }

    fn add_struct_def(
        &mut self,
        name: &LIdent,
        fields: &[LStructField],
    ) -> Result<(), AnalysisError> {
        if self
            .structs
            .get(&name.data)
            .is_some_and(|info| info.status == InitStatus::Defined)
        {
            return Err(AnalysisError::new(
                AnalysisErrorKind::StructRedefinition,
                name.location.clone(),
            ));
        }

        if fields.is_empty() {
            return Err(AnalysisError::new(
                AnalysisErrorKind::StructEmpty,
                name.location.clone(),
            ));
        }

        // declare the struct here so its fields can recursively refer to itself
        self.structs.insert(
            name.data.clone(),
            StructInfo {
                fields: HashMap::new(),
                offsets: HashMap::new(),
                size: 0,
                align: 1,
                status: InitStatus::Declared,
            },
        );

        let mut resolved_fields = HashMap::new();
        let mut offsets = HashMap::new();
        let mut size = 0;
        let mut align = 1;
        for field in fields {
            if resolved_fields.contains_key(&field.data.name.data) {
                return Err(AnalysisError::new(
                    AnalysisErrorKind::StructFieldRedeclaration,
                    field.data.name.location.clone(),
                ));
            }

            let typ =
                self.resolve_non_void_type(&field.data.typ.data, field.data.typ.location.clone())?;
            self.check_type_is_defined(&typ, field.data.typ.location.clone())?;
            let (field_size, field_align) = self.layout(&typ);
            size = align_to(size, field_align);
            offsets.insert(field.data.name.data.clone(), size);
            size += field_size;
            align = align.max(field_align);
            resolved_fields.insert(field.data.name.data.clone(), typ);
        }

        self.structs.insert(
            name.data.clone(),
            StructInfo {
                fields: resolved_fields,
                offsets,
                size: align_to(size, align),
                align,
                status: InitStatus::Defined,
            },
        );
        Ok(())
    }

    pub fn field_type(&self, base_type: &RType, field: &LIdent) -> Result<RType, AnalysisError> {
        let RType::Struct(struct_name) = base_type else {
            return Err(AnalysisError::new(
                AnalysisErrorKind::FieldAccessOnNonStruct,
                field.location.clone(),
            ));
        };

        let info = self.structs.get(struct_name).ok_or_else(|| {
            AnalysisError::new(AnalysisErrorKind::UndefinedStruct, field.location.clone())
        })?;
        if info.status != InitStatus::Defined {
            return Err(AnalysisError::new(
                AnalysisErrorKind::UndefinedStruct,
                field.location.clone(),
            ));
        }

        info.fields.get(&field.data).cloned().ok_or_else(|| {
            AnalysisError::new(
                AnalysisErrorKind::StructFieldNotFound,
                field.location.clone(),
            )
        })
    }

    fn check_type_is_defined(&self, typ: &RType, location: Location) -> Result<(), AnalysisError> {
        if let RType::Struct(name) = typ {
            match self.structs.get(name) {
                Some(info) if info.status == InitStatus::Defined => {}
                Some(_) => {
                    return Err(AnalysisError::new(
                        AnalysisErrorKind::UndefinedStruct,
                        location,
                    ));
                }
                None => return Err(AnalysisError::new(AnalysisErrorKind::UnknownType, location)),
            }
        }

        // assume all other types are good
        // functions are checked earlier, variables will be checked later
        Ok(())
    }

    fn add_function_decl(&mut self, function: FunctionInfo) -> Result<(), AnalysisError> {
        self.add_function(
            function,
            InitStatus::Declared,
            AnalysisErrorKind::FunctionRedeclaration,
        )
    }

    fn add_function_def(&mut self, function: FunctionInfo) -> Result<(), AnalysisError> {
        self.add_function(
            function,
            InitStatus::Defined,
            AnalysisErrorKind::FunctionRedefinition,
        )
    }

    fn add_function(
        &mut self,
        function: FunctionInfo,
        status: InitStatus,
        duplicate_kind: AnalysisErrorKind,
    ) -> Result<(), AnalysisError> {
        let function = self.resolve_function_types(function)?;
        self.check_function_name(&function)?;
        self.check_parameter_names(&function)?;
        self.check_parameter_variables(&function)?;

        if let Some(prev_function) = self.functions.get(&function.name.data) {
            self.check_function_signature_match(prev_function, &function)?;

            if prev_function.status == InitStatus::Defined {
                return Err(AnalysisError::new(
                    duplicate_kind,
                    function.name.location.clone(),
                ));
            }
        }

        self.functions.insert(
            function.name.data.clone(),
            FunctionInfo { status, ..function },
        );

        Ok(())
    }

    fn check_function_name(&self, function: &FunctionInfo) -> Result<(), AnalysisError> {
        // functions and typedefs are checked in the same computation
        // so their order matters, one or the order is not hoisted in relation to each other
        if self.typedefs.contains_key(&function.name.data) {
            // variables aren't populated yet so just check typedef
            return Err(AnalysisError::new(
                AnalysisErrorKind::TypedefAliasIsFunction,
                function.name.location.clone(),
            ));
        }

        Ok(())
    }

    /// resolve return type and parameter types
    /// function return types can be void
    /// parameter types cannot be void
    fn resolve_function_types(
        &self,
        function: FunctionInfo,
    ) -> Result<FunctionInfo, AnalysisError> {
        let ret_typ_location = function.ret_typ.location.clone();
        let ret_typ = self.resolve_type(&function.ret_typ.data, ret_typ_location.clone())?;
        self.check_type_is_defined(&ret_typ, ret_typ_location.clone())?;
        if matches!(ret_typ, RType::Struct(_)) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::StructValueNotAllowed,
                ret_typ_location,
            ));
        }

        for parameter in &function.params {
            let typ_location = parameter.data.typ.location.clone();
            let typ = self.resolve_non_void_type(&parameter.data.typ.data, typ_location.clone())?;
            self.check_type_is_defined(&typ, typ_location.clone())?;
            if matches!(typ, RType::Struct(_)) {
                return Err(AnalysisError::new(
                    AnalysisErrorKind::StructValueNotAllowed,
                    typ_location,
                ));
            }
        }

        Ok(function)
    }

    /// note: the parameter names of a function declaration do not have to match the parameter
    /// names of the same function's definition
    fn check_parameter_names(&self, function: &FunctionInfo) -> Result<(), AnalysisError> {
        let mut names = HashSet::new();

        for parameter in &function.params {
            if !names.insert(&parameter.data.name.data) {
                return Err(AnalysisError::new(
                    AnalysisErrorKind::FunctionRepeatedParameterName,
                    parameter.data.name.location.clone(),
                ));
            }
        }

        Ok(())
    }

    /// checks whether a parameter variable name is a typedef (a type)
    fn check_parameter_variables(&self, function: &FunctionInfo) -> Result<(), AnalysisError> {
        for parameter in &function.params {
            if self.typedefs.contains_key(&parameter.data.name.data) {
                return Err(AnalysisError::new(
                    AnalysisErrorKind::VariableIsType,
                    parameter.data.name.location.clone(),
                ));
            }
        }

        Ok(())
    }

    fn check_function_signature_match(
        &self,
        prev: &FunctionInfo,
        new: &FunctionInfo,
    ) -> Result<(), AnalysisError> {
        let prev_ret = self.resolve_type(&prev.ret_typ.data, prev.ret_typ.location.clone())?;
        let new_ret = self.resolve_type(&new.ret_typ.data, new.ret_typ.location.clone())?;
        if prev_ret != new_ret {
            return Err(AnalysisError::new(
                AnalysisErrorKind::FunctionReturnTypeMismatch,
                new.ret_typ.location.clone(),
            ));
        }
        if prev.params.len() != new.params.len() {
            let location = new.name.location.clone();
            return Err(AnalysisError::new(
                AnalysisErrorKind::FunctionParamCountMismatch,
                location,
            ));
        }

        // check params in order
        for (new_param, existing_param) in zip(&new.params, &prev.params) {
            let new_type = self.resolve_non_void_type(
                &new_param.data.typ.data,
                new_param.data.typ.location.clone(),
            )?;
            let existing_type = self.resolve_non_void_type(
                &existing_param.data.typ.data,
                existing_param.data.typ.location.clone(),
            )?;
            if new_type != existing_type {
                return Err(AnalysisError::new(
                    AnalysisErrorKind::FunctionParamTypeMismatch,
                    new_param.data.typ.location.clone(),
                ));
            }
        }

        Ok(())
    }

    fn add_typedef(&mut self, typ: &LTyp, alias: &LIdent) -> Result<(), AnalysisError> {
        // typedefs can only be from a primitive to a name
        // ex: typedef int A;
        //     typedef A B;
        //  is not allowed
        if matches!(typ.data, Typ::Alias(_)) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::TypedefTypeNotPrimitive,
                typ.location.clone(),
            ));
        }
        if self.functions.contains_key(&alias.data) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::TypedefAliasIsFunction,
                alias.location.clone(),
            ));
        }
        if self.typedefs.contains_key(&alias.data) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::TypedefRepeatedTypedef,
                alias.location.clone(),
            ));
        }

        let resolved_type = self.resolve_non_void_type(&typ.data, typ.location.clone())?;
        self.typedefs.insert(alias.data.clone(), resolved_type);
        Ok(())
    }

    /// Resolve aliases recursively and remove source locations from the semantic type.
    fn resolve_type(&self, typ: &Typ, location: Location) -> Result<RType, AnalysisError> {
        match typ {
            Typ::Int => Ok(RType::Int),
            Typ::Bool => Ok(RType::Bool),
            Typ::Void => Ok(RType::Void),
            Typ::Alias(alias) => self.typedefs.get(&alias.data).cloned().ok_or_else(|| {
                AnalysisError::new(AnalysisErrorKind::UnknownType, alias.location.clone())
            }),
            Typ::Struct(name) => {
                if self.structs.contains_key(&name.data) {
                    Ok(RType::Struct(name.data.clone()))
                } else {
                    Err(AnalysisError::new(AnalysisErrorKind::UnknownType, location))
                }
            }
            Typ::Pointer(pointee) => {
                let inner = self.resolve_type(&pointee.data, pointee.location.clone())?;
                Ok(RType::Pointer(Box::new(inner)))
            }
            Typ::Array(_) => Err(AnalysisError::new(
                AnalysisErrorKind::ArraysNotImplemented,
                location,
            )),
        }
    }

    fn resolve_non_void_type(&self, typ: &Typ, location: Location) -> Result<RType, AnalysisError> {
        let typ = self.resolve_type(typ, location.clone())?;

        if matches!(&typ, RType::Void) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::VoidUsedAsVariableType,
                location,
            ));
        }
        Ok(typ)
    }

    fn add_variable(
        &mut self,
        name: String,
        typ: &LTyp,
        location: Location,
    ) -> Result<(), AnalysisError> {
        if self.functions.contains_key(&name) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::VariableShadowsFunction,
                location,
            ));
        }

        if self.typedefs.contains_key(&name) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::VariableIsType,
                location,
            ));
        }

        if self.variables.contains_key(&name) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::VariableRedeclaration,
                location,
            ));
        }

        let typ_location = typ.location.clone();
        let resolved_type = self.resolve_non_void_type(&typ.data, typ_location.clone())?;
        self.check_type_is_defined(&resolved_type, typ_location)?;
        if matches!(&resolved_type, RType::Struct(_)) {
            return Err(AnalysisError::new(
                AnalysisErrorKind::StructValueNotAllowed,
                typ.location.clone(),
            ));
        }
        self.variables.insert(name, resolved_type);
        Ok(())
    }
}
