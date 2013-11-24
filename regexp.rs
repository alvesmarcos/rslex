//
// rslex - a lexer generator for rust
//
// regexp.rs
// Regexp parsing and representation
//
// Andrei de A. Formiga, 2013-08-09
//

extern mod std;

use buffer::LookaheadBuffer;

#[deriving(Eq, Clone)]
enum Token { LBrack, RBrack, Id(~str), LParen, RParen, Asterisk, 
             Plus, Bar, Dash, String(~str), End, Eof, Error(char) }

#[deriving(Eq)]
pub enum ClassItem { Singles(~str), Range(char, char) }

#[deriving(Eq)]
pub enum Ast { Symb(~str), Str(~str), Union(~Ast, ~Ast),
               Conc(~Ast, ~Ast), Star(~Ast), OnePlus(~Ast), 
               CharClass(~[ClassItem]), Epsilon }

/// A token stream with capacity for lookahead of 1 token
struct TokenStream<'r> {
    buffer: &'r mut LookaheadBuffer<'r>,
    term: &'r [char],
    peek: Option<Token>
}

impl<'r> TokenStream<'r> {
    pub fn new(buffer: &'r mut LookaheadBuffer<'r>, term: &'r [char]) -> TokenStream<'r> {
        TokenStream { buffer: buffer, term: term, peek: None }
    }

    fn next_token(&mut self) -> Token {
        let res = match self.peek {
            None => self.next_token_raw(),
            Some(ref t) => (*t).clone()
        };
        self.peek = None;
        res
    }

    fn next_token_raw(&mut self) -> Token {
        self.buffer.skip_whitespace();
        match self.buffer.next_char() {
            Some('[') => LBrack,
            Some(']') => RBrack,
            Some('(') => LParen,
            Some(')') => RParen,
            Some('*') => Asterisk,
            Some('+') => Plus,
            Some('|') => Bar,
            Some('-') => Dash,
            Some('\'') => String(self.parse_string('\'')),
            Some('"') => String(self.parse_string('"')),
            Some(c) if std::char::is_alphabetic(c) => Id(self.parse_id(c)),
            Some(c) if is_terminator(c, self.term) => End,
            None => Eof,
            Some(c) => Error(c)
        }
    }

    fn return_token(&mut self, tok: Token) {
        self.peek = Some(tok);
    }

    fn parse_string(&mut self, delim: char) -> ~str {
        let mut res : ~str = ~"";
        loop {
            match self.buffer.next_char() {
                None => fail!("Unexpected end of file. Expected closing {}", delim),
                Some(c) if c == delim => break,
                Some(c) => res.push_char(c)
            }
        }
        res    
    }

    fn parse_id(&mut self, first: char) -> ~str {
        let mut res : ~str = ~"";
        res.push_char(first);
        loop {
            match self.buffer.next_char() {
                Some(c) if is_id_char(c) => res.push_char(c),
                Some(c) => { self.buffer.return_char(c); break }
                None => break
            }
        }
        res
    }
}


#[inline]
fn is_id_char(c: char) -> bool {
    std::char::is_alphanumeric(c) || c == '_'
}

#[inline]
fn match_next_token(ts: &mut TokenStream, t: Token) {
    let rt = ts.next_token();
    if rt != t {
        fail!("Unexpeced token: expected {:?}, got {:?}", t, rt);
    }
}

#[inline]
fn is_terminator(c: char, term: &[char]) -> bool {
    term.contains(&c)
}


// regexp := union
// union  := union '|' concat | concat
// concat := concat factor | factor
// factor := (regexp) | regexp'*' | regexp'+' | class | id | str
// class  := '[' (char | range)* ']'
// range  := char'-'char

// parse a regexp from the token stream until one of the terminators in term occurs
pub fn parse_regexp(ts: &mut TokenStream) -> Ast {
    parse_union(ts)
}

fn parse_union(ts: &mut TokenStream) -> Ast {
    let left = parse_concat(ts);
    match ts.next_token() {
        Bar => {
            let right = parse_union(ts);
            Union(~left, ~right)
        }
        tok => {
            ts.return_token(tok);
            left
        }
    }
}

fn parse_concat(ts: &mut TokenStream) -> Ast {
    let left = parse_factor(ts);
    match ts.next_token() {
        Bar => {
            ts.return_token(Bar);
            left
        }
        End => {
            ts.return_token(End);
            left
        }
        RParen => {
            ts.return_token(RParen);
            left
        }
        tok => {
            ts.return_token(tok);
            let right = parse_concat(ts);
            Conc(~left, ~right)
        }
    }
}

fn trailing_closure(ts: &mut TokenStream) -> Option<Token> {
    match ts.next_token() {
        Asterisk => Some(Asterisk),
        Plus => Some(Plus),
        t => { ts.return_token(t); None }
    }
}

fn parse_character_class(ts: &mut TokenStream) -> Ast {
    let mut res = std::vec::with_capacity(2);
    loop {
        match ts.next_token() {
            String(s1) => {
                match ts.next_token() {
                    Dash => {
                        match ts.next_token() {
                            String(s2) => res.push(Range(s1.char_at(0), s2.char_at(0))),
                            _ => fail!("Ill-formed character class range")
                        }
                    }
                    tok => {
                        ts.return_token(tok);
                        res.push(Singles(s1))
                    }
                }
            }
            Dash => fail!("Ill-formed character class range"),
            RBrack => break,
            tok => fail!("Unexpected token in character class: {:?}", tok)
        }
    }
    CharClass(res)
}

#[inline]
fn parse_factor(ts: &mut TokenStream) -> Ast {
    let pre = match ts.next_token() {
        LParen => { let e = parse_regexp(ts); 
                    match_next_token(ts, RParen); 
                    e }
        LBrack => parse_character_class(ts),
        Id(s) => Symb(s),
        String(s) => Str(s),
        tok => fail!("Unexpected token in regexp: {:?}", tok)
    };
    match trailing_closure(ts) {
        Some(Asterisk) => Star(~pre),
        Some(Plus) => OnePlus(~pre),
        Some(_) => fail!("Unexpected closure character"),
        None => pre
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{LBrack, RBrack, Id, LParen, RParen, Asterisk, Bar, Dash, String, End, Eof, Error };
    use super::{CharClass};
    use super::{parse_character_class};
    use buffer::LookaheadBuffer;

    #[test]
    fn test_parse_string_ts() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("abc'* ");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(ts1.parse_string('\''), ~"abc");
        assert_eq!(ts1.buffer.next_char(), Some('*'));

        let mut b2 = LookaheadBuffer::new("abc'def\"  ");
        let mut ts2 = TokenStream::new(&mut b2, term);
        assert_eq!(ts2.parse_string('"'), ~"abc'def");
        assert_eq!(ts2.buffer.next_char(), Some(' ')); 
    }

    #[test]
    #[should_fail]
    fn unclosed_string_ts() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("abc'def  ");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(ts1.parse_string('"'), ~"abc'def");
    }

