use crate::expression_evaluator::FullExpressionEvaluator;
use crate::value::ValuesMap;
use std::fmt;
use std::io::Write;
use std::sync::RwLock;

pub struct ComposedRenderer<'a> {
    renderers: RwLock<Vec<Box<dyn Render + 'a>>>,
}

pub trait Render {
    fn render(&self, out: &mut dyn Write, params: &ValuesMap);
}

impl<'a> ComposedRenderer<'a> {
    pub fn new() -> Self {
        let renderers = RwLock::new(vec![]);
        Self { renderers }
    }
    pub fn add_renderer(&self, renderer: Box<dyn Render + 'a>) {
        self.renderers.write().unwrap().push(renderer)
    }
}

impl<'a> Render for ComposedRenderer<'a> {
    fn render(&self, out: &mut dyn Write, params: &ValuesMap) {
        for r in self.renderers.read().unwrap().iter() {
            r.render(out, params)
        }
    }
}

impl<'a> fmt::Debug for ComposedRenderer<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ComposedRenderer")
    }
}

#[derive(Debug)]
pub struct RawTextRenderer<'a> {
    content: &'a str,
}

impl<'a> RawTextRenderer<'a> {
    pub fn new(content: &'a str) -> Self {
        Self { content }
    }
}

impl<'a> Render for RawTextRenderer<'a> {
    fn render(&self, out: &mut dyn Write, params: &ValuesMap) {
        out.write(self.content.as_bytes());
    }
}

pub struct ExpressionRenderer<'a> {
    expression: FullExpressionEvaluator<'a>,
}

impl<'a> Render for ExpressionRenderer<'a> {
    fn render(&self, out: &mut dyn Write, params: &ValuesMap) {
        self.expression.render(out, params);
    }
}

impl<'a> ExpressionRenderer<'a> {
    pub fn new(expression: FullExpressionEvaluator<'a>) -> Self {
        Self { expression }
    }
}
