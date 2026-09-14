//! `outline`: item signatures with line ranges, bodies omitted.

use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{Attribute, Fields, ImplItem, Item, TraitItem};

/// One printed row: 1-based inclusive line range, nesting depth, text, and
/// the first doc-comment line when `--docs` is set.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub depth: usize,
    pub text: String,
    pub doc: Option<String>,
}

pub fn run(paths: &[PathBuf], docs: bool) -> anyhow::Result<u8> {
    let mut files = Vec::new();
    let mut failed = false;
    for path in paths {
        if path.is_dir() {
            collect_rs_files(path, &mut files);
        } else {
            files.push(path.clone());
        }
    }
    let mut stdout = io::stdout().lock();
    for file in files {
        match fs::read_to_string(&file) {
            Ok(src) => {
                if let Err(err) = stdout.write_all(render_file(&file, &src, docs).as_bytes()) {
                    if err.kind() == ErrorKind::BrokenPipe {
                        return Ok(0);
                    }
                    return Err(err.into());
                }
            }
            Err(err) => {
                eprintln!("{}: {err}", file.display());
                failed = true;
            }
        }
    }
    Ok(u8::from(failed))
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                collect_rs_files(&path, out);
            }
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// Render one file's outline, including its header line.
pub fn render_file(path: &Path, src: &str, docs: bool) -> String {
    let mut out = format!("{}\n", path.display());
    let rows = match syn::parse_file(src) {
        Ok(file) => {
            let mut rows = Vec::new();
            items(&file.items, 0, docs, &mut rows);
            rows
        }
        Err(err) => {
            let at = err.span().start();
            out = format!(
                "{}: parse error at {}:{}; approximate outline\n",
                path.display(),
                at.line,
                at.column + 1
            );
            approximate(src)
        }
    };
    for row in rows {
        let range = format!("{}-{}", row.start, row.end);
        let indent = "  ".repeat(row.depth);
        out.push_str(&format!("  {range:<9} {indent}{}\n", row.text));
        if let Some(doc) = row.doc {
            out.push_str(&format!("  {:<9} {indent}  /// {doc}\n", ""));
        }
    }
    out
}

fn items(items: &[Item], depth: usize, docs: bool, rows: &mut Vec<Row>) {
    for item in items {
        item_rows(item, depth, docs, rows);
    }
}

