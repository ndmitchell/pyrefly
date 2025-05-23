/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::mem;

use itertools::Either;
use ruff_python_ast::AnyParameterRef;
use ruff_python_ast::ExceptHandler;
use ruff_python_ast::Expr;
use ruff_python_ast::Parameters;
use ruff_python_ast::Stmt;
use ruff_python_ast::StmtExpr;
use ruff_python_ast::StmtFunctionDef;
use ruff_text_size::Ranged;

use crate::binding::binding::AnnotationTarget;
use crate::binding::binding::Binding;
use crate::binding::binding::BindingAnnotation;
use crate::binding::binding::BindingFunction;
use crate::binding::binding::BindingYield;
use crate::binding::binding::BindingYieldFrom;
use crate::binding::binding::FunctionSource;
use crate::binding::binding::IsAsync;
use crate::binding::binding::Key;
use crate::binding::binding::KeyAnnotation;
use crate::binding::binding::KeyClass;
use crate::binding::binding::KeyFunction;
use crate::binding::binding::KeyYield;
use crate::binding::binding::KeyYieldFrom;
use crate::binding::binding::LastStmt;
use crate::binding::binding::ReturnExplicit;
use crate::binding::binding::ReturnImplicit;
use crate::binding::binding::ReturnType;
use crate::binding::bindings::BindingsBuilder;
use crate::binding::bindings::FuncInfo;
use crate::binding::bindings::LegacyTParamBuilder;
use crate::binding::scope::FlowStyle;
use crate::binding::scope::Scope;
use crate::binding::scope::ScopeKind;
use crate::graph::index::Idx;
use crate::module::short_identifier::ShortIdentifier;
use crate::ruff::ast::Ast;
use crate::sys_info::SysInfo;
use crate::util::prelude::VecExt;

impl<'a> BindingsBuilder<'a> {
    fn parameters(
        &mut self,
        x: &mut Parameters,
        function_idx: Idx<KeyFunction>,
        class_key: Option<Idx<KeyClass>>,
    ) {
        let mut self_name = None;
        for x in x.iter_non_variadic_params() {
            if class_key.is_some() && self_name.is_none() {
                self_name = Some(x.parameter.name.clone());
            }
            self.bind_function_param(
                AnnotationTarget::Param(x.parameter.name.id.clone()),
                AnyParameterRef::NonVariadic(x),
                function_idx,
                class_key,
            );
        }
        if let Some(box args) = &x.vararg {
            self.bind_function_param(
                AnnotationTarget::ArgsParam(args.name.id.clone()),
                AnyParameterRef::Variadic(args),
                function_idx,
                class_key,
            );
        }
        if let Some(box kwargs) = &x.kwarg {
            self.bind_function_param(
                AnnotationTarget::KwargsParam(kwargs.name.id.clone()),
                AnyParameterRef::Variadic(kwargs),
                function_idx,
                class_key,
            );
        }
        if let Scope {
            kind: ScopeKind::Method(method),
            ..
        } = self.scopes.current_mut()
        {
            method.self_name = self_name;
        }
    }

