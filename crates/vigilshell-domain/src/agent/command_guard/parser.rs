use super::AgentResult;
#[derive(Debug)]
pub(super) struct Node {
    pub words: Vec<String>,
    pub redirects: Vec<(String, String)>,
    pub next: Option<String>,
}
#[derive(Debug)]
enum Token {
    Word(String),
    Op(String),
}

pub(super) fn parse(source: &str) -> AgentResult<Vec<Node>> {
    if source.len() > 64 * 1024 {
        return Err("command_too_large");
    }
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None;
    let mut chars = source.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\0' || c.is_control() && !matches!(c, '\t' | '\n') {
            return Err("invalid_command_syntax");
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else if q == '"' && matches!(c, '$' | '`' | '\\') {
                return Err("unsupported_command_syntax");
            } else {
                word.push(c);
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                started = true;
            }
            '\\' => {
                let escaped = chars.next().ok_or("invalid_command_syntax")?;
                if escaped.is_control() {
                    return Err("invalid_command_syntax");
                }
                word.push(escaped);
                started = true;
            }
            ' ' | '\t' => {
                if started {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                    started = false;
                }
            }
            ';' | '\n' | '|' | '&' | '>' | '<' => {
                if started {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                    started = false;
                }
                let mut op = if c == '\n' {
                    ";".to_string()
                } else {
                    c.to_string()
                };
                if chars.peek() == Some(&c) && matches!(c, '|' | '&' | '>') {
                    chars.next();
                    op.push(c);
                }
                if op == "&" || op == "<" {
                    return Err("unsupported_command_syntax");
                }
                tokens.push(Token::Op(op));
            }
            '$' | '`' | '*' | '?' | '[' | ']' | '{' | '}' | '(' | ')' | '~' | '#' => {
                return Err("unsupported_command_syntax");
            }
            _ => {
                word.push(c);
                started = true;
            }
        }
        if tokens.len() > 512 || word.len() > 32768 {
            return Err("command_too_large");
        }
    }
    if quote.is_some() {
        return Err("invalid_command_syntax");
    }
    if started {
        tokens.push(Token::Word(word));
    }
    if tokens.len() > 512 {
        return Err("command_too_large");
    }
    let mut nodes = Vec::new();
    let mut node = Node {
        words: vec![],
        redirects: vec![],
        next: None,
    };
    let mut tokens = tokens.into_iter();
    while let Some(token) = tokens.next() {
        match token {
            Token::Word(word) => node.words.push(word),
            Token::Op(op) if matches!(op.as_str(), ">" | ">>") => {
                if node.words.is_empty() {
                    return Err("invalid_command_syntax");
                }
                let Some(Token::Word(target)) = tokens.next() else {
                    return Err("invalid_command_syntax");
                };
                // 尚不接受 FD 重定向，不能错误重建为位置参数。
                if node
                    .words
                    .last()
                    .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
                {
                    return Err("unsupported_command_syntax");
                }
                node.redirects.push((op, target));
            }
            Token::Op(op) => {
                if node.words.is_empty() {
                    return Err("invalid_command_syntax");
                }
                node.next = Some(op);
                nodes.push(node);
                node = Node {
                    words: vec![],
                    redirects: vec![],
                    next: None,
                };
            }
        }
    }
    if node.words.is_empty() {
        return Err("invalid_command_syntax");
    }
    nodes.push(node);
    if nodes.len() > 32
        || nodes
            .iter()
            .any(|n| n.words[0].is_empty() || n.words[0].contains('='))
    {
        return Err("invalid_command_syntax");
    }
    Ok(nodes)
}
