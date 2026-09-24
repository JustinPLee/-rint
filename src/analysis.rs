// ast -> ast

use std::collections::{HashMap, HashSet};
use std::iter::zip;

use crate::ast::{
    BinOp, Expr, GlobalDecl, LBinOp, LBlock, LExpr, LGlobalDecl, LIdent, LParam, LProgram, LStmt,
    LTyp, Program, Stmt, Typ,
};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::location::{Location, loc};

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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum FunctionStatus {
    Declared,
    Defined,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FunctionInfo {
    name: LIdent,
    params: Vec<LParam>,
    ret_typ: LTyp,
    status: FunctionStatus,
}

#[derive(Clone, Debug)]
struct Env {
    functions: HashMap<String, FunctionInfo>,
    typedefs: HashMap<String, Typ>,
    variables: HashMap<String, Typ>,
    expected_ret_type: Typ,
}

pub fn semantic_analysis(program: &LProgram) -> Result<(), AnalysisError> {
    let mut env = Env::new();

    // keep this first!
    process_gdecls(&program.data, &mut env)?;

    check_main(&env, program.location.clone())?;
    check_break_and_continue(&program.data)?;
    typecheck(&program.data, &env)?;

    Ok(())
}

fn check_main(env: &Env, location: Location) -> Result<(), AnalysisError> {
    let Some(main) = env.functions.get("main") else {
        return Err(AnalysisError::new(AnalysisErrorKind::MainMissing, location));
    };

    if main.status != FunctionStatus::Defined {
        return Err(AnalysisError::new(
            AnalysisErrorKind::MainNotDefined,
            main.name.location.clone(),
        ));
    }

    if main.ret_typ.data != Typ::Int {
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
                    status: FunctionStatus::Declared,
                };

                if body.is_some() {
                    env.add_function_def(function)?;
                } else {
                    env.add_function_decl(function)?;
                }
            }
        }
    }

    Ok(())
}

fn typecheck(program: &Program, env: &Env) -> Result<(), AnalysisError> {
    for gdecl in &program.0 {
        tc_global_decl(gdecl, env)?;
    }

    Ok(())
}

fn tc_global_decl(gdecl: &LGlobalDecl, env: &Env) -> Result<(), AnalysisError> {
    match &gdecl.data {
        GlobalDecl::Typedef { .. } => Ok(()), // assume checked in the gdecl pass
        GlobalDecl::FunDef {
            ret_typ,
            params,
            body,
            ..
        } => tc_fundef(ret_typ, params, body, env),
    }
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
    function_env.expected_ret_type =
        env.resolve_type(&ret_typ.data, true, ret_typ.location.clone())?;

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

fn tc_assign(name: &LIdent, value: &LExpr, env: &Env) -> Result<(), AnalysisError> {
    let value_type = tc_value_expr(value, env)?;
    let variable_type = env.variables.get(&name.data).ok_or_else(|| {
        AnalysisError::new(AnalysisErrorKind::VariableNotFound, name.location.clone())
    })?;

    require_type(&value_type, variable_type, value.location.clone())
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

fn tc_expr(expr: &LExpr, env: &Env) -> Result<Typ, AnalysisError> {
    // delegates again
    match &expr.data {
        Expr::Int(_) => Ok(Typ::Int),
        Expr::True | Expr::False => Ok(Typ::Bool),
        Expr::Ident(name) => tc_ident(name, &expr.location, env),
        Expr::BinOp { op, lhs, rhs } => tc_binop(op, lhs, rhs, env),
        Expr::TernOp {
            cond,
            true_expr,
            false_expr,
        } => tc_ternop(cond, true_expr, false_expr, env),
        Expr::FunCall { name, args } => tc_call(name, args, env),
    }
}

fn tc_ident(name: &str, location: &Location, env: &Env) -> Result<Typ, AnalysisError> {
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

fn tc_binop(op: &LBinOp, lhs: &LExpr, rhs: &LExpr, env: &Env) -> Result<Typ, AnalysisError> {
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
            require_type(&lhs_type, &Typ::Int, lhs.location.clone())?;
            require_type(&rhs_type, &Typ::Int, rhs.location.clone())?;
            Ok(Typ::Int)
        }
        BinOp::Greater | BinOp::GreaterEq | BinOp::Less | BinOp::LessEq => {
            require_type(&lhs_type, &Typ::Int, lhs.location.clone())?;
            require_type(&rhs_type, &Typ::Int, rhs.location.clone())?;
            Ok(Typ::Bool)
        }
        BinOp::LogicAnd | BinOp::LogicOr => {
            require_type(&lhs_type, &Typ::Bool, lhs.location.clone())?;
            require_type(&rhs_type, &Typ::Bool, rhs.location.clone())?;
            Ok(Typ::Bool)
        }
        BinOp::EqualEq | BinOp::NotEq => {
            require_type(&lhs_type, &rhs_type, rhs.location.clone())?;
            Ok(Typ::Bool)
        }
    }
}

fn tc_ternop(
    cond: &LExpr,
    true_expr: &LExpr,
    false_expr: &LExpr,
    env: &Env,
) -> Result<Typ, AnalysisError> {
    let cond_type = tc_value_expr(cond, env)?;
    let true_type = tc_value_expr(true_expr, env)?;
    let false_type = tc_value_expr(false_expr, env)?;

    require_type(&cond_type, &Typ::Bool, cond.location.clone())?;
    require_type(&true_type, &false_type, false_expr.location.clone())?;

    Ok(true_type)
}

/// must evaluate to bool
fn check_returns_bool(expr: &LExpr, env: &Env) -> Result<(), AnalysisError> {
    let expr_type = tc_value_expr(expr, env)?;
    require_type(&expr_type, &Typ::Bool, expr.location.clone())
}

fn tc_return(
    expr: Option<&LExpr>,
    stmt_location: &Location,
    env: &Env,
) -> Result<(), AnalysisError> {
    let ret_typ = match expr {
        Some(expr) => tc_expr(expr, env)?,
        None => Typ::Void,
    };

    // if returning an expr, set the location to the expr's location
    // otherwise, set the location to the return statement
    let location = expr
        .map(|expr| expr.location.clone())
        .unwrap_or_else(|| stmt_location.clone());

    if ret_typ != env.expected_ret_type {
        return Err(AnalysisError::new(
            AnalysisErrorKind::FunctionInvalidReturnType,
            location,
        ));
    }

    Ok(())
}

fn tc_call(name: &LIdent, args: &[Box<LExpr>], env: &Env) -> Result<Typ, AnalysisError> {
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
        if arg_type != param.data.typ.data {
            return Err(AnalysisError::new(
                AnalysisErrorKind::FunctionArgsTypeMismatch,
                arg.location.clone(),
            ));
        }
    }

    Ok(function.ret_typ.data.clone())
}

