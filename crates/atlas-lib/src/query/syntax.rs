//! The text of a program: tokens, then clauses that keep where each name was
//! written, before any relation is resolved. Nothing here knows the schema;
//! a misspelled relation is still a well-formed clause.
use super::{Error, ErrorKind, Location, Origin};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A lower-case name: a relation, field, builtin or keyword.
    Name(String),
    /// A variable: an upper-case name, or one that starts with `_` and has
    /// more letters.
    Var(String),
    /// `_`, matching anything without binding.
    Wild,
    Text(String),
    Integer(u64),
    Punct(&'static str),
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    at: Location,
    /// Byte offsets in the source, for the clause text.
    start: usize,
    end: usize,
}

/// A count written in a body, where the language has none.
const COUNT_HINT: &str = "a count goes in a rule head, not a body: write `n{key: K, n: count(C)} :- r{key: K, id: C}.` and then query it, `?- n{key: K, n: N}.` (cargo atlas help --topic query)";
/// `order by` written as a literal, after a comma.
const ORDER_HINT: &str = "`order by` ends a query after its last literal, with no comma before it: `?- r{a: A, n: N} order by N desc limit 10.` (cargo atlas help --topic query)";

/// Longer punctuation first, so `:-` is never read as `:` then `-`.
const PUNCTS: [&str; 16] = [
    ":-", "?-", "!=", "<=", ">=", "(", ")", "{", "}", ",", ".", ":", "!", "=", "<", ">",
];

#[expect(
    clippy::too_many_lines,
    reason = "longer than 70 lines; to be split along its decisions"
)]
fn lex(source: &str, origin: Origin) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    let (mut line, mut column) = (1, 1);
    let advance = |c: char, line: &mut usize, column: &mut usize| {
        if c == '\n' {
            *line += 1;
            *column = 1;
        } else {
            *column += 1;
        }
    };
    while let Some(&(start, c)) = chars.peek() {
        let at = Location { line, column };
        if c.is_whitespace() {
            advance(c, &mut line, &mut column);
            chars.next();
            continue;
        }
        if c == '%' || c == '#' {
            while let Some(&(_, c)) = chars.peek() {
                if c == '\n' {
                    break;
                }
                advance(c, &mut line, &mut column);
                chars.next();
            }
            continue;
        }
        let tok = if c.is_alphabetic() || c == '_' {
            let mut word = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if !(c.is_alphanumeric() || c == '_') {
                    break;
                }
                word.push(c);
                advance(c, &mut line, &mut column);
                chars.next();
            }
            if word == "_" {
                Tok::Wild
            } else if word.starts_with(|c: char| c.is_uppercase() || c == '_') {
                Tok::Var(word)
            } else {
                Tok::Name(word)
            }
        } else if c.is_ascii_digit() {
            let mut digits = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if !c.is_ascii_digit() {
                    break;
                }
                digits.push(c);
                advance(c, &mut line, &mut column);
                chars.next();
            }
            Tok::Integer(digits.parse().map_err(|_| {
                Error::located(
                    ErrorKind::Syntax,
                    origin,
                    source,
                    at,
                    format!("integer {digits} is too large"),
                )
            })?)
        } else if c == '"' {
            advance(c, &mut line, &mut column);
            chars.next();
            let mut text = String::new();
            loop {
                let Some((_, c)) = chars.next() else {
                    return Err(Error::located(
                        ErrorKind::Syntax,
                        origin,
                        source,
                        at,
                        "text has no closing `\"`".into(),
                    ));
                };
                advance(c, &mut line, &mut column);
                match c {
                    '"' => break,
                    '\\' => {
                        let Some((_, escaped)) = chars.next() else {
                            continue;
                        };
                        advance(escaped, &mut line, &mut column);
                        text.push(match escaped {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                    }
                    _ => text.push(c),
                }
            }
            Tok::Text(text)
        } else {
            let rest = &source[start..];
            let Some(punct) = PUNCTS.iter().copied().find(|p| rest.starts_with(p)) else {
                return Err(Error::located(
                    ErrorKind::Syntax,
                    origin,
                    source,
                    at,
                    format!("unexpected character `{c}`"),
                ));
            };
            for c in punct.chars() {
                advance(c, &mut line, &mut column);
                chars.next();
            }
            Tok::Punct(punct)
        };
        let end = chars.peek().map_or(source.len(), |&(i, _)| i);
        tokens.push(Token {
            tok,
            at,
            start,
            end,
        });
    }
    Ok(tokens)
}

