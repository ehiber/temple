use crate::error::{Error, ErrorKind, Result, SourceLocation};
use crate::expression_parser::ExpressionParser;
use crate::keyword::{RegexEnum, ROUGH_TOKENIZER};
use crate::lexer::Token;
use crate::renderer::{ComposedRenderer, RawTextRenderer};
use crate::statement::parser::StatementParser;
use crate::statement::{StatementInfo, StatementInfoList, StatementInfoType};
use crate::template_env::TemplateEnv;
use regex::Regex;
use std::rc::Rc;
use std::sync::RwLock;

#[derive(Debug)]
pub struct TemplateParser<'a, 'b> {
    template_body: RwLock<&'a str>,
    env: RwLock<&'b TemplateEnv>,
    rough_tokenizer: Regex,
    text_blocks: RwLock<Vec<TextBlockInfo>>,
    current_block_info: RwLock<TextBlockInfo>,
    lines: RwLock<Vec<LineInfo>>,
    current_line_info: RwLock<LineInfo>,
}

impl<'a, 'b> TemplateParser<'a, 'b> {
    pub fn new(body: &'a str, env: &'b TemplateEnv) -> Result<Self> {
        let rough_tokenizer = Regex::new(&ROUGH_TOKENIZER[..ROUGH_TOKENIZER.len() - 1]).unwrap();

        Ok(Self {
            template_body: RwLock::new(body),
            env: RwLock::new(env),
            rough_tokenizer,
            text_blocks: RwLock::new(vec![]),
            current_block_info: RwLock::new(TextBlockInfo::default()),
            lines: RwLock::new(vec![]),
            current_line_info: RwLock::new(LineInfo::default()),
        })
    }

