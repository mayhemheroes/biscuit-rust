//! helper functions and structure to create tokens and blocks
use super::{Biscuit, Block};
use crate::crypto::KeyPair;
use crate::datalog::{self, SymbolTable};
use crate::error;
use chrono::{DateTime, NaiveDateTime, Utc};
use rand_core::{CryptoRng, RngCore};
use std::{
    collections::BTreeSet,
    convert::{TryFrom, TryInto},
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// reexport those because the builder uses the same definitions
pub use crate::datalog::{Binary, Unary};

#[derive(Clone, Debug)]
pub struct BlockBuilder {
    pub facts: Vec<Fact>,
    pub rules: Vec<Rule>,
    pub checks: Vec<Check>,
    pub context: Option<String>,
}

impl BlockBuilder {
    pub fn new() -> BlockBuilder {
        BlockBuilder {
            facts: vec![],
            rules: vec![],
            checks: vec![],
            context: None,
        }
    }

    pub fn add_fact<F: TryInto<Fact>>(&mut self, fact: F) -> Result<(), error::Token> {
        let fact = fact.try_into().map_err(|_| error::Token::ParseError)?;
        self.facts.push(fact);
        Ok(())
    }

    pub fn add_rule<R: TryInto<Rule>>(&mut self, rule: R) -> Result<(), error::Token> {
        let rule = rule.try_into().map_err(|_| error::Token::ParseError)?;
        self.rules.push(rule);
        Ok(())
    }

    pub fn add_check<C: TryInto<Check>>(&mut self, check: C) -> Result<(), error::Token> {
        let check = check.try_into().map_err(|_| error::Token::ParseError)?;
        self.checks.push(check);
        Ok(())
    }

    pub fn set_context(&mut self, context: String) {
        self.context = Some(context);
    }

    pub fn build(self, mut symbols: SymbolTable) -> Block {
        let symbols_start = symbols.symbols.len();

        let mut facts = Vec::new();
        for fact in self.facts {
            facts.push(fact.convert(&mut symbols));
        }

        let mut rules = Vec::new();
        for rule in self.rules {
            rules.push(rule.convert(&mut symbols));
        }

        let mut checks = Vec::new();
        for check in self.checks {
            checks.push(check.convert(&mut symbols));
        }
        let new_syms = SymbolTable {
            symbols: symbols.symbols.split_off(symbols_start),
        };

        Block {
            symbols: new_syms,
            facts,
            rules,
            checks,
            context: self.context,
            version: super::MAX_SCHEMA_VERSION,
        }
    }

    pub fn check_right(&mut self, right: &str) {
        let check = rule(
            "check_right",
            &[s(right)],
            &[
                pred("resource", &[var("resource_name")]),
                pred("operation", &[s(right)]),
                pred("right", &[var("resource_name"), s(right)]),
            ],
        );

        let _ = self.add_check(check);
    }

    pub fn check_resource(&mut self, resource: &str) {
        let check = rule(
            "resource_check",
            &[s("resource_check")],
            &[pred("resource", &[string(resource)])],
        );

        let _ = self.add_check(check);
    }

    pub fn check_operation(&mut self, operation: &str) {
        let check = rule(
            "operation_check",
            &[s("operation_check")],
            &[pred("operation", &[s(operation)])],
        );

        let _ = self.add_check(check);
    }

    pub fn resource_prefix(&mut self, prefix: &str) {
        let check = constrained_rule(
            "prefix",
            &[var("resource")],
            &[pred("resource", &[var("resource")])],
            &[Expression {
                ops: vec![
                    Op::Value(var("resource")),
                    Op::Value(string(prefix)),
                    Op::Binary(Binary::Prefix),
                ],
            }],
        );

        let _ = self.add_check(check);
    }

    pub fn resource_suffix(&mut self, suffix: &str) {
        let check = constrained_rule(
            "suffix",
            &[var("resource")],
            &[pred("resource", &[var("resource")])],
            &[Expression {
                ops: vec![
                    Op::Value(var("resource")),
                    Op::Value(string(suffix)),
                    Op::Binary(Binary::Suffix),
                ],
            }],
        );

        let _ = self.add_check(check);
    }

    pub fn expiration_date(&mut self, exp: SystemTime) {
        let check = constrained_rule(
            "expiration",
            &[var("date")],
            &[pred("time", &[var("date")])],
            &[Expression {
                ops: vec![
                    Op::Value(var("date")),
                    Op::Value(date(&exp)),
                    Op::Binary(Binary::LessOrEqual),
                ],
            }],
        );

        let _ = self.add_check(check);
    }

    pub fn revocation_id(&mut self, id: i64) {
        let _ = self.add_fact(fact("revocation_id", &[int(id)]));
    }
}

impl std::default::Default for BlockBuilder {
    fn default() -> Self {
        BlockBuilder::new()
    }
}

#[derive(Clone)]
pub struct BiscuitBuilder<'a> {
    root_key_id: Option<u32>,
    root: &'a KeyPair,
    pub symbols_start: usize,
    pub symbols: SymbolTable,
    pub facts: Vec<datalog::Fact>,
    pub rules: Vec<datalog::Rule>,
    pub checks: Vec<datalog::Check>,
    pub context: Option<String>,
}