    pub fn function_def(&mut self, mut x: StmtFunctionDef) {
        // Get preceding function definition, if any. Used for building an overload type.
        let mut pred_idx = None;
        let mut pred_function_idx = None;
        if let Some(flow) = self.scopes.current().flow.info.get(&x.name.id) {
            if let FlowStyle::FunctionDef(fidx, _) = flow.style {
                pred_idx = Some(flow.key);
                pred_function_idx = Some(fidx);
            }
        }

        let body = mem::take(&mut x.body);
        let decorators = self.ensure_and_bind_decorators(mem::take(&mut x.decorator_list));
        let source = if is_ellipse(&body) {
            FunctionSource::Stub
        } else {
            FunctionSource::Impl
        };
        let mut return_annotation = mem::take(&mut x.returns);
        self.functions.push(FuncInfo::default());

        let func_name = x.name.clone();
        let (class_key, class_meta) = match &self.scopes.current().kind {
            ScopeKind::ClassBody(body) => (
                Some(self.table.classes.0.insert(body.as_class_key())),
                Some(
                    self.table
                        .class_metadata
                        .0
                        .insert(body.as_class_metadata_key()),
                ),
            ),
            _ => (None, None),
        };

        self.scopes.push(Scope::annotation(x.range));

        let tparams = x
            .type_params
            .as_mut()
            .map(|tparams| self.type_params(tparams));

        let mut legacy = Some(LegacyTParamBuilder::new(tparams.is_some()));

        // We need to bind all the parameters expressions _after_ the type params, but before the parameter names,
        // which might shadow some types.
        for (param, default) in Ast::parameters_iter_mut(&mut x.parameters) {
            self.ensure_type_opt(param.annotation.as_deref_mut(), &mut legacy);
            if let Some(default) = default {
                self.ensure_expr_opt(default.as_deref_mut());
            }
        }
        self.ensure_type_opt(return_annotation.as_deref_mut(), &mut legacy);

        let return_ann_range = return_annotation.map(|x| {
            (
                x.range(),
                self.table.insert(
                    KeyAnnotation::ReturnAnnotation(ShortIdentifier::new(&func_name)),
                    BindingAnnotation::AnnotateExpr(
                        AnnotationTarget::Return(func_name.id.clone()),
                        *x,
                        class_key,
                    ),
                ),
            )
        });
        let return_ann = return_ann_range.as_ref().map(|(_, key)| *key);

        // Collect the keys of terminal expressions. Used to determine the implicit return type.
        let last_exprs = function_last_expressions(&body, self.sys_info);
        let last_expr_keys = last_exprs.map(|x| {
            x.into_map(|(last, x)| {
                (
                    last,
                    self.table.types.0.insert(match last {
                        LastStmt::Expr => Key::StmtExpr(x.range()),
                        LastStmt::With(_) => Key::ContextExpr(x.range()),
                    }),
                )
            })
            .into_boxed_slice()
        });

        let legacy_tparam_builder = legacy.unwrap();
        legacy_tparam_builder.add_name_definitions(self);

        if class_key.is_none() {
            self.scopes.push(Scope::function(x.range));
        } else {
            self.scopes.push(Scope::method(x.range, func_name.clone()));
        }

        let legacy_tparams = legacy_tparam_builder.lookup_keys();

        let function_idx = self
            .table
            .functions
            .0
            .insert(KeyFunction(ShortIdentifier::new(&func_name)));
        self.parameters(&mut x.parameters, function_idx, class_key);

        self.init_static_scope(&body, false);
        self.stmts(body);
        let func_scope = self.scopes.pop();
        self.scopes.pop();

        if let ScopeKind::Method(method) = func_scope.kind
            && let ScopeKind::ClassBody(body) = &mut self.scopes.current_mut().kind
        {
            body.instance_attributes_by_method
                .insert(method.name.id, method.instance_attributes);
        }
        let is_async = x.is_async;

        let accumulate = self.functions.pop().unwrap();
        let is_generator = !accumulate.yields.is_empty();

        // Implicit return
        let implicit_return = self.table.insert(
            Key::ReturnImplicit(ShortIdentifier::new(&func_name)),
            Binding::ReturnImplicit(ReturnImplicit {
                last_exprs: last_expr_keys,
                function_source: source,
                decorators: decorators.clone().into_boxed_slice(),
            }),
        );

        // Collect the keys of explicit returns.
        let return_keys = accumulate
            .returns
            .into_map(|x| {
                self.table.insert(
                    Key::ReturnExplicit(x.range),
                    Binding::ReturnExplicit(ReturnExplicit {
                        annot: return_ann,
                        expr: x.value,
                        is_generator,
                        is_async,
                    }),
                )
            })
            .into_boxed_slice();

        // Collect the keys of yield expressions.
        let yield_keys = accumulate
            .yields
            .into_map(|x| match x {
                Either::Left(x) => {
                    // Add binding to get the send type for a yield expression.
                    Either::Left(
                        self.table
                            .insert(KeyYield(x.range), BindingYield::Yield(return_ann, x)),
                    )
                }
                Either::Right(x) => {
                    // Add binding to get the return type for a yield from expression.
                    Either::Right(self.table.insert(
                        KeyYieldFrom(x.range),
                        BindingYieldFrom::YieldFrom(return_ann, x),
                    ))
                }
            })
            .into_boxed_slice();

        self.table.insert(
            Key::ReturnType(ShortIdentifier::new(&func_name)),
            Binding::ReturnType(ReturnType {
                annot: return_ann_range,
                returns: return_keys,
                implicit_return,
                yields: yield_keys,
                is_async: x.is_async,
            }),
        );

        let function_idx = self.table.insert(
            KeyFunction(ShortIdentifier::new(&func_name)),
            BindingFunction {
                def: x,
                source,
                class_key,
                decorators: decorators.into_boxed_slice(),
                legacy_tparams: legacy_tparams.into_boxed_slice(),
                successor: None,
            },
        );

        if let Some(pred_function_idx) = pred_function_idx {
            let pred_binding = self.table.functions.1.get_mut(pred_function_idx).unwrap();
            pred_binding.successor = Some(function_idx);
        }

        self.bind_definition(
            &func_name,
            Binding::Function(function_idx, pred_idx, class_meta),
            FlowStyle::FunctionDef(function_idx, return_ann.is_some()),
        );
    }
}