/// A constant as written.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Const {
    Text(String),
    Integer(u64),
    Boolean(bool),
}

#[derive(Clone, Debug)]
pub(crate) enum Term {
    Var(String, Location),
    Wild(Location),
    Const(Const, Location),
}
impl Term {
    pub(crate) fn at(&self) -> Location {
        match self {
            Self::Var(_, at) | Self::Wild(at) | Self::Const(_, at) => *at,
        }
    }
}

/// `field: term` inside a relation's braces.
#[derive(Clone, Debug)]
pub(crate) struct Field<T> {
    pub(crate) name: String,
    pub(crate) term: T,
    pub(crate) at: Location,
}

#[derive(Clone, Debug)]
pub(crate) struct Atom {
    pub(crate) relation: String,
    pub(crate) at: Location,
    pub(crate) fields: Vec<Field<Term>>,
}

#[derive(Clone, Debug)]
pub(crate) enum HeadTerm {
    Term(Term),
    /// `count(V, ...)`: distinct bindings of the variables per group.
    Count(Vec<(String, Location)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Functions over text values. Only `last_segment` can bind its second
/// argument; the others test values that are already bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Builtin {
    /// `under(Path, Dir)`: `Path` is `Dir` or inside it; `""` is the root.
    Under,
    StartsWith,
    EndsWith,
    Contains,
    /// `last_segment(Path, Segment)`: the part after the last `::`.
    LastSegment,
}
impl Builtin {
    pub(crate) const ALL: [(&'static str, Self); 5] = [
        ("under", Self::Under),
        ("starts_with", Self::StartsWith),
        ("ends_with", Self::EndsWith),
        ("contains", Self::Contains),
        ("last_segment", Self::LastSegment),
    ];
    pub(crate) fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, b)| *b == self)
            .map_or("", |(name, _)| name)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Literal {
    Atom {
        negated: bool,
        atom: Atom,
    },
    Compare {
        op: Op,
        left: Term,
        right: Term,
        at: Location,
    },
    Builtin {
        builtin: Builtin,
        args: Vec<Term>,
        at: Location,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Order {
    pub(crate) var: String,
    pub(crate) descending: bool,
    pub(crate) at: Location,
}

#[derive(Clone, Debug)]
pub(crate) enum ClauseKind {
    Rule {
        relation: String,
        head: Vec<Field<HeadTerm>>,
        body: Vec<Literal>,
    },
    Query {
        body: Vec<Literal>,
        order: Vec<Order>,
        limit: Option<usize>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Clause {
    pub(crate) kind: ClauseKind,
    pub(crate) at: Location,
    /// The clause as written, on one line.
    pub(crate) text: String,
}

/// A parsed program: its clauses, with the source kept for error messages.
#[derive(Clone, Debug)]
pub struct Program {
    pub(crate) clauses: Vec<Clause>,
    pub(crate) source: String,
    pub(crate) origin: Origin,
}
impl Program {
    pub(crate) fn error(&self, kind: ErrorKind, at: Location, message: String) -> Error {
        Error::located(kind, self.origin, &self.source, at, message)
    }
    /// The number of clauses (rules, facts and queries).
    pub fn len(&self) -> usize {
        self.clauses.len()
    }
    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }
}

struct Parser<'a> {
    tokens: Vec<Token>,
    i: usize,
    source: &'a str,
    origin: Origin,
}
impl Parser<'_> {
    fn error(&self, at: Location, message: String) -> Error {
        Error::located(ErrorKind::Syntax, self.origin, self.source, at, message)
    }
    fn here(&self) -> Location {
        self.tokens.get(self.i).map_or_else(
            || {
                let line = self.source.lines().count().max(1);
                let column = self.source.lines().last().map_or(0, str::len) + 1;
                Location { line, column }
            },
            |t| t.at,
        )
    }
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.i).map(|t| &t.tok)
    }
    fn peek_at(&self, offset: usize) -> Option<&Tok> {
        self.tokens.get(self.i + offset).map(|t| &t.tok)
    }
    fn is(&self, punct: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(p)) if *p == punct)
    }
    fn is_name(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Tok::Name(n)) if n == word)
    }
    fn describe(tok: Option<&Tok>) -> String {
        match tok {
            None => "the end of the program".into(),
            Some(Tok::Name(n) | Tok::Var(n)) => format!("`{n}`"),
            Some(Tok::Wild) => "`_`".into(),
            Some(Tok::Text(t)) => format!("{t:?}"),
            Some(Tok::Integer(n)) => format!("`{n}`"),
            Some(Tok::Punct(p)) => format!("`{p}`"),
        }
    }
    fn expect(&mut self, punct: &str, context: &str) -> Result<Location, Error> {
        if self.is(punct) {
            let at = self.here();
            self.i += 1;
            Ok(at)
        } else {
            Err(self.error(
                self.here(),
                format!(
                    "expected `{punct}` {context}, found {}",
                    Self::describe(self.peek())
                ),
            ))
        }
    }
    fn name(&mut self, what: &str) -> Result<(String, Location), Error> {
        let at = self.here();
        match self.peek().cloned() {
            Some(Tok::Name(n)) => {
                self.i += 1;
                Ok((n, at))
            }
            other => Err(self.error(
                at,
                format!(
                    "expected {what} (a lower-case name), found {}",
                    Self::describe(other.as_ref())
                ),
            )),
        }
    }
    fn term(&mut self) -> Result<Term, Error> {
        let at = self.here();
        let term = match self.peek().cloned() {
            Some(Tok::Var(v)) => Term::Var(v, at),
            Some(Tok::Wild) => Term::Wild(at),
            Some(Tok::Text(t)) => Term::Const(Const::Text(t), at),
            Some(Tok::Integer(n)) => Term::Const(Const::Integer(n), at),
            Some(Tok::Name(n)) if n == "true" => Term::Const(Const::Boolean(true), at),
            Some(Tok::Name(n)) if n == "false" => Term::Const(Const::Boolean(false), at),
            Some(Tok::Name(n)) => {
                return Err(self.error(
                    at,
                    format!(
                        "`{n}` is not a value: variables start with an upper-case letter, and text is quoted (\"{n}\")"
                    ),
                ));
            }
            other => {
                return Err(self.error(
                    at,
                    format!(
                        "expected a variable, `_` or a value, found {}",
                        Self::describe(other.as_ref())
                    ),
                ));
            }
        };
        self.i += 1;
        Ok(term)
    }
    /// `{ field: X, ... }`, after the relation name.
    fn fields<T>(
        &mut self,
        relation: &str,
        mut value: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<Field<T>>, Error> {
        let open = self.here();
        if self.is("(") {
            return Err(self.error(
                open,
                format!(
                    "relations take named fields in braces: `{relation}{{field: Value, ...}}`; parentheses are for the builtins ({})",
                    Builtin::ALL.map(|(n, _)| n).join(", ")
                ),
            ));
        }
        self.expect("{", &format!("after `{relation}`"))?;
        let mut fields = Vec::new();
        while !self.is("}") {
            let (name, at) = self.name("a field name")?;
            self.expect(":", &format!("after field `{name}`"))?;
            let term = value(self)?;
            fields.push(Field { name, term, at });
            if !self.is("}") {
                self.expect(",", "between fields")?;
            }
        }
        self.i += 1;
        Ok(fields)
    }
    #[expect(
        clippy::too_many_lines,
        reason = "longer than 70 lines; to be split along its decisions"
    )]
    fn literal(&mut self) -> Result<Literal, Error> {
        let at = self.here();
        let negated = if self.is("!") || self.is_name("not") {
            self.i += 1;
            true
        } else {
            false
        };
        if let Some(Tok::Name(name)) = self.peek().cloned()
            && !matches!(name.as_str(), "true" | "false")
        {
            if let Some((_, builtin)) = Builtin::ALL.iter().find(|(n, _)| *n == name)
                && matches!(self.peek_at(1), Some(Tok::Punct("(")))
            {
                if negated {
                    return Err(self.error(
                        at,
                        format!(
                            "`{name}` is a test, not a relation; negate it by testing the opposite"
                        ),
                    ));
                }
                self.i += 2;
                let mut args = Vec::new();
                while !self.is(")") {
                    args.push(self.term()?);
                    if !self.is(")") {
                        self.expect(",", &format!("between the arguments of `{name}`"))?;
                    }
                }
                self.i += 1;
                if args.len() != 2 {
                    return Err(self.error(
                        at,
                        format!("`{name}` takes 2 arguments, given {}", args.len()),
                    ));
                }
                return Ok(Literal::Builtin {
                    builtin: *builtin,
                    args,
                    at,
                });
            }
            if name == "order" && matches!(self.peek_at(1), Some(Tok::Name(by)) if by == "by") {
                return Err(self.error(at, ORDER_HINT.to_owned()));
            }
            if matches!(self.peek_at(1), Some(Tok::Punct("=")))
                && matches!(self.peek_at(2), Some(Tok::Name(count)) if count == "count")
            {
                return Err(self.error(at, COUNT_HINT.to_owned()));
            }
            let (relation, rel_at) = self.name("a relation")?;
            let fields = self.fields(&relation, Self::term)?;
            return Ok(Literal::Atom {
                negated,
                atom: Atom {
                    relation,
                    at: rel_at,
                    fields,
                },
            });
        }
        if negated {
            return Err(self.error(
                self.here(),
                format!(
                    "expected a relation after the negation, found {}",
                    Self::describe(self.peek())
                ),
            ));
        }
        if matches!(self.peek_at(1), Some(Tok::Punct("=")))
            && matches!(self.peek_at(2), Some(Tok::Name(count)) if count == "count")
        {
            return Err(self.error(at, COUNT_HINT.to_owned()));
        }
        let left = self.term()?;
        let op_at = self.here();
        let op = match self.peek() {
            Some(Tok::Punct("=")) => Op::Eq,
            Some(Tok::Punct("!=")) => Op::Ne,
            Some(Tok::Punct("<")) => Op::Lt,
            Some(Tok::Punct("<=")) => Op::Le,
            Some(Tok::Punct(">")) => Op::Gt,
            Some(Tok::Punct(">=")) => Op::Ge,
            other => {
                return Err(self.error(
                    op_at,
                    format!(
                        "expected a comparison (=, !=, <, <=, >, >=) after the value, found {}",
                        Self::describe(other)
                    ),
                ));
            }
        };
        self.i += 1;
        let right = self.term()?;
        Ok(Literal::Compare {
            op,
            left,
            right,
            at,
        })
    }
    /// Literals separated by commas, up to (not including) a token that
    /// ends the body: `.`, or `order`/`limit` in a query.
    fn body(&mut self, query: bool) -> Result<Vec<Literal>, Error> {
        let mut body = vec![self.literal()?];
        while self.is(",") {
            self.i += 1;
            body.push(self.literal()?);
        }
        if !self.is(".") && !(query && (self.is_name("order") || self.is_name("limit"))) {
            let expected = if query {
                "`,`, `order by`, `limit` or `.`"
            } else {
                "`,` or `.`"
            };
            return Err(self.error(
                self.here(),
                format!(
                    "expected {expected} after a literal, found {}",
                    Self::describe(self.peek())
                ),
            ));
        }
        Ok(body)
    }
    fn head_term(&mut self) -> Result<HeadTerm, Error> {
        if self.is_name("count") && matches!(self.peek_at(1), Some(Tok::Punct("("))) {
            self.i += 2;
            let mut vars = Vec::new();
            while !self.is(")") {
                let at = self.here();
                match self.peek().cloned() {
                    Some(Tok::Var(v)) => {
                        self.i += 1;
                        vars.push((v, at));
                    }
                    other => {
                        return Err(self.error(
                            at,
                            format!(
                                "`count` takes variables, found {}",
                                Self::describe(other.as_ref())
                            ),
                        ));
                    }
                }
                if !self.is(")") {
                    self.expect(",", "between the variables of `count`")?;
                }
            }
            let close = self.here();
            self.i += 1;
            if vars.is_empty() {
                return Err(self.error(close, "`count` needs at least one variable".into()));
            }
            return Ok(HeadTerm::Count(vars));
        }
        self.term().map(HeadTerm::Term)
    }
    #[expect(
        clippy::too_many_lines,
        reason = "longer than 70 lines; to be split along its decisions"
    )]
    fn clause(&mut self) -> Result<Clause, Error> {
        let first = self.i;
        let at = self.here();
        let kind = if self.is("?-") {
            self.i += 1;
            let body = self.body(true)?;
            let mut order = Vec::new();
            if self.is_name("order") {
                self.i += 1;
                if !self.is_name("by") {
                    return Err(self.error(self.here(), "expected `by` after `order`".into()));
                }
                self.i += 1;
                loop {
                    let at = self.here();
                    let Some(Tok::Var(var)) = self.peek().cloned() else {
                        return Err(self.error(
                            at,
                            format!(
                                "`order by` takes the query's variables, found {}",
                                Self::describe(self.peek())
                            ),
                        ));
                    };
                    self.i += 1;
                    let descending = if self.is_name("desc") {
                        self.i += 1;
                        true
                    } else {
                        if self.is_name("asc") {
                            self.i += 1;
                        }
                        false
                    };
                    order.push(Order {
                        var,
                        descending,
                        at,
                    });
                    if !self.is(",") {
                        break;
                    }
                    self.i += 1;
                }
            }
            let mut limit = None;
            if self.is_name("limit") {
                self.i += 1;
                let at = self.here();
                let Some(Tok::Integer(n)) = self.peek().cloned() else {
                    return Err(self.error(
                        at,
                        format!(
                            "`limit` takes a number, found {}",
                            Self::describe(self.peek())
                        ),
                    ));
                };
                self.i += 1;
                limit = Some(usize::try_from(n).unwrap_or(usize::MAX));
            }
            ClauseKind::Query { body, order, limit }
        } else {
            let (relation, _) = self.name("a rule head or `?-`")?;
            let head = self.fields(&relation, Self::head_term)?;
            let body = if self.is(":-") {
                self.i += 1;
                self.body(false)?
            } else {
                Vec::new()
            };
            ClauseKind::Rule {
                relation,
                head,
                body,
            }
        };
        let end = self.expect(".", "to end the clause")?;
        let _ = end;
        let start = self.tokens.get(first).map_or(0, |t| t.start);
        let stop = self
            .tokens
            .get(self.i - 1)
            .map_or(self.source.len(), |t| t.end);
        let text = self.source[start..stop]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        Ok(Clause { kind, at, text })
    }
}

