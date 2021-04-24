use nu_errors::ShellError;
use nu_plugin::{serve_plugin, Plugin};
use nu_protocol::{
    CallInfo, Primitive, ReturnSuccess, ReturnValue, Signature, SyntaxShape, TaggedDictBuilder,
    UntaggedValue, Value,
};
use std::fmt;

const LOG_KEYWORD: &str = "UnityEngine.Debug:Log";
const EMPTY_NEWLINE: &str = "\n\n";

#[derive(Debug, PartialEq)]
pub enum LogType {
    Log,
    Warning,
    Error,
    Unknown,
}

impl fmt::Display for LogType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct LogLine<'a> {
    pub log_type: LogType,
    pub message: &'a str,
    pub callstack: &'a str,
    pub trimmed_callstack: &'a str,
}

impl LogLine<'_> {
    fn same(&self, other: &LogLine) -> bool {
        self.log_type == other.log_type && self.message == other.message
    }
}

struct UnityLog {
    count: usize,
    no_collapse: bool,
}

impl Default for UnityLog {
    fn default() -> Self {
        UnityLog {
            count: 3,
            no_collapse: false,
        }
    }
}

impl UnityLog {
    fn new() -> UnityLog {
        UnityLog {
            ..Default::default()
        }
    }

    fn len(&mut self, value: Value) -> Result<Vec<Value>, ShellError> {
        match &value.value {
            UntaggedValue::Primitive(Primitive::String(s)) => {
                let tag = &value.tag;

                let sanitized = s.replace("\r\n", "\n");
                let input = sanitized.replace("\r", "\n");

                let mut lines: Vec<LogLine> = input
                    .split_terminator(EMPTY_NEWLINE)
                    .filter(|s| s.contains(LOG_KEYWORD))
                    .map(|block| -> Option<LogLine> {
                        let index = block.rfind(LOG_KEYWORD)?;
                        let (_, bottom) = block.split_at(index);
                        let (_, user_log) = bottom.split_once('\n')?;
                        // remove our custom logging methods
                        let custom_method = user_log.lines().next().unwrap_or("");
                        let trimmed = match custom_method.contains("Debug")
                            || custom_method.contains("Log")
                        {
                            true => user_log
                                .split_once('\n')
                                .map_or_else(|| user_log, |(_a, b)| b),
                            false => user_log,
                        };

                        let type_line = bottom.trim_start_matches(LOG_KEYWORD);
                        let log_type: LogType;
                        if type_line.starts_with("Error") {
                            log_type = LogType::Error;
                        } else if type_line.starts_with("Warning") {
                            log_type = LogType::Warning;
                        } else {
                            log_type = LogType::Log;
                        }

                        // next works like First() here *eyeroll*
                        Some(LogLine {
                            log_type,
                            message: block.lines().next().unwrap_or(""),
                            callstack: block,
                            trimmed_callstack: trimmed,
                        })
                    })
                    .flatten() // removes None elements
                    .collect();

                //TODO: check here if we have any lines and if not then we have a player log
                // that we need to check differently
                if lines.is_empty() {
                    lines = input
                        .split_terminator(EMPTY_NEWLINE)
                        .map(|block| -> Option<LogLine> {
                            Some(LogLine {
                                log_type: LogType::Unknown,
                                message: block.lines().next()?,
                                callstack: block,
                                trimmed_callstack: block.split_once('\n')?.1,
                            })
                        })
                        .flatten()
                        .collect()
                }

                if self.no_collapse {
                    lines.sort_by_key(|x| x.message);
                    lines.dedup_by(|a, b| a.same(b));
                }

                let rows = lines
                    .into_iter()
                    .map(|line| {
                        let mut dict = TaggedDictBuilder::new(tag);

                        dict.insert_untagged(
                            "type",
                            UntaggedValue::string(line.log_type.to_string()).into_value(tag),
                        );

                        dict.insert_untagged(
                            "message",
                            UntaggedValue::string(line.message).into_value(tag),
                        );

                        let truncated: String = line
                            .trimmed_callstack
                            .lines()
                            .take(self.count)
                            .map(|x| x.trim().to_string())
                            .collect();

                        dict.insert_untagged(
                            "short",
                            UntaggedValue::string(truncated).into_value(tag),
                        );

                        //TODO: add the full stacktrace as a table with colums: method, parameters, line

                        if dict.is_empty() {
                            Value::nothing()
                        } else {
                            dict.into_value()
                        }
                    })
                    .collect();

                Ok(rows)
            }
            _ => Err(ShellError::labeled_error(
                "Unrecognized type in stream",
                "'len' given non-string info by this",
                value.tag.span,
            )),
        }
    }
}

impl Plugin for UnityLog {
    fn config(&mut self) -> Result<Signature, ShellError> {
        Ok(Signature::build("unity")
            .desc("A plugin for reading Unity3D Player and Editor from development and release builds. Usage example: 'open Player.log | unity'")
            .named(
                "count",
                SyntaxShape::Int,
                "Set how many lines for the short callstacks are printed. Defaults to 5.",
                Some('c'),
            )
            .switch(
                "no-collapse",
                "Do not collapse same log statements together.",
                Some('n'),
            )
            .filter())
    }

    fn begin_filter(&mut self, call_info_args: CallInfo) -> Result<Vec<ReturnValue>, ShellError> {
        match call_info_args.args.get("count") {
            None => {}
            Some(c) => self.count = c.as_usize()?,
        }

        match call_info_args.args.get("no-collapse") {
            None => {}
            Some(n) => self.no_collapse = n.as_bool()?,
        }

        Ok(vec![])
    }

    fn filter(&mut self, input: Value) -> Result<Vec<ReturnValue>, ShellError> {
        let output = self.len(input);
        Ok(output?.into_iter().map(ReturnSuccess::value).collect())
    }
}

fn main() {
    serve_plugin(&mut UnityLog::new());
}