impl<'a> BiscuitBuilder<'a> {
    pub fn new(root: &'a KeyPair, base_symbols: SymbolTable) -> BiscuitBuilder<'a> {
        BiscuitBuilder {
            root_key_id: None,
            root,
            symbols_start: base_symbols.symbols.len(),
            symbols: base_symbols,
            facts: vec![],
            rules: vec![],
            checks: vec![],
            context: None,
        }
    }

    pub fn set_root_key_id(&mut self, id: u32) {
        self.root_key_id = Some(id);
    }
    pub fn add_authority_fact<F: TryInto<Fact>>(&mut self, fact: F) -> Result<(), error::Token> {
        let fact = fact.try_into().map_err(|_| error::Token::ParseError)?;

        let f = fact.convert(&mut self.symbols);
        self.facts.push(f);
        Ok(())
    }

    pub fn add_authority_rule<Ru: TryInto<Rule>>(&mut self, rule: Ru) -> Result<(), error::Token> {
        let rule = rule.try_into().map_err(|_| error::Token::ParseError)?;

        let r = rule.convert(&mut self.symbols);
        self.rules.push(r);
        Ok(())
    }

    pub fn add_authority_check<C: TryInto<Check>>(&mut self, rule: C) -> Result<(), error::Token> {
        let check: Check = rule.try_into().map_err(|_| error::Token::ParseError)?;
        let c = check.convert(&mut self.symbols);
        self.checks.push(c);
        Ok(())
    }

    pub fn add_right(&mut self, resource: &str, right: &str) {
        let _ = self.add_authority_fact(fact("right", &[string(resource), s(right)]));
    }

    pub fn set_context(&mut self, context: String) {
        self.context = Some(context);
    }

    pub fn build(self) -> Result<Biscuit, error::Token> {
        self.build_with_rng(&mut rand::rngs::OsRng)
    }

    pub fn build_with_rng<R: RngCore + CryptoRng>(
        mut self,
        rng: &'a mut R,
    ) -> Result<Biscuit, error::Token> {
        let new_syms = SymbolTable {
            symbols: self.symbols.symbols.split_off(self.symbols_start),
        };

        let authority_block = Block {
            symbols: new_syms,
            facts: self.facts,
            rules: self.rules,
            checks: self.checks,
            context: self.context,
            version: super::MAX_SCHEMA_VERSION,
        };

        Biscuit::new_with_rng(
            rng,
            self.root_key_id,
            self.root,
            self.symbols,
            authority_block,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    Variable(String),
    Integer(i64),
    Str(String),
    Date(u64),
    Bytes(Vec<u8>),
    Bool(bool),
    Set(BTreeSet<Term>),
}

impl Term {
    pub fn convert(&self, symbols: &mut SymbolTable) -> datalog::Term {
        match self {
            Term::Variable(s) => datalog::Term::Variable(symbols.insert(s) as u32),
            Term::Integer(i) => datalog::Term::Integer(*i),
            Term::Str(s) => datalog::Term::Str(symbols.insert(s)),
            Term::Date(d) => datalog::Term::Date(*d),
            Term::Bytes(s) => datalog::Term::Bytes(s.clone()),
            Term::Bool(b) => datalog::Term::Bool(*b),
            Term::Set(s) => datalog::Term::Set(s.iter().map(|i| i.convert(symbols)).collect()),
        }
    }

    pub fn convert_from(f: &datalog::Term, symbols: &SymbolTable) -> Self {
        match f {
            datalog::Term::Variable(s) => Term::Variable(symbols.print_symbol(*s as u64)),
            datalog::Term::Integer(i) => Term::Integer(*i),
            datalog::Term::Str(s) => Term::Str(symbols.print_symbol(*s)),
            datalog::Term::Date(d) => Term::Date(*d),
            datalog::Term::Bytes(s) => Term::Bytes(s.clone()),
            datalog::Term::Bool(b) => Term::Bool(*b),
            datalog::Term::Set(s) => {
                Term::Set(s.iter().map(|i| Term::convert_from(i, symbols)).collect())
            }
        }
    }
}

impl From<&Term> for Term {
    fn from(i: &Term) -> Self {
        match i {
            Term::Variable(ref v) => Term::Variable(v.clone()),
            Term::Integer(ref i) => Term::Integer(*i),
            Term::Str(ref s) => Term::Str(s.clone()),
            Term::Date(ref d) => Term::Date(*d),
            Term::Bytes(ref s) => Term::Bytes(s.clone()),
            Term::Bool(b) => Term::Bool(*b),
            Term::Set(ref s) => Term::Set(s.clone()),
        }
    }
}

impl AsRef<Term> for Term {
    fn as_ref(&self) -> &Term {
        self
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::Variable(i) => write!(f, "${}", i),
            Term::Integer(i) => write!(f, "{}", i),
            Term::Str(s) => write!(f, "\"{}\"", s),
            Term::Date(d) => {
                let date =
                    DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(*d as i64, 0), Utc);
                write!(f, "{}", date.to_rfc3339())
            }
            Term::Bytes(s) => write!(f, "hex:{}", hex::encode(s)),
            Term::Bool(b) => {
                if *b {
                    write!(f, "true")
                } else {
                    write!(f, "false")
                }
            }
            Term::Set(s) => {
                let terms = s.iter().map(|term| term.to_string()).collect::<Vec<_>>();
                write!(f, "[ {}]", terms.join(", "))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub struct Predicate {
    pub name: String,
    pub terms: Vec<Term>,
}

impl Predicate {
    pub fn convert(&self, symbols: &mut SymbolTable) -> datalog::Predicate {
        let name = symbols.insert(&self.name);
        let mut terms = vec![];

        for term in self.terms.iter() {
            terms.push(term.convert(symbols));
        }

        datalog::Predicate { name, terms }
    }

    pub fn convert_from(p: &datalog::Predicate, symbols: &SymbolTable) -> Self {
        Predicate {
            name: symbols.print_symbol(p.name),
            terms: p
                .terms
                .iter()
                .map(|term| Term::convert_from(term, symbols))
                .collect(),
        }
    }

    pub fn new(name: String, terms: &[Term]) -> Predicate {
        Predicate {
            name,
            terms: terms.to_vec(),
        }
    }
}

impl AsRef<Predicate> for Predicate {
    fn as_ref(&self) -> &Predicate {
        self
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.name)?;

        if !self.terms.is_empty() {
            write!(f, "{}", self.terms[0])?;

            if self.terms.len() > 1 {
                for i in 1..self.terms.len() {
                    write!(f, ", {}", self.terms[i])?;
                }
            }
        }
        write!(f, ")")
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub struct Fact(pub Predicate);

impl Fact {
    pub fn new(name: String, terms: &[Term]) -> Fact {
        Fact(Predicate::new(name, terms))
    }
}

impl Fact {
    pub fn convert(&self, symbols: &mut SymbolTable) -> datalog::Fact {
        datalog::Fact {
            predicate: self.0.convert(symbols),
        }
    }

    pub fn convert_from(f: &datalog::Fact, symbols: &SymbolTable) -> Self {
        Fact(Predicate::convert_from(&f.predicate, symbols))
    }
}

impl fmt::Display for Fact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    pub ops: Vec<Op>,
}

impl Expression {
    pub fn convert(&self, symbols: &mut SymbolTable) -> datalog::Expression {
        datalog::Expression {
            ops: self.ops.iter().map(|op| op.convert(symbols)).collect(),
        }
    }

    pub fn convert_from(e: &datalog::Expression, symbols: &SymbolTable) -> Self {
        Expression {
            ops: e
                .ops
                .iter()
                .map(|op| Op::convert_from(op, symbols))
                .collect(),
        }
    }
}

impl AsRef<Expression> for Expression {
    fn as_ref(&self) -> &Expression {
        self
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut syms = super::default_symbol_table();
        let expr = self.convert(&mut syms);
        let s = expr.print(&syms).unwrap();
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Value(Term),
    Unary(Unary),
    Binary(Binary),
}

impl Op {
    pub fn convert(&self, symbols: &mut SymbolTable) -> datalog::Op {
        match self {
            Op::Value(t) => datalog::Op::Value(t.convert(symbols)),
            Op::Unary(u) => datalog::Op::Unary(u.clone()),
            Op::Binary(b) => datalog::Op::Binary(b.clone()),
        }
    }

    pub fn convert_from(op: &datalog::Op, symbols: &SymbolTable) -> Self {
        match op {
            datalog::Op::Value(t) => Op::Value(Term::convert_from(t, symbols)),
            datalog::Op::Unary(u) => Op::Unary(u.clone()),
            datalog::Op::Binary(b) => Op::Binary(b.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub head: Predicate,
    pub body: Vec<Predicate>,
    pub expressions: Vec<Expression>,
}

impl Rule {
    pub fn convert(&self, symbols: &mut SymbolTable) -> datalog::Rule {
        let head = self.head.convert(symbols);
        let mut body = vec![];
        let mut expressions = vec![];

        for p in self.body.iter() {
            body.push(p.convert(symbols));
        }

        for c in self.expressions.iter() {
            expressions.push(c.convert(symbols));
        }

        datalog::Rule {
            head,
            body,
            expressions,
        }
    }

    pub fn convert_from(r: &datalog::Rule, symbols: &SymbolTable) -> Self {
        Rule {
            head: Predicate::convert_from(&r.head, symbols),
            body: r
                .body
                .iter()
                .map(|p| Predicate::convert_from(p, symbols))
                .collect(),
            expressions: r
                .expressions
                .iter()
                .map(|c| Expression::convert_from(c, symbols))
                .collect(),
        }
    }

    pub fn validate_variables(&self) -> Result<(), String> {
        let mut head_variables: std::collections::HashSet<String> = self
            .head
            .terms
            .iter()
            .filter_map(|term| match term {
                Term::Variable(s) => Some(s.to_string()),
                _ => None,
            })
            .collect();

        for predicate in self.body.iter() {
            for term in predicate.terms.iter() {
                if let Term::Variable(v) = term {
                    head_variables.remove(v);
                    if head_variables.is_empty() {
                        return Ok(());
                    }
                }
            }
        }

        Err(format!(
            "rule head contains variables that are not used in predicates of the rule's body: {}",
            head_variables
                .iter()
                .map(|s| format!("${}", s))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

fn display_rule_body(r: &Rule, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if !r.body.is_empty() {
        write!(f, "{}", r.body[0])?;

        if r.body.len() > 1 {
            for i in 1..r.body.len() {
                write!(f, ", {}", r.body[i])?;
            }
        }
    }

    if !r.expressions.is_empty() {
        if !r.body.is_empty() {
            write!(f, ", ")?;
        }

        write!(f, "{}", r.expressions[0])?;

        if r.expressions.len() > 1 {
            for i in 1..r.expressions.len() {
                write!(f, ", {}", r.expressions[i])?;
            }
        }
    }

    Ok(())
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} <- ", self.head)?;

        display_rule_body(self, f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub queries: Vec<Rule>,
}

impl Check {
    pub fn convert(&self, symbols: &mut SymbolTable) -> datalog::Check {
        let mut queries = vec![];
        for q in self.queries.iter() {
            queries.push(q.convert(symbols));
        }

        datalog::Check { queries }
    }

    pub fn convert_from(r: &datalog::Check, symbols: &SymbolTable) -> Self {
        let mut queries = vec![];
        for q in r.queries.iter() {
            queries.push(Rule::convert_from(q, symbols));
        }

        Check { queries }
    }
}

impl TryFrom<Rule> for Check {
    type Error = error::Token;

    fn try_from(value: Rule) -> Result<Self, Self::Error> {
        Ok(Check {
            queries: vec![value],
        })
    }
}

impl TryFrom<&[Rule]> for Check {
    type Error = error::Token;

    fn try_from(values: &[Rule]) -> Result<Self, Self::Error> {
        Ok(Check {
            queries: values.to_vec(),
        })
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "check if ")?;

        if !self.queries.is_empty() {
            display_rule_body(&self.queries[0], f)?;

            if self.queries.len() > 1 {
                for i in 1..self.queries.len() {
                    write!(f, " or ")?;
                    display_rule_body(&self.queries[i], f)?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyKind {
    Allow,
    Deny,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    pub queries: Vec<Rule>,
    pub kind: PolicyKind,
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.queries.is_empty() {
            match self.kind {
                PolicyKind::Allow => write!(f, "allow if ")?,
                PolicyKind::Deny => write!(f, "deny if ")?,
            }

            if !self.queries.is_empty() {
                display_rule_body(&self.queries[0], f)?;

                if self.queries.len() > 1 {
                    for i in 1..self.queries.len() {
                        write!(f, " or ")?;
                        display_rule_body(&self.queries[i], f)?;
                    }
                }
            }
        } else {
            match self.kind {
                PolicyKind::Allow => write!(f, "allow")?,
                PolicyKind::Deny => write!(f, "deny")?,
            }
        }

        Ok(())
    }
}

/// creates a new fact
pub fn fact<I: AsRef<Term>>(name: &str, terms: &[I]) -> Fact {
    Fact(pred(name, terms))
}

/// creates a predicate
pub fn pred<I: AsRef<Term>>(name: &str, terms: &[I]) -> Predicate {
    Predicate {
        name: name.to_string(),
        terms: terms.iter().map(|term| term.as_ref().clone()).collect(),
    }
}

/// creates a rule
pub fn rule<T: AsRef<Term>, P: AsRef<Predicate>>(
    head_name: &str,
    head_terms: &[T],
    predicates: &[P],
) -> Rule {
    Rule {
        head: pred(head_name, head_terms),
        body: predicates.iter().map(|p| p.as_ref().clone()).collect(),
        expressions: Vec::new(),
    }
}

/// creates a rule with constraints
pub fn constrained_rule<T: AsRef<Term>, P: AsRef<Predicate>, E: AsRef<Expression>>(
    head_name: &str,
    head_terms: &[T],
    predicates: &[P],
    expressions: &[E],
) -> Rule {
    Rule {
        head: pred(head_name, head_terms),
        body: predicates.iter().map(|p| p.as_ref().clone()).collect(),
        expressions: expressions.iter().map(|c| c.as_ref().clone()).collect(),
    }
}

/// creates a check
pub fn check<P: AsRef<Predicate>>(predicates: &[P]) -> Check {
    let empty_terms: &[Term] = &[];
    Check {
        queries: vec![Rule {
            head: pred("query", empty_terms),
            body: predicates.iter().map(|p| p.as_ref().clone()).collect(),
            expressions: Vec::new(),
        }],
    }
}

/// creates an integer value
pub fn int(i: i64) -> Term {
    Term::Integer(i)
}

/// creates a string
pub fn string(s: &str) -> Term {
    Term::Str(s.to_string())
}

/// creates a string
pub fn s(s: &str) -> Term {
    Term::Str(s.to_string())
}

/// creates a date
///
/// internally the date will be stored as seconds since UNIX_EPOCH
pub fn date(t: &SystemTime) -> Term {
    let dur = t.duration_since(UNIX_EPOCH).unwrap();
    Term::Date(dur.as_secs())
}

/// creates a variable for a rule
pub fn var(s: &str) -> Term {
    Term::Variable(s.to_string())
}

/// creates a variable for a rule
pub fn variable(s: &str) -> Term {
    Term::Variable(s.to_string())
}

/// creates a byte array
pub fn bytes(s: &[u8]) -> Term {
    Term::Bytes(s.to_vec())
}

/// creates a boolean
pub fn boolean(b: bool) -> Term {
    Term::Bool(b)
}

/// creates a set
pub fn set(s: BTreeSet<Term>) -> Term {
    Term::Set(s)
}

impl TryFrom<Term> for i64 {
    type Error = error::Token;
    fn try_from(value: Term) -> Result<Self, Self::Error> {
        match value {
            Term::Integer(i) => Ok(i),
            _ => Err(error::Token::ConversionError(format!(
                "expected integer, got {:?}",
                value
            ))),
        }
    }
}

impl TryFrom<Term> for bool {
    type Error = error::Token;
    fn try_from(value: Term) -> Result<Self, Self::Error> {
        match value {
            Term::Bool(b) => Ok(b),
            _ => Err(error::Token::ConversionError(format!(
                "expected boolean, got {:?}",
                value
            ))),
        }
    }
}

impl TryFrom<Term> for String {
    type Error = error::Token;
    fn try_from(value: Term) -> Result<Self, Self::Error> {
        println!("converting string from {:?}", value);
        match value {
            Term::Str(s) => Ok(s),
            _ => Err(error::Token::ConversionError(format!(
                "expected string or symbol, got {:?}",
                value
            ))),
        }
    }
}

impl TryFrom<Term> for Vec<u8> {
    type Error = error::Token;
    fn try_from(value: Term) -> Result<Self, Self::Error> {
        match value {
            Term::Bytes(b) => Ok(b),
            _ => Err(error::Token::ConversionError(format!(
                "expected byte array, got {:?}",
                value
            ))),
        }
    }
}

impl TryFrom<Term> for SystemTime {
    type Error = error::Token;
    fn try_from(value: Term) -> Result<Self, Self::Error> {
        match value {
            Term::Date(d) => Ok(UNIX_EPOCH + Duration::from_secs(d)),
            _ => Err(error::Token::ConversionError(format!(
                "expected date, got {:?}",
                value
            ))),
        }
    }
}

impl<T: Ord + TryFrom<Term, Error = error::Token>> TryFrom<Term> for BTreeSet<T> {
    type Error = error::Token;
    fn try_from(value: Term) -> Result<Self, Self::Error> {
        match value {
            Term::Set(d) => d.iter().cloned().map(TryFrom::try_from).collect(),
            _ => Err(error::Token::ConversionError(format!(
                "expected set, got {:?}",
                value
            ))),
        }
    }
}

macro_rules! tuple_try_from(
    ($ty1:ident, $ty2:ident, $($ty:ident),*) => (
        tuple_try_from!(__impl $ty1, $ty2; $($ty),*);
        );
    (__impl $($ty: ident),+; $ty1:ident, $($ty2:ident),*) => (
        tuple_try_from_impl!($($ty),+);
        tuple_try_from!(__impl $($ty),+ , $ty1; $($ty2),*);
        );
    (__impl $($ty: ident),+; $ty1:ident) => (
        tuple_try_from_impl!($($ty),+);
        tuple_try_from_impl!($($ty),+, $ty1);
        );
    );

impl<A: TryFrom<Term, Error = error::Token>> TryFrom<Fact> for (A,) {
    type Error = error::Token;
    fn try_from(fact: Fact) -> Result<Self, Self::Error> {
        let mut terms = fact.0.terms;
        let mut it = terms.drain(..);

        Ok((it
            .next()
            .ok_or_else(|| error::Token::ConversionError("not enough terms in fact".to_string()))
            .and_then(A::try_from)?,))
    }
}

macro_rules! tuple_try_from_impl(
    ($($ty: ident),+) => (
        impl<$($ty: TryFrom<Term, Error = error::Token>),+> TryFrom<Fact> for ($($ty),+) {
            type Error = error::Token;
            fn try_from(fact: Fact) -> Result<Self, Self::Error> {
                let mut terms = fact.0.terms;
                let mut it = terms.drain(..);

                Ok((
                        $(
                            it.next().ok_or(error::Token::ConversionError("not enough terms in fact".to_string())).and_then($ty::try_from)?
                         ),+
                   ))

            }
        }
        );
    );

tuple_try_from!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U);
