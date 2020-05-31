use crate::renderer::Render;
use crate::value::visitors;
use crate::value::Value;
use std::io::Write;

pub trait Evaluate {
    fn evaluate(&self) -> Value;
}
#[derive(Debug)]
pub enum BinaryOperation {
    Plus,
    Minus,
    Mul,
    Div,
    DivInteger,
    Modulo,
    Pow,
    LogicalOr,
    LogicalAnd,
    LogicalEq,
    LogicalNe,
    LogicalLt,
    LogicalGt,
    LogicalGe,
    LogicalLe,
    StringConcat,
}

#[derive(Debug)]
pub enum UnaryOperation {
    Plus,
    Minus,
    LogicalNot,
}
pub struct SubscriptExpression<'a> {
    expression: Box<Expression<'a>>,
    subscript_expression: Vec<Box<dyn Evaluate + 'a>>,
}
pub enum Expression<'a> {
    Constant(Value),
    BinaryExpression(BinaryOperation, Box<Expression<'a>>, Box<Expression<'a>>),
    UnaryExpression(UnaryOperation, Box<Expression<'a>>),
    SubscriptExpression(SubscriptExpression<'a>),
}

impl<'a> SubscriptExpression<'a> {
    pub fn new(expression: Box<Expression<'a>>) -> Self {
        let subscript_expression = vec![];
        Self {
            expression,
            subscript_expression,
        }
    }
    pub fn add_index(&mut self, subscript: Box<dyn Evaluate + 'a>) {
        self.subscript_expression.push(subscript);
    }
}
impl<'a> Evaluate for SubscriptExpression<'a> {
    fn evaluate(&self) -> Value {
        let mut cur = self.expression.evaluate();
        for idx in &self.subscript_expression {
            let subscript = idx.evaluate();
            cur = visitors::Subscription::apply(cur, subscript);
        }

        cur
    }
}
impl<'a> Evaluate for Expression<'a> {
    fn evaluate(&self) -> Value {
        match &self {
            Expression::Constant(value) => value.clone(),
            Expression::BinaryExpression(op, left, right) => {
                let left_val = left.evaluate();
                let right_val = right.evaluate();
                visitors::BinaryMathOperation::apply(op, left_val, right_val)
            }
            Expression::UnaryExpression(op, expr) => {
                let expression = expr.evaluate();
                match op {
                    UnaryOperation::Plus => expression,
                    UnaryOperation::Minus => -expression,
                    UnaryOperation::LogicalNot => !expression,
                }
            }
            Expression::SubscriptExpression(sub) => sub.evaluate(),
        }
    }
}
pub struct FullExpressionEvaluator<'a> {
    expression: Option<Expression<'a>>,
}

impl<'a> Render for FullExpressionEvaluator<'a> {
    fn render(&self, out: &mut dyn Write) {
        let value = self.evaluate();
        out.write(value.to_string().as_bytes());
    }
}

impl<'a> FullExpressionEvaluator<'a> {
    pub fn new() -> Self {
        Self { expression: None }
    }

    pub fn set_expression(&mut self, expression: Expression<'a>) {
        self.expression = Some(expression)
    }
}

impl<'a> Evaluate for FullExpressionEvaluator<'a> {
    fn evaluate(&self) -> Value {
        match &self.expression {
            Some(expression) => expression.evaluate(),
            None => Value::default(),
        }
    }
}