/// Given the body of a function, what are the potential expressions that
/// could be the last ones to be executed, where the function then falls off the end.
///
/// * Return None to say there are branches that fall off the end always.
/// * Return Some([]) to say that we can never reach the end (e.g. always return, raise)
/// * Return Some(xs) to say this set might be the last expression.
fn function_last_expressions<'a>(
    x: &'a [Stmt],
    sys_info: &SysInfo,
) -> Option<Vec<(LastStmt, &'a Expr)>> {
    fn f<'a>(sys_info: &SysInfo, x: &'a [Stmt], res: &mut Vec<(LastStmt, &'a Expr)>) -> Option<()> {
        match x.last()? {
            Stmt::Expr(x) => res.push((LastStmt::Expr, &x.value)),
            Stmt::Return(_) | Stmt::Raise(_) => {}
            Stmt::With(x) => {
                let kind = IsAsync::new(x.is_async);
                for y in &x.items {
                    res.push((LastStmt::With(kind), &y.context_expr));
                }
                f(sys_info, &x.body, res)?;
            }
            Stmt::If(x) => {
                let mut last_test = None;
                for (test, body) in sys_info.pruned_if_branches(x) {
                    last_test = test;
                    f(sys_info, body, res)?;
                }
                if last_test.is_some() {
                    // The final `if` can fall through, so the `if` itself might be the last statement.
                    return None;
                }
            }
            Stmt::Try(x) => {
                if !x.finalbody.is_empty() {
                    f(sys_info, &x.finalbody, res)?;
                } else {
                    if x.orelse.is_empty() {
                        f(sys_info, &x.body, res)?;
                    } else {
                        f(sys_info, &x.orelse, res)?;
                    }
                    for handler in &x.handlers {
                        match handler {
                            ExceptHandler::ExceptHandler(x) => f(sys_info, &x.body, res)?,
                        }
                    }
                    // If we don't have a matching handler, we raise an exception, which is fine.
                }
            }
            _ => return None,
        }
        Some(())
    }

    let mut res = Vec::new();
    f(sys_info, x, &mut res)?;
    Some(res)
}

fn is_docstring(x: &Stmt) -> bool {
    match x {
        Stmt::Expr(StmtExpr {
            value: box Expr::StringLiteral(..),
            ..
        }) => true,
        _ => false,
    }
}

fn is_ellipse(x: &[Stmt]) -> bool {
    match x.iter().find(|x| !is_docstring(x)) {
        Some(Stmt::Expr(StmtExpr {
            value: box Expr::EllipsisLiteral(_),
            ..
        })) => true,
        _ => false,
    }
}