fn item_rows(item: &Item, depth: usize, docs: bool, rows: &mut Vec<Row>) {
    let (start, end) = range_without_attrs(item);
    let push = |rows: &mut Vec<Row>, text: String, attrs: &[Attribute]| {
        let doc = if docs { first_doc_line(attrs) } else { None };
        rows.push(Row {
            start,
            end,
            depth,
            text,
            doc,
        });
    };
    match item {
        Item::Fn(f) => {
            let mut f = f.clone();
            f.attrs.clear();
            f.block = Box::new(syn::parse_quote!({}));
            push(rows, render(Item::Fn(f)), &item_attrs(item));
        }
        Item::Struct(s) => {
            let mut s = s.clone();
            s.attrs.clear();
            s.fields.iter_mut().for_each(|field| field.attrs.clear());
            push(rows, render(Item::Struct(s)), &item_attrs(item));
        }
        Item::Union(u) => {
            let mut u = u.clone();
            u.attrs.clear();
            u.fields
                .named
                .iter_mut()
                .for_each(|field| field.attrs.clear());
            push(rows, render(Item::Union(u)), &item_attrs(item));
        }
        Item::Enum(e) => {
            let names: Vec<String> = e
                .variants
                .iter()
                .map(|v| match v.fields {
                    Fields::Unit => v.ident.to_string(),
                    Fields::Unnamed(_) => format!("{}(..)", v.ident),
                    Fields::Named(_) => format!("{} {{ .. }}", v.ident),
                })
                .collect();
            let mut header = e.clone();
            header.attrs.clear();
            header.variants.clear();
            let text = format!("{} {{ {} }}", render(Item::Enum(header)), names.join(", "));
            push(rows, text, &item_attrs(item));
        }
        Item::Trait(t) => {
            let mut header = t.clone();
            header.attrs.clear();
            header.items.clear();
            push(rows, render(Item::Trait(header)), &item_attrs(item));
            for inner in &t.items {
                let (start, end) = range_without_attrs(inner);
                let text = render_trait_item(inner);
                let doc = if docs {
                    first_doc_line(&trait_item_attrs(inner))
                } else {
                    None
                };
                rows.push(Row {
                    start,
                    end,
                    depth: depth + 1,
                    text,
                    doc,
                });
            }
        }
        Item::Impl(i) => {
            let mut header = i.clone();
            header.attrs.clear();
            header.items.clear();
            push(rows, render(Item::Impl(header)), &item_attrs(item));
            for inner in &i.items {
                let (start, end) = range_without_attrs(inner);
                let text = render_impl_item(inner);
                let doc = if docs {
                    first_doc_line(&impl_item_attrs(inner))
                } else {
                    None
                };
                rows.push(Row {
                    start,
                    end,
                    depth: depth + 1,
                    text,
                    doc,
                });
            }
        }
        Item::Mod(m) => {
            let mut header = m.clone();
            header.attrs.clear();
            header.content = None;
            header.semi = Some(Default::default());
            let text = render(Item::Mod(header));
            match &m.content {
                Some((_, inner)) if is_cfg_test(&m.attrs) => {
                    let text = format!("#[cfg(test)] {text}  ({} items)", inner.len());
                    rows.push(Row {
                        start,
                        end,
                        depth,
                        text,
                        doc: None,
                    });
                }
                Some((_, inner)) => {
                    push(rows, text, &m.attrs);
                    items(inner, depth + 1, docs, rows);
                }
                None => push(rows, text, &m.attrs),
            }
        }
        Item::Const(c) => {
            let mut c = c.clone();
            c.attrs.clear();
            c.expr = Box::new(syn::parse_quote!(_));
            let text = render(Item::Const(c));
            push(
                rows,
                text.trim_end_matches(" = _").to_string(),
                &item_attrs(item),
            );
        }
        Item::Static(s) => {
            let mut s = s.clone();
            s.attrs.clear();
            s.expr = Box::new(syn::parse_quote!(_));
            let text = render(Item::Static(s));
            push(
                rows,
                text.trim_end_matches(" = _").to_string(),
                &item_attrs(item),
            );
        }
        Item::Type(t) => {
            let mut t = t.clone();
            t.attrs.clear();
            push(rows, render(Item::Type(t)), &item_attrs(item));
        }
        Item::Macro(m) => {
            if let Some(ident) = &m.ident {
                push(rows, format!("macro_rules! {ident}"), &m.attrs);
            }
        }
        _ => {}
    }
}