pub(crate) fn parse(source: &str, origin: Origin) -> Result<Program, Error> {
    let tokens = lex(source, origin)?;
    let mut parser = Parser {
        tokens,
        i: 0,
        source,
        origin,
    };
    let mut clauses = Vec::new();
    while parser.i < parser.tokens.len() {
        clauses.push(parser.clause()?);
    }
    Ok(Program {
        clauses,
        source: source.to_owned(),
        origin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_user(source: &str) -> Result<Program, Error> {
        parse(source, Origin::Program)
    }

    #[test]
    fn rules_facts_and_queries_parse_with_their_text() {
        let program = parse_user(
            "% a comment\nreach{from: A, to: B} :- edge{from: A, to: B}.\nroot{name: \"main\"}.\n?- reach{from: A, to: B}, !blocked{at: B}, A != B order by A desc, B limit 5.",
        )
        .unwrap();
        assert_eq!(program.len(), 3);
        assert_eq!(
            program.clauses[0].text,
            "reach{from: A, to: B} :- edge{from: A, to: B}."
        );
        let ClauseKind::Query { body, order, limit } = &program.clauses[2].kind else {
            panic!("query")
        };
        assert_eq!(body.len(), 3);
        assert!(matches!(body[1], Literal::Atom { negated: true, .. }));
        assert_eq!(order.len(), 2);
        assert!(order[0].descending && !order[1].descending);
        assert_eq!(*limit, Some(5));
    }

    #[test]
    fn errors_name_line_column_and_what_was_found() {
        let error = parse_user("?- call{from: F}\n   , call(F).").unwrap_err();
        assert_eq!(
            error.location,
            Some(Location {
                line: 2,
                column: 10
            })
        );
        assert!(error.message.contains("named fields in braces"), "{error}");
        let error = parse_user("?- call{from: f}.").unwrap_err();
        assert!(error.message.contains("upper-case"), "{error}");
        let error = parse_user("?- call{from: F}").unwrap_err();
        assert!(error.message.contains("the end of the program"), "{error}");
        let error = parse_user("x{n: count()} :- y{a: A}.").unwrap_err();
        assert!(error.message.contains("at least one"), "{error}");
        let rendered = parse_user("?- a{x: X} b{y: Y}.").unwrap_err().to_string();
        assert!(rendered.starts_with("query:1:12:"), "{rendered}");
        assert!(rendered.contains("?- a{x: X} b{y: Y}."), "{rendered}");
    }

    #[test]
    fn text_escapes_and_keywords() {
        let program = parse_user(r#"?- t{a: "say \"hi\"\n", b: true, c: 7}."#).unwrap();
        let ClauseKind::Query { body, .. } = &program.clauses[0].kind else {
            panic!("query")
        };
        let Literal::Atom { atom, .. } = &body[0] else {
            panic!("atom")
        };
        assert!(
            matches!(&atom.fields[0].term, Term::Const(Const::Text(t), _) if t == "say \"hi\"\n")
        );
        assert!(matches!(
            &atom.fields[1].term,
            Term::Const(Const::Boolean(true), _)
        ));
        assert!(matches!(
            &atom.fields[2].term,
            Term::Const(Const::Integer(7), _)
        ));
    }
}