    fn fine_parsing(&self, renderer: Rc<ComposedRenderer<'a>>) -> Result<()> {
        let mut statements_stack: StatementInfoList = vec![];
        let root = StatementInfo::new(StatementInfoType::TemplateRoot, Token::Unknown, renderer);
        statements_stack.push(root);
        for orig_block in self.text_blocks.read().unwrap().iter() {
            match orig_block.mode {
                TextBlockType::RawBlock | TextBlockType::RawText => {
                    if orig_block.range.size() == 0 {
                        continue;
                    }
                    let text = self.template_body.read().unwrap();
                    let new_renderer =
                        RawTextRenderer::new(&text[orig_block.range.start..orig_block.range.end]);
                    statements_stack
                        .last()
                        .unwrap()
                        .current_composition
                        .add_renderer(Box::new(new_renderer));
                }
                TextBlockType::Expression => {
                    let text = self.template_body.read().unwrap();
                    let new_renderer = ExpressionParser::parse(
                        &text[orig_block.range.start..orig_block.range.end],
                    )?;
                    statements_stack
                        .last()
                        .unwrap()
                        .current_composition
                        .add_renderer(Box::new(new_renderer));
                }
                TextBlockType::Comment => {}
                TextBlockType::Statement | TextBlockType::LineStatement => {
                    let text = self.template_body.read().unwrap();
                    StatementParser::parse(
                        &text[orig_block.range.start..orig_block.range.end],
                        &mut statements_stack,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn parse(&mut self) -> Result<ComposedRenderer<'a>> {
        match self.rough_parsing() {
            Ok(_) => {
                let renderer = Rc::new(ComposedRenderer::new());
                self.fine_parsing(renderer.clone())?;
                Ok(Rc::try_unwrap(renderer).unwrap())
            }
            Err(error) => Err(error),
        }
    }

    fn rough_parsing(&mut self) -> Result<()> {
        let match_begin = self
            .rough_tokenizer
            .captures_iter(&self.template_body.read().unwrap());

        for capture in match_begin {
            // This does not seem idiomatic to rust
            let mut match_start = 0;
            let mut n_regex = 0;
            let mut match_end = 0;

            for i in 1..10 {
                if let Some(m) = capture.get(i) {
                    n_regex = i - 1;
                    match_start = m.start();
                    match_end = m.end();
                    break;
                };
            }

            match RegexEnum::n(n_regex).unwrap() {
                RegexEnum::NewLine => {
                    self.finish_current_line(match_start);

                    let new_line_start = self.current_line_info.read().unwrap().range.end + 1;
                    self.current_line_info.write().unwrap().range.start = new_line_start;

                    if self.current_line_info.read().unwrap().range.start
                        < self.template_body.read().unwrap().len()
                    {
                        match self.current_block_info.read().unwrap().mode {
                            TextBlockType::LineStatement => {}
                            _ => continue,
                        };
                        self.finish_current_block(match_start, TextBlockType::RawText, None);
                        self.current_block_info.write().unwrap().range.start =
                            self.current_line_info.read().unwrap().range.start;
                    }
                }
                RegexEnum::CommentBegin => {
                    match self.current_block_info.read().unwrap().mode {
                        TextBlockType::RawBlock => continue,
                        TextBlockType::RawText => {}
                        _ => {
                            self.finish_current_line(match_end);
                            return Err(Error::from(ErrorKind::UnexpectedCommentBegin(
                                SourceLocation::new(match_start, match_end),
                            )));
                        }
                    };

                    self.finish_current_block(match_start, TextBlockType::Comment, None);

                    self.current_block_info.write().unwrap().range.start = match_end;
                    self.current_block_info.write().unwrap().mode = TextBlockType::Comment;
                }

                RegexEnum::CommentEnd => {
                    match self.current_block_info.read().unwrap().mode {
                        TextBlockType::RawBlock => continue,
                        TextBlockType::Comment => {}
                        _ => {
                            self.finish_current_line(match_end);
                            return Err(Error::from(ErrorKind::UnexpectedCommentEnd(
                                SourceLocation::new(match_start, match_end),
                            )));
                        }
                    };
                    self.current_block_info.write().unwrap().range.start =
                        self.finish_current_block(match_start, TextBlockType::RawText, None);
                }

                RegexEnum::ExprBegin => {
                    self.start_control_block(TextBlockType::Expression, match_start, match_end);
                }
                RegexEnum::ExprEnd => {
                    match self.current_block_info.read().unwrap().mode {
                        TextBlockType::RawText => {
                            self.finish_current_line(match_end);
                            return Err(Error::from(ErrorKind::UnexpectedExprEnd(
                                SourceLocation::new(match_start, match_end),
                            )));
                        }
                        TextBlockType::Expression => {}
                        _ => {
                            continue;
                        }
                    };
                    self.current_block_info.write().unwrap().range.start =
                        self.finish_current_block(match_start, TextBlockType::RawText, None);
                }
                RegexEnum::StmtBegin => {
                    self.start_control_block(TextBlockType::Statement, match_start, match_end);
                }
                RegexEnum::StmtEnd => {
                    match self.current_block_info.read().unwrap().mode {
                        TextBlockType::RawBlock => {
                            self.finish_current_line(match_end);
                            return Err(Error::from(ErrorKind::UnexpectedStmtEnd(
                                SourceLocation::new(match_start, match_end),
                            )));
                        }
                        TextBlockType::Statement => {}
                        _ => {
                            continue;
                        }
                    }
                    self.current_block_info.write().unwrap().range.start =
                        self.finish_current_block(match_start, TextBlockType::RawText, None);
                }
                RegexEnum::RawBegin => {
                    match self.current_block_info.read().unwrap().mode {
                        TextBlockType::RawBlock => continue,
                        TextBlockType::Comment | TextBlockType::RawText => {}
                        _ => {
                            self.finish_current_line(match_end);
                            return Err(Error::from(ErrorKind::UnexpectedRawBegin(
                                SourceLocation::new(match_start, match_end),
                            )));
                        }
                    };
                    self.start_control_block(TextBlockType::RawBlock, match_start, match_end);
                }
                RegexEnum::RawEnd => {
                    match self.current_block_info.read().unwrap().mode {
                        TextBlockType::Comment => continue,
                        TextBlockType::RawBlock => {}
                        _ => {
                            self.finish_current_line(match_end);
                            return Err(Error::from(ErrorKind::UnexpectedRawEnd(
                                SourceLocation::new(match_start, match_end),
                            )));
                        }
                    };
                    self.current_block_info.write().unwrap().range.start = self
                        .finish_current_block(
                            match_end - 2,
                            TextBlockType::RawText,
                            Some(match_start),
                        );
                }
            };
        }
        let len_of_temp = self.template_body.read().unwrap().len();
        self.finish_current_line(len_of_temp);
        if let TextBlockType::RawBlock = self.current_block_info.read().unwrap().mode {
            return Err(Error::from(ErrorKind::ExpectedRawEnd(SourceLocation::new(
                len_of_temp,
                len_of_temp + 2, // TODO: THERE is not handling of expected end of comment????
            ))));
        }
        self.finish_current_block(len_of_temp, TextBlockType::RawText, None);

        Ok(())
    }

    fn start_control_block(
        &self,
        mode: TextBlockType,
        match_start: usize,
        mut start_offset: usize,
    ) {
        let end_offset = match self.current_block_info.read().unwrap().mode {
            TextBlockType::RawText => self.strip_block_left(
                start_offset,
                match_start,
                self.env.read().unwrap().settings().lstrip_blocks,
            ),
            _ => return,
        };
        self.finish_current_block(end_offset, mode, None);

        if start_offset < self.template_body.read().unwrap().len() {
            let ctrl_char = self
                .template_body
                .read()
                .unwrap()
                .chars()
                .nth(start_offset)
                .unwrap();

            if ctrl_char == '+' || ctrl_char == '-' {
                start_offset += 1;
            }
        }
        //m_currentBlockInfo.type = blockType; TODO: remove line in jinja2cpp
        if let TextBlockType::RawBlock = self.current_block_info.read().unwrap().mode {
            start_offset = self.strip_block_right(
                start_offset - 2,
                self.env.read().unwrap().settings().trim_blocks,
            );
        }
        self.current_block_info.write().unwrap().range.start = start_offset;
    }
    fn finish_current_block(
        &self,
        mut position: usize,
        next_block: TextBlockType,
        match_start: Option<usize>,
    ) -> usize {
        let mut new_position = position;
        match self.current_block_info.read().unwrap().mode {
            TextBlockType::RawBlock => {
                let current_position = match_start.unwrap_or(position);
                let original_position = position;
                position = self.strip_block_left(
                    current_position + 2,
                    current_position,
                    self.env.read().unwrap().settings().lstrip_blocks,
                );
                new_position = self.strip_block_right(
                    original_position,
                    self.env.read().unwrap().settings().trim_blocks,
                );
            }
            TextBlockType::RawText => {
                position = self.strip_block_left(
                    position + 2,
                    position,
                    self.env.read().unwrap().settings().lstrip_blocks,
                )
            }
            _ => {
                if let TextBlockType::RawText = next_block {
                    new_position = self.strip_block_right(
                        position,
                        self.env.read().unwrap().settings().trim_blocks,
                    );
                }

                if position != 0 {
                    let ctrl_char = self
                        .template_body
                        .read()
                        .unwrap()
                        .chars()
                        .nth(position - 1)
                        .unwrap();

                    if ctrl_char == '+' || ctrl_char == '-' {
                        position -= 1;
                    }
                }
            }
        };

        self.current_block_info.write().unwrap().range.end = position;

        self.text_blocks
            .write()
            .unwrap()
            .push(*self.current_block_info.read().unwrap());

        self.current_block_info.write().unwrap().mode = next_block;

        new_position
    }
    fn strip_block_left(
        &self,
        ctrl_char_pos: usize,
        mut end_offset: usize,
        mut do_trim: bool,
    ) -> usize {
        let mut do_total_strip = false;
        if ctrl_char_pos < self.template_body.read().unwrap().len() {
            let ctrl_char = self
                .template_body
                .read()
                .unwrap()
                .chars()
                .nth(ctrl_char_pos)
                .unwrap();
            if ctrl_char == '+' {
                do_trim = false;
            } else {
                do_total_strip = ctrl_char == '-';
            }
            do_trim |= do_total_strip;
        }
        if !do_trim {
            return end_offset;
        }
        match self.current_block_info.read().unwrap().mode {
            TextBlockType::RawText | TextBlockType::RawBlock => {}
            _ => return end_offset,
        }

        let original_offset = end_offset;
        let mut same_line = true;
        let start_offset = self.current_block_info.read().unwrap().range.start;
        let templ = self.template_body.read().unwrap();
        for ch in templ[start_offset..original_offset].chars().rev() {
            if !ch.is_whitespace() {
                if !same_line {
                    break;
                }
                if do_total_strip {
                    return end_offset;
                } else {
                    return original_offset;
                }
            }
            if ch == '\n' {
                if !do_total_strip {
                    break;
                }
                same_line = false;
            }
            end_offset -= 1;
        }

        end_offset
    }
    fn strip_block_right(&self, position: usize, mut do_trim: bool) -> usize {
        let mut new_pos = position + 2;

        if position != 0 {
            match self.current_block_info.read().unwrap().mode {
                TextBlockType::RawText => {}
                _ => {
                    let ctrl_char = self
                        .template_body
                        .read()
                        .unwrap()
                        .chars()
                        .nth(position - 1)
                        .unwrap();
                    do_trim = if ctrl_char == '-' {
                        true
                    } else {
                        if ctrl_char == '+' {
                            false
                        } else {
                            do_trim
                        }
                    }
                }
            }
        }

        if do_trim {
            let templ = self.template_body.read().unwrap();

            for ch in templ[position + 2..].chars() {
                if ch == '\n' {
                    new_pos += 1;
                    break;
                }
                if !ch.is_whitespace() {
                    break;
                }
                new_pos += 1;
            }
        }

        new_pos
    }
    fn finish_current_line(&self, position: usize) {
        self.current_line_info.write().unwrap().range.end = position;
        self.lines
            .write()
            .unwrap()
            .push(*self.current_line_info.read().unwrap());
        self.current_line_info.write().unwrap().line_number += 1;
    }
}

#[derive(Debug, Clone, Copy)]
struct TextBlockInfo {
    range: Range,
    mode: TextBlockType,
}
impl Default for TextBlockInfo {
    fn default() -> Self {
        Self {
            range: Range { start: 0, end: 0 },
            mode: TextBlockType::RawText,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Range {
    start: usize,
    end: usize,
}

impl Range {
    pub fn size(&self) -> usize {
        self.end - self.start
    }
}

#[derive(Debug, Clone, Copy)]
enum TextBlockType {
    RawText,
    Expression,
    Statement,
    Comment,
    LineStatement,
    RawBlock,
}

#[derive(Debug, Clone, Copy)]
struct LineInfo {
    range: Range,
    line_number: usize,
}

impl Default for LineInfo {
    fn default() -> Self {
        Self {
            range: Range { start: 0, end: 0 },
            line_number: 0,
        }
    }
}