fn tc_value_expr(expr: &LExpr, env: &Env) -> Result<Typ, AnalysisError> {
    // value type is basically non-void
    let typ = tc_expr(expr, env)?;

    if typ == Typ::Void {
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
            GlobalDecl::Typedef { .. } | GlobalDecl::FunDef { .. } => continue,
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
        Stmt::Assign { .. } | Stmt::Return(_) | Stmt::Expr(_) | Stmt::Assert(_) => Ok(()),
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

fn require_type(a: &Typ, b: &Typ, location: Location) -> Result<(), AnalysisError> {
    if a != b {
        return Err(AnalysisError::new(
            AnalysisErrorKind::TypeMismatch,
            location,
        ));
    }

    Ok(())
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
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            typedefs: HashMap::new(),
            variables: HashMap::new(),
            expected_ret_type: Typ::Void,
        }
    }

    fn add_function_decl(&mut self, function: FunctionInfo) -> Result<(), AnalysisError> {
        self.add_function(
            function,
            FunctionStatus::Declared,
            AnalysisErrorKind::FunctionRedeclaration,
        )
    }

    fn add_function_def(&mut self, function: FunctionInfo) -> Result<(), AnalysisError> {
        self.add_function(
            function,
            FunctionStatus::Defined,
            AnalysisErrorKind::FunctionRedefinition,
        )
    }

    fn add_function(
        &mut self,
        function: FunctionInfo,
        status: FunctionStatus,
        duplicate_kind: AnalysisErrorKind,
    ) -> Result<(), AnalysisError> {
        let function = self.resolve_function_types(function)?;
        self.check_function_name(&function)?;
        self.check_parameter_names(&function)?;
        self.check_parameter_variables(&function)?;

        if let Some(prev_function) = self.functions.get(&function.name.data) {
            self.check_function_signature_match(prev_function, &function)?;

            if prev_function.status == FunctionStatus::Defined {
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
        mut function: FunctionInfo,
    ) -> Result<FunctionInfo, AnalysisError> {
        let ret_typ_location = function.ret_typ.location.clone();
        let ret_typ = self.resolve_type(&function.ret_typ.data, true, ret_typ_location.clone())?;
        function.ret_typ = loc(ret_typ, ret_typ_location);

        for parameter in &mut function.params {
            let typ_location = parameter.data.typ.location.clone();
            let typ = self.resolve_type(&parameter.data.typ.data, false, typ_location.clone())?;
            parameter.data.typ = loc(typ, typ_location);
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
        if prev.ret_typ.data != new.ret_typ.data {
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
            if new_param.data.typ.data != existing_param.data.typ.data {
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
        if matches!(typ.data, Typ::Named(_)) {
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

        self.typedefs.insert(alias.data.clone(), typ.data.clone());
        Ok(())
    }

    /// use the typedef mapping to get the underlying type
    fn reduce_type(&self, typ: &Typ, location: Location) -> Result<Typ, AnalysisError> {
        match typ {
            Typ::Named(alias) => self
                .typedefs
                .get(alias)
                .cloned()
                .ok_or_else(|| AnalysisError::new(AnalysisErrorKind::UnknownType, location)),
            primitive => Ok(primitive.clone()),
        }
    }

    /// get underlying type and check if void is valid
    fn resolve_type(
        &self,
        typ: &Typ,
        allow_void: bool,
        location: Location,
    ) -> Result<Typ, AnalysisError> {
        let typ = self.reduce_type(typ, location.clone())?;

        if !allow_void && matches!(typ, Typ::Void) {
            unreachable!("checked by parser??");
            // return Err(AnalysisError::new(
            //     AnalysisErrorKind::VoidUsedAsVariableType,
            //     location,
            // ));
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

        let typ = self.resolve_type(&typ.data, false, typ.location.clone())?;
        self.variables.insert(name, typ);
        Ok(())
    }
}