fn render_trait_item(item: &TraitItem) -> String {
    let mut item = item.clone();
    match &mut item {
        TraitItem::Fn(f) => {
            f.attrs.clear();
            f.default = None;
            f.semi_token = Some(Default::default());
        }
        TraitItem::Const(c) => c.attrs.clear(),
        TraitItem::Type(t) => t.attrs.clear(),
        TraitItem::Macro(m) => m.attrs.clear(),
        _ => {}
    }
    let wrapper: Item = syn::parse_quote!(trait __RustAiLean { #item });
    inner_of(&render_raw(wrapper))
}

fn render_impl_item(item: &ImplItem) -> String {
    let mut item = item.clone();
    match &mut item {
        ImplItem::Fn(f) => {
            f.attrs.clear();
            f.block = syn::parse_quote!({});
        }
        ImplItem::Const(c) => {
            c.attrs.clear();
            c.expr = syn::parse_quote!(_);
        }
        ImplItem::Type(t) => t.attrs.clear(),
        ImplItem::Macro(m) => m.attrs.clear(),
        _ => {}
    }
    let wrapper: Item = syn::parse_quote!(impl __RustAiLean { #item });
    inner_of(&render_raw(wrapper))
        .trim_end_matches(" = _")
        .to_string()
}

/// Pretty-print a single item as a one-line signature with its body removed.
fn render(item: Item) -> String {
    strip_body(&collapse(&render_raw(item)))
}

fn render_raw(item: Item) -> String {
    prettyplease::unparse(&syn::File {
        shebang: None,
        attrs: Vec::new(),
        items: vec![item],
    })
}

/// Text between a wrapper's first line (`trait X {`) and its closing `}`.
fn inner_of(rendered: &str) -> String {
    let lines: Vec<&str> = rendered.lines().collect();
    let inner = lines
        .get(1..lines.len().saturating_sub(1))
        .unwrap_or(&[])
        .join("\n");
    strip_body(&collapse(&inner))
}

/// Join all lines into one and undo the line-wrapping punctuation.
fn collapse(text: &str) -> String {
    let mut s = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [
        (", )", ")"),
        ("( ", "("),
        (" )", ")"),
        (", >", ">"),
        ("< ", "<"),
        (", }", " }"),
        (", ]", "]"),
        ("[ ", "["),
    ] {
        while s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s
}

fn strip_body(text: &str) -> String {
    let t = text.trim_end();
    let t = t.strip_suffix("{}").map(str::trim_end).unwrap_or(t);
    let t = t.strip_suffix(';').unwrap_or(t);
    let t = t.strip_suffix(',').unwrap_or(t);
    t.trim_end().to_string()
}

fn item_attrs(item: &Item) -> Vec<Attribute> {
    match item {
        Item::Fn(x) => x.attrs.clone(),
        Item::Struct(x) => x.attrs.clone(),
        Item::Union(x) => x.attrs.clone(),
        Item::Enum(x) => x.attrs.clone(),
        Item::Trait(x) => x.attrs.clone(),
        Item::Impl(x) => x.attrs.clone(),
        Item::Const(x) => x.attrs.clone(),
        Item::Static(x) => x.attrs.clone(),
        Item::Type(x) => x.attrs.clone(),
        _ => Vec::new(),
    }
}

fn trait_item_attrs(item: &TraitItem) -> Vec<Attribute> {
    match item {
        TraitItem::Fn(x) => x.attrs.clone(),
        TraitItem::Const(x) => x.attrs.clone(),
        TraitItem::Type(x) => x.attrs.clone(),
        TraitItem::Macro(x) => x.attrs.clone(),
        _ => Vec::new(),
    }
}

fn impl_item_attrs(item: &ImplItem) -> Vec<Attribute> {
    match item {
        ImplItem::Fn(x) => x.attrs.clone(),
        ImplItem::Const(x) => x.attrs.clone(),
        ImplItem::Type(x) => x.attrs.clone(),
        ImplItem::Macro(x) => x.attrs.clone(),
        _ => Vec::new(),
    }
}

fn first_doc_line(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("doc") {
            return None;
        }
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return None;
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = &nv.value
        else {
            return None;
        };
        let line = s.value().trim().to_string();
        (!line.is_empty()).then_some(line)
    })
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Ident>()
                .map(|id| id == "test")
                .unwrap_or(false)
    })
}

/// Line range of a node, starting at its first token after outer attributes.
fn range_without_attrs<T: ToTokens>(node: &T) -> (usize, usize) {
    let tokens: Vec<_> = strip_outer_attrs(node.to_token_stream())
        .into_iter()
        .collect();
    let start = tokens.first().map(|t| t.span().start().line).unwrap_or(0);
    let end = tokens.last().map(|t| t.span().end().line).unwrap_or(start);
    (start, end)
}

/// Drop leading `# [ ... ]` groups (outer attributes and doc comments).
fn strip_outer_attrs(tokens: TokenStream) -> TokenStream {
    use proc_macro2::{Delimiter, TokenTree};
    let mut iter = tokens.into_iter().peekable();
    loop {
        match iter.peek() {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                let hash = iter.next();
                match iter.peek() {
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => {
                        iter.next();
                    }
                    _ => return hash.into_iter().chain(iter).collect(),
                }
            }
            _ => return iter.collect(),
        }
    }
}

/// Fallback for files that do not parse: keyword-based line scan.
fn approximate(src: &str) -> Vec<Row> {
    const STARTS: &[&str] = &[
        "fn ",
        "async fn ",
        "const fn ",
        "unsafe fn ",
        "extern fn ",
        "struct ",
        "enum ",
        "union ",
        "trait ",
        "unsafe trait ",
        "impl ",
        "impl<",
        "unsafe impl",
        "mod ",
        "type ",
        "const ",
        "static ",
        "macro_rules!",
    ];
    src.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let indent = line.len() - line.trim_start().len();
            let mut rest = line.trim();
            if let Some(r) = rest.strip_prefix("pub") {
                rest = match r.strip_prefix('(') {
                    Some(r) => r.split_once(')').map(|(_, r)| r).unwrap_or(r),
                    None => r,
                }
                .trim_start();
            }
            STARTS.iter().any(|s| rest.starts_with(s)).then(|| Row {
                start: i + 1,
                end: i + 1,
                depth: indent / 4,
                text: strip_body(line.trim())
                    .trim_end_matches('{')
                    .trim_end()
                    .to_string(),
                doc: None,
            })
        })
        .collect()
}
