//! フロントエンド: ソース → full-moon AST → HIR。require / dofile で読み込まれるファイルも解決する。

pub mod literal;
mod lower;

use std::path::{Path, PathBuf};

use full_moon::ast::Ast;
use full_moon::LuaVersion;

use crate::diag::{Diagnostics, Location};
use crate::hir::{FileId, Program, SourceFile, SourceKind, Span};
use crate::spec::Registry;

pub(crate) use lower::Lowerer;

/// LuaJIT の構文(goto / ラベル、cdata 数値、`\x` `\z` エスケープ)でパースする。
pub fn parse(source: &str) -> Result<Ast, Vec<full_moon::Error>> {
    full_moon::parse_fallible(source, LuaVersion::luajit()).into_result()
}

/// フロントエンドの結果。CPU 側の書き換えのため、各ファイルの AST も保持する。
pub struct Frontend {
    pub program: Program,
    /// `program.files` と同じ順序の AST。
    pub asts: Vec<Ast>,
}

/// メインチャンクと、そこから定数パスで参照されるモジュール / dofile を読み込む。
pub fn build(
    source: &str,
    chunk_name: &str,
    base_dir: Option<&Path>,
    registry: &Registry,
    diags: &mut Diagnostics,
) -> Option<Frontend> {
    let mut fe = Frontend {
        program: Program::default(),
        asts: Vec::new(),
    };
    let main = add_file(
        &mut fe,
        registry,
        diags,
        chunk_name.to_string(),
        None,
        source.to_string(),
        SourceKind::Main,
        base_dir,
    )?;
    fe.program.main_file = main;
    Some(fe)
}

#[allow(clippy::too_many_arguments)]
fn add_file(
    fe: &mut Frontend,
    registry: &Registry,
    diags: &mut Diagnostics,
    chunk: String,
    path: Option<PathBuf>,
    source: String,
    kind: SourceKind,
    base_dir: Option<&Path>,
) -> Option<FileId> {
    let ast = match parse(&source) {
        Ok(ast) => ast,
        Err(errors) => {
            for e in errors {
                let (start, _) = e.range();
                diags.error(
                    "syntax",
                    Location::new(chunk.clone(), start.line() as u32, start.character() as u32),
                    e.error_message().into_owned(),
                );
            }
            return None;
        }
    };
    let id = fe.program.files.len() as FileId;
    fe.program.files.push(SourceFile {
        id,
        chunk,
        path,
        source,
        kind,
        main: 0,
    });

    let span = Span {
        file: id,
        start: 0,
        end: fe.program.files[id as usize].source.len() as u32,
        line: 1,
        col: 1,
    };
    let (main, requires, dofiles) = {
        let mut lw = Lowerer::new(&mut fe.program, registry, diags, id);
        let main = lw.lower_chunk(ast.nodes(), span);
        (main, lw.requires, lw.dofiles)
    };
    fe.program.files[id as usize].main = main;
    fe.asts.push(ast);

    for (name, span) in requires {
        if fe.program.modules.contains_key(&name) {
            continue;
        }
        match find_module(&name, &registry.options.package_path, base_dir) {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(src) => {
                    // 循環 require に備えて、パースより先に登録しておく。
                    let placeholder = fe.program.files.len() as FileId;
                    fe.program.modules.insert(name.clone(), placeholder);
                    match add_file(fe, registry, diags, name.clone(), Some(p), src, SourceKind::Module, base_dir) {
                        Some(fid) => {
                            fe.program.modules.insert(name, fid);
                        }
                        None => {
                            fe.program.modules.shift_remove(&name);
                        }
                    }
                }
                Err(e) => diags.error(
                    "module-read",
                    fe.program.location(span),
                    format!("cannot read module `{name}` ({}): {e}", p.display()),
                ),
            },
            None => diags.warning(
                "module-not-found",
                fe.program.location(span),
                format!(
                    "module `{name}` was not found at convert time; it is left to the runtime `require` \
                     (its functions cannot be used inside GPU kernels)"
                ),
            ),
        }
    }

    for (rel, span) in dofiles {
        if fe.program.dofiles.contains_key(&rel) {
            continue;
        }
        let p = resolve_relative(Path::new(&rel), base_dir);
        match std::fs::read_to_string(&p) {
            Ok(src) => {
                let placeholder = fe.program.files.len() as FileId;
                fe.program.dofiles.insert(rel.clone(), placeholder);
                match add_file(fe, registry, diags, rel.clone(), Some(p), src, SourceKind::Dofile, base_dir) {
                    Some(fid) => {
                        fe.program.dofiles.insert(rel, fid);
                    }
                    None => {
                        fe.program.dofiles.shift_remove(&rel);
                    }
                }
            }
            Err(_) => diags.warning(
                "dofile-not-found",
                fe.program.location(span),
                format!(
                    "file `{rel}` for dofile was not found at convert time; it is left to the runtime"
                ),
            ),
        }
    }

    Some(id)
}

fn resolve_relative(p: &Path, base_dir: Option<&Path>) -> PathBuf {
    match base_dir {
        Some(base) if p.is_relative() => base.join(p),
        _ => p.to_path_buf(),
    }
}

/// `package.path` 形式の検索パスからモジュールのファイルを探す。
pub fn find_module(name: &str, package_path: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let rel = name.replace('.', std::path::MAIN_SEPARATOR_STR);
    for template in package_path.split(';') {
        let template = template.trim();
        if template.is_empty() || !template.contains('?') {
            continue;
        }
        let candidate = resolve_relative(Path::new(&template.replace('?', &rel)), base_dir);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
