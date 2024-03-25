use std::collections::VecDeque;
use cpp_demangle::DemangleOptions;
use anyhow::Result;
use cpp_demangle::{DemangleNodeType, DemangleWrite, Symbol};

#[derive(Debug, Clone)]
enum NodeType {
    Node(DemangleNodeType),
    String(String),
}

#[derive(Debug, Clone)]
struct Node {
    nt: NodeType,
    children: Vec<Node>,
}

pub fn structured_demangle(sym: &Symbol<&str>) -> Result<Vec<Node>> {
    struct S {
        stack: VecDeque<Node>,
        results: Vec<Node>,
    }

    impl DemangleWrite for S {
        fn push_demangle_node(&mut self, nt: DemangleNodeType) {
            println!("  node: {}{nt:?}:", "  ".repeat(self.stack.len()));
            self.stack.push_back(Node::new(nt));

        }

        fn write_string(&mut self, s: &str) -> std::fmt::Result {
            println!("string: {}{s:?}", "  ".repeat(self.stack.len()));
            self.stack.push_back(Node::new_string(s.to_string()));
            Ok(())
        }

        fn pop_demangle_node(&mut self) {
            println!("   pop: ");

            let mut strings = Vec::new();
            while matches!(self.stack.back().unwrap().nt, NodeType::String(_)) {
                if let Some(NodeType::String(s)) = self.stack.pop_back().map(|n| n.nt) {
                    strings.push(s);
                } else {
                    unreachable!()
                }
            }
            let mut child = self.stack.pop_back().expect("pop with no child");
            child.children.extend(strings.into_iter().map(Node::new_string));
            // println!("  child: {:#?}", child);
            match self.stack.back_mut() {
                Some(Node { children, .. }) => {
                    children.push(child)
                },
                None => self.results.push(child),
            }
        }
    }

    let mut s = S { stack: VecDeque::new(), results: Vec::new() };
    sym.structured_demangle(&mut s, &DemangleOptions::default())?;
    println!();
    println!("results");
    println!("{:#?}", s.results);
    println!();
    println!("stack");
    println!("{:#?}", s.stack);

    Ok(s.stack.into())
    // s.stack.pop_back().ok_or(anyhow::anyhow!("No root node"))
}

impl Node {
    fn new(nt: DemangleNodeType) -> Self {
        Self {
            nt: NodeType::Node(nt),
            children: Vec::with_capacity(1),
        }
    }

    fn new_string(s: String) -> Self {
        Self {
            nt: NodeType::String(s),
            children: Vec::with_capacity(0),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_structured_demangle() {
        let sym = Symbol::new("_ZN9LuaEntity23luaReadProductsFinishedEP9lua_State").unwrap();
        println!("{:?}", structured_demangle(&sym).unwrap());
    }
}