    #[test]
    fn test_parse_id_ts() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("abc'* ");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(ts1.parse_id('x'), ~"xabc");
        assert_eq!(ts1.buffer.next_char(), Some('\''));

        let mut b2 = LookaheadBuffer::new("bc_def   ");
        let mut ts2 = TokenStream::new(&mut b2, term);
        assert_eq!(ts2.parse_id('a'), ~"abc_def");

        let mut b3 = LookaheadBuffer::new("_times|'xy')*   ");
        let mut ts3 = TokenStream::new(&mut b3, term);
        assert_eq!(ts3.parse_id('n'), ~"n_times");
        assert_eq!(ts3.buffer.next_char(), Some('|'));

        let mut b4 = LookaheadBuffer::new(" +xy)*   ");
        let mut ts4 = TokenStream::new(&mut b4, term);
        assert_eq!(ts4.parse_id('n'), ~"n");
        assert_eq!(ts4.buffer.next_char(), Some(' '));
        assert_eq!(ts4.buffer.next_char(), Some('+'));
        assert_eq!(ts4.buffer.next_char(), Some('x'));
        assert_eq!(ts4.parse_id('x'), ~"xy");
    }

    #[test]
    fn test_next_token_raw() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("'return'");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(ts1.next_token(), String(~"return"));

        let mut b2 = LookaheadBuffer::new("return");
        let mut ts2 = TokenStream::new(&mut b2, term);
        assert_eq!(ts2.next_token(), Id(~"return"));
        ts2.return_token(Id(~"return"));
        assert_eq!(ts2.next_token(), Id(~"return"));
        assert_eq!(ts2.next_token(), Eof);

        let mut b3 = LookaheadBuffer::new("(['a'-'z'])(['A'-'Z'])*");
        let mut ts3 = TokenStream::new(&mut b3, term);
        assert_eq!(ts3.next_token(), LParen);
        assert_eq!(ts3.next_token(), LBrack);
        assert_eq!(ts3.next_token(), String(~"a"));
        assert_eq!(ts3.next_token(), Dash);
        assert_eq!(ts3.next_token(), String(~"z"));
        assert_eq!(ts3.next_token(), RBrack);
        assert_eq!(ts3.next_token(), RParen);
        assert_eq!(ts3.next_token(), LParen);

        assert_eq!(ts3.peek, None);
        ts3.return_token(LParen);
        assert!(!ts3.peek.is_none());
        assert_eq!(ts3.next_token(), LParen);
        assert_eq!(ts3.peek, None);

        assert_eq!(ts3.next_token(), LBrack);
        assert_eq!(ts3.next_token(), String(~"A"));
        assert_eq!(ts3.next_token(), Dash);
        assert_eq!(ts3.next_token(), String(~"Z"));
        assert_eq!(ts3.next_token(), RBrack);
        assert_eq!(ts3.next_token(), RParen);
        assert_eq!(ts3.next_token(), Asterisk);
        assert_eq!(ts3.next_token(), Eof);

        let mut b4 = LookaheadBuffer::new("letter \t (letter | digit)*,");
        let mut ts4 = TokenStream::new(&mut b4, term);
        assert_eq!(ts4.next_token(), Id(~"letter"));
        assert_eq!(ts4.next_token(), LParen);
        assert_eq!(ts4.next_token(), Id(~"letter"));
        assert_eq!(ts4.next_token(), Bar);
        assert_eq!(ts4.next_token(), Id(~"digit"));
        assert_eq!(ts4.next_token(), RParen);
        assert_eq!(ts4.next_token(), Asterisk);
        assert_eq!(ts4.next_token(), End);

        let mut b5 = LookaheadBuffer::new("let  & dig,");
        let mut ts5 = TokenStream::new(&mut b5, term);
        assert_eq!(ts5.next_token(), Id(~"let"));
        assert_eq!(ts5.next_token(), Error('&'));
    }

    #[test]
    fn test_parse_charclass() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("'A'-'Z'],");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(parse_character_class(&mut ts1), CharClass(~[Range('A', 'Z')]));
        assert_eq!(ts1.next_token(), End);

        let mut b2 = LookaheadBuffer::new("]");
        let mut ts2 = TokenStream::new(&mut b2, term);
        assert_eq!(parse_character_class(&mut ts2), CharClass(~[]));

        let mut b3 = LookaheadBuffer::new("'abcABC']");
        let mut ts3 = TokenStream::new(&mut b3, term);
        assert_eq!(parse_character_class(&mut ts3), CharClass(~[Singles(~"abcABC")]));

        let mut b4 = LookaheadBuffer::new("'ab''cd''0'-'9''55']");
        let mut ts4 = TokenStream::new(&mut b4, term);
        assert_eq!(parse_character_class(&mut ts4), 
                   CharClass(~[Singles(~"ab"), Singles(~"cd"), 
                               Range('0', '9'), Singles(~"55")]));
    }

    #[test]
    #[should_fail]
    fn test_bad_charclass() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("'A'--'Z'],");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(parse_character_class(&mut ts1), CharClass(~[Range('A', 'Z')]));
    }

    #[test]
    #[should_fail]
    fn test_bad_charclass2() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("'A'*'Z'],");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(parse_character_class(&mut ts1), CharClass(~[Range('A', 'Z')]));
    }

    #[test]
    fn test_parse_regexp() {
        let term = [','];
        let mut b1 = LookaheadBuffer::new("letter,");
        let mut ts1 = TokenStream::new(&mut b1, term);
        assert_eq!(parse_regexp(&mut ts1), Symb(~"letter"));

        let mut b2 = LookaheadBuffer::new("letter*,");
        let mut ts2 = TokenStream::new(&mut b2, term);
        assert_eq!(parse_regexp(&mut ts2), Star(~Symb(~"letter")));

        let mut b3 = LookaheadBuffer::new("letter (letter | digit)*,");
        let mut ts3 = TokenStream::new(&mut b3, term);
        assert_eq!(parse_regexp(&mut ts3), 
                   Conc(~Symb(~"letter"), ~Star(~Union(~Symb(~"letter"), ~Symb(~"digit")))));
        assert_eq!(ts3.next_token(), End);

        let mut b4 = LookaheadBuffer::new("['0'-'9']+ '.' ['0'-'9']+,");
        let mut ts4 = TokenStream::new(&mut b4, term);
        assert_eq!(parse_regexp(&mut ts4),
                   Conc(~OnePlus(~CharClass(~[Range('0', '9')])), 
                        ~Conc(~Str(~"."), ~OnePlus(~CharClass(~[Range('0', '9')])))));
    }
}
