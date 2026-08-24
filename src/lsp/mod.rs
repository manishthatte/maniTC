// lsp/mod.rs — ManiT Language Server Protocol implementation
//
// Provides real-time diagnostics, hover, and completion for .mt files.
// Uses the compiler frontend (lexer → parser → semantic analyzer) to
// produce LSP diagnostics on every file open/change/save.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tokio::sync::RwLock;
use std::collections::HashMap;

use crate::error::{CompileError, Diagnostic as ManiDiagnostic};
use crate::lexer::Lexer;
use crate::parser::Parser as ManiParser;
use crate::semantic::SemanticAnalyzer;

// ---------------------------------------------------------------------------
// Language server state
// ---------------------------------------------------------------------------

pub struct ManiTLanguageServer {
    client: Client,
    /// In-memory document store (uri → source text).
    documents: RwLock<HashMap<Url, String>>,
}

impl ManiTLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
        }
    }

    /// Run the compiler frontend and publish diagnostics to the client.
    async fn diagnose(&self, uri: &Url, text: &str) {
        let diagnostics = self.check_source(text);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    /// Run lexer → parser → semantic analysis and collect all errors as
    /// LSP `Diagnostic` objects.
    ///
    /// On a reserved stack. The language server runs on tokio worker threads,
    /// which get the default stack, and the parser's depth guard is only
    /// enforceable on a stack deep enough to reach it — so deeply nested
    /// source in an open editor buffer used to abort the whole server process
    /// rather than produce the diagnostic. The same defect as the one the F-8
    /// corpus harness hit, in the place a user would actually meet it: the
    /// editor is exactly where half-written, deeply-nested code lives.
    fn check_source(&self, source: &str) -> Vec<Diagnostic> {
        let owned = source.to_string();
        crate::with_compiler_stack(move || Self::check_source_inner(&owned))
    }

    fn check_source_inner(source: &str) -> Vec<Diagnostic> {
        let mut diags = Vec::new();

        // Lex
        let mut lexer = Lexer::with_file(source, "<lsp>");
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                diags.push(compile_error_to_diagnostic(&e));
                return diags;
            }
        };

        // Parse
        let mut parser = ManiParser::with_file(tokens, "<lsp>");
        let program = match parser.parse() {
            Ok(p) => p,
            Err(e) => {
                diags.push(compile_error_to_diagnostic(&e));
                return diags;
            }
        };

        // Semantic analysis / type checking
        let mut analyzer = SemanticAnalyzer::with_file("<lsp>");
        match analyzer.analyze(&program) {
            Ok(_) => {}
            Err(e) => {
                diags.push(compile_error_to_diagnostic(&e));
            }
        }

        diags
    }
}

// ---------------------------------------------------------------------------
// LSP position conversion (A6)
//
// `Position.character` is an offset in UTF-16 code units, not bytes. Using it
// directly as a byte index mis-locates the cursor on any line containing a
// non-ASCII character, and slicing at an offset that lands inside a multi-byte
// character panics — Rust rejects non-char-boundary `str` indices even for an
// empty range. Both directions are converted explicitly here.
// ---------------------------------------------------------------------------

/// Characters that make up a ManiT identifier, matching the lexer.
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Convert an LSP `character` offset (UTF-16 code units) to a byte offset
/// within `line`. Always returns a char boundary; clamps past the end.
fn utf16_to_byte(line: &str, character: u32) -> usize {
    let target = character as usize;
    let mut units = 0usize;
    for (byte_idx, ch) in line.char_indices() {
        if units >= target {
            return byte_idx;
        }
        units += ch.len_utf16();
    }
    line.len()
}

/// Convert a byte offset within `line` to an LSP `character` offset.
fn byte_to_utf16(line: &str, byte: usize) -> u32 {
    let mut units = 0usize;
    for (byte_idx, ch) in line.char_indices() {
        if byte_idx >= byte {
            break;
        }
        units += ch.len_utf16();
    }
    units as u32
}

/// Byte range of the identifier surrounding `byte_col`, walking whole
/// characters so the result is always on char boundaries.
fn ident_range_at(line: &str, byte_col: usize) -> (usize, usize) {
    let start = line[..byte_col]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_ident_char(*c))
        .map(|(i, _)| i)
        .last()
        .unwrap_or(byte_col);
    let end = line[byte_col..]
        .char_indices()
        .take_while(|(_, c)| is_ident_char(*c))
        .map(|(i, c)| byte_col + i + c.len_utf8())
        .last()
        .unwrap_or(byte_col);
    (start, end)
}

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

fn compile_error_to_diagnostic(err: &CompileError) -> Diagnostic {
    let d: &ManiDiagnostic = err.diagnostic();

    let line = if d.line > 0 { d.line - 1 } else { 0 } as u32;
    let col = if d.col > 0 { d.col - 1 } else { 0 } as u32;

    Diagnostic {
        range: Range {
            start: Position {
                line,
                character: col,
            },
            end: Position {
                line,
                character: col + 1,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("manitc".to_string()),
        message: d.message.clone(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// LSP trait implementation
// ---------------------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for ManiTLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), ":".into()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "manitc-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ManiT LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());
        self.diagnose(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents
                .write()
                .await
                .insert(uri.clone(), change.text.clone());
            self.diagnose(&uri, &change.text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        if let Some(text) = docs.get(&uri) {
            let text = text.clone();
            drop(docs);
            self.diagnose(&uri, &text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(source) = docs.get(uri) else {
            return Ok(None);
        };

        // Find the word under cursor
        let lines: Vec<&str> = source.lines().collect();
        let line_idx = pos.line as usize;
        if line_idx >= lines.len() {
            return Ok(None);
        }
        let line = lines[line_idx];
        // A6: `character` is a UTF-16 offset — convert before indexing bytes.
        let col = utf16_to_byte(line, pos.character);
        if col >= line.len() {
            return Ok(None);
        }

        // Extract the identifier at the cursor position on char boundaries.
        let (start, end) = ident_range_at(line, col);
        let word = &line[start..end];

        if word.is_empty() {
            return Ok(None);
        }

        // Response ranges are in UTF-16 units too.
        let range_start = byte_to_utf16(line, start);
        let range_end = byte_to_utf16(line, end);

        // Provide hover information for keywords and built-in types
        let info = match word {
            // Types
            "Int" => Some("**Int** — 27-trit balanced ternary signed integer"),
            "Float" => Some("**Float** — 54-trit balanced ternary floating point"),
            "Trit" => Some("**Trit** — balanced ternary digit: +1, 0, or -1"),
            "Bool" => Some("**Bool** — boolean: true or false"),
            "Str" => Some("**Str** — UTF-8 string"),
            "Char" => Some("**Char** — Unicode scalar value"),
            "Void" => Some("**Void** — unit type (no value)"),
            "Word" => Some("**Word** — 27-trit machine word"),
            // Keywords
            "fn" => Some("**fn** — function declaration"),
            "let" => Some("**let** — variable binding"),
            "mut" => Some("**mut** — mutable binding modifier"),
            "if" => Some("**if** — conditional branch"),
            "elif" => Some("**elif** — else-if branch"),
            "else" => Some("**else** — fallback branch"),
            "tif" => Some("**tif** — ternary conditional: tif expr { +1 } tunknown { 0 } telse { -1 }"),
            "tunknown" => Some("**tunknown** — zero branch in ternary conditional (tif)"),
            "telse" => Some("**telse** — negative branch in ternary conditional (tif)"),
            "match" => Some("**match** — pattern matching"),
            "for" => Some("**for** — loop over range or iterator"),
            "while" => Some("**while** — conditional loop"),
            "loop" => Some("**loop** — infinite loop (break to exit)"),
            "return" => Some("**return** — return from function"),
            "break" => Some("**break** — exit loop"),
            "continue" => Some("**continue** — skip to next iteration"),
            "struct" => Some("**struct** — record type definition"),
            "enum" => Some("**enum** — tagged union type definition"),
            "impl" => Some("**impl** — method implementation block"),
            "trait" => Some("**trait** — trait (interface) definition"),
            "use" => Some("**use** — import module or item"),
            "pub" => Some("**pub** — public visibility modifier"),
            "spawn" => Some("**spawn** — launch concurrent ternary task"),
            "async" => Some("**async** — asynchronous function modifier"),
            "await" => Some("**await** — await async result"),
            "true" => Some("**true** — boolean true"),
            "false" => Some("**false** — boolean false"),
            "unknown" => Some("**unknown** — ternary unknown state (trit 0)"),
            // Ternary logic
            "tand" => Some("**tand** — ternary AND (Kleene strong conjunction): min(a, b)"),
            "tor" => Some("**tor** — ternary OR (Kleene strong disjunction): max(a, b)"),
            "tnot" => Some("**tnot** — ternary NOT (negation): -a"),
            // C1: the Lukasiewicz family. These landed in the language before
            // they landed here, so hover was silent on them.
            "timp" => Some("**timp** — Lukasiewicz implication: min(+1, 1 - a + b). \
                            `a timp a` is +1 even for unknown — the deduction \
                            theorem, and what makes this L3 rather than K3"),
            "teq" => Some("**teq** — Lukasiewicz equivalence: (a timp b) tand (b timp a)"),
            "tposs" => Some("**tposs** — possibility (M): +1 if a >= 0, else -1. \
                             Two-valued whatever it is given"),
            "tnec" => Some("**tnec** — necessity (L): +1 only if a = +1, else -1. \
                            Dual to tposs: `tnec a == tnot tposs tnot a`"),
            // C2 / T3ISA v1.5: the lane-wise family — 27 trits at once.
            "tandw" => Some("**tandw** — lane-wise AND: per-trit min across all \
                             27 lanes of a word. One T3 instruction"),
            "torw" => Some("**torw** — lane-wise OR: per-trit max across 27 lanes"),
            "txorw" => Some("**txorw** — lane-wise balanced sum mod 3. Not an \
                             involution: THREE applications recover the original"),
            "timpw" => Some("**timpw** — lane-wise Lukasiewicz implication, per lane"),
            "tcmpw" => Some("**tcmpw** — lane-wise three-way compare: sign(a_i - b_i) \
                             per lane"),
            "tnotw" => Some("**tnotw** — lane-wise NOT: negates all 27 lanes. \
                             Compiles to TNEG — negating a balanced-ternary \
                             number already flips every trit"),
            _ => None,
        };

        if let Some(info_text) = info {
            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info_text.to_string(),
                }),
                range: Some(Range {
                    start: Position {
                        line: pos.line,
                        character: range_start,
                    },
                    end: Position {
                        line: pos.line,
                        character: range_end,
                    },
                }),
            }))
        } else {
            // For non-keyword identifiers, try to get type from semantic analysis
            let source_clone = source.clone();
            let word_owned = word.to_string();
            // Release the read lock before calling get_identifier_type
            std::mem::drop(docs);
            let type_info = self.get_identifier_type(&source_clone, &word_owned, line_idx + 1);
            if let Some(ty) = type_info {
                Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```manit\n{}: {}\n```", word_owned, ty),
                    }),
                    range: Some(Range {
                        start: Position {
                            line: pos.line,
                            character: range_start,
                        },
                        end: Position {
                            line: pos.line,
                            character: range_end,
                        },
                    }),
                }))
            } else {
                Ok(None)
            }
        }
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let keywords = [
            ("fn", CompletionItemKind::KEYWORD, "Function declaration"),
            ("let", CompletionItemKind::KEYWORD, "Variable binding"),
            ("mut", CompletionItemKind::KEYWORD, "Mutable modifier"),
            ("if", CompletionItemKind::KEYWORD, "Conditional branch"),
            ("elif", CompletionItemKind::KEYWORD, "Else-if branch"),
            ("else", CompletionItemKind::KEYWORD, "Fallback branch"),
            ("tif", CompletionItemKind::KEYWORD, "Ternary conditional"),
            ("tunknown", CompletionItemKind::KEYWORD, "Ternary zero branch"),
            ("telse", CompletionItemKind::KEYWORD, "Ternary negative branch"),
            ("for", CompletionItemKind::KEYWORD, "Loop over range"),
            ("while", CompletionItemKind::KEYWORD, "Conditional loop"),
            ("loop", CompletionItemKind::KEYWORD, "Infinite loop"),
            ("match", CompletionItemKind::KEYWORD, "Pattern matching"),
            ("return", CompletionItemKind::KEYWORD, "Return from function"),
            ("break", CompletionItemKind::KEYWORD, "Exit loop"),
            ("continue", CompletionItemKind::KEYWORD, "Next iteration"),
            ("struct", CompletionItemKind::KEYWORD, "Record type"),
            ("enum", CompletionItemKind::KEYWORD, "Tagged union type"),
            ("impl", CompletionItemKind::KEYWORD, "Method block"),
            ("trait", CompletionItemKind::KEYWORD, "Trait definition"),
            ("use", CompletionItemKind::KEYWORD, "Import"),
            ("pub", CompletionItemKind::KEYWORD, "Public visibility"),
            ("spawn", CompletionItemKind::KEYWORD, "Concurrent task"),
            ("async", CompletionItemKind::KEYWORD, "Async modifier"),
            ("await", CompletionItemKind::KEYWORD, "Await result"),
            ("tand", CompletionItemKind::OPERATOR, "Ternary AND: min(a,b)"),
            ("tor", CompletionItemKind::OPERATOR, "Ternary OR: max(a,b)"),
            ("tnot", CompletionItemKind::OPERATOR, "Ternary NOT: -a"),
            // C1: Lukasiewicz family
            ("timp", CompletionItemKind::OPERATOR, "Lukasiewicz implication: min(+1, 1-a+b)"),
            ("teq", CompletionItemKind::OPERATOR, "Lukasiewicz equivalence"),
            ("tposs", CompletionItemKind::OPERATOR, "Possibility (M): +1 if a >= 0"),
            ("tnec", CompletionItemKind::OPERATOR, "Necessity (L): +1 only if a = +1"),
            // C2: lane-wise family — 27 trits at once
            ("tandw", CompletionItemKind::OPERATOR, "Lane-wise AND: per-trit min, 27 lanes"),
            ("torw", CompletionItemKind::OPERATOR, "Lane-wise OR: per-trit max, 27 lanes"),
            ("txorw", CompletionItemKind::OPERATOR, "Lane-wise sum mod 3, 27 lanes"),
            ("timpw", CompletionItemKind::OPERATOR, "Lane-wise implication, 27 lanes"),
            ("tcmpw", CompletionItemKind::OPERATOR, "Lane-wise compare: sign(a_i - b_i)"),
            ("tnotw", CompletionItemKind::OPERATOR, "Lane-wise NOT: negates all 27 lanes"),
            ("true", CompletionItemKind::CONSTANT, "Boolean true"),
            ("false", CompletionItemKind::CONSTANT, "Boolean false"),
            ("unknown", CompletionItemKind::CONSTANT, "Ternary unknown (0)"),
            // Built-in types
            ("Int", CompletionItemKind::TYPE_PARAMETER, "27-trit signed integer"),
            ("Float", CompletionItemKind::TYPE_PARAMETER, "54-trit floating point"),
            ("Trit", CompletionItemKind::TYPE_PARAMETER, "Balanced ternary digit"),
            ("Bool", CompletionItemKind::TYPE_PARAMETER, "Boolean"),
            ("Str", CompletionItemKind::TYPE_PARAMETER, "UTF-8 string"),
            ("Char", CompletionItemKind::TYPE_PARAMETER, "Unicode character"),
            ("Void", CompletionItemKind::TYPE_PARAMETER, "Unit type"),
            ("Word", CompletionItemKind::TYPE_PARAMETER, "27-trit machine word"),
        ];

        let items: Vec<CompletionItem> = keywords
            .iter()
            .map(|(label, kind, detail)| CompletionItem {
                label: label.to_string(),
                kind: Some(*kind),
                detail: Some(detail.to_string()),
                ..Default::default()
            })
            .collect();

        Ok(Some(CompletionResponse::Array(items)))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

impl ManiTLanguageServer {
    /// Try to resolve an identifier's type by running the semantic analyzer.
    /// Returns a string representation of the type, or None.
    fn get_identifier_type(&self, source: &str, _name: &str, _line: usize) -> Option<String> {
        // Reserved stack, for the same reason as `check_source`.
        let owned = source.to_string();
        let name = _name.to_string();
        crate::with_compiler_stack(move || Self::get_identifier_type_inner(&owned, &name))
    }

    fn get_identifier_type_inner(source: &str, _name: &str) -> Option<String> {
        let mut lexer = Lexer::with_file(source, "<lsp>");
        let tokens = lexer.tokenize().ok()?;
        let mut parser = ManiParser::with_file(tokens, "<lsp>");
        let program = parser.parse().ok()?;
        let mut analyzer = SemanticAnalyzer::with_file("<lsp>");
        let typed = analyzer.analyze(&program).ok()?;

        // Search for matching function name
        for func in &typed.functions {
            if func.name == _name {
                return Some(format!("fn(…) -> {}", func.ret_ty.display()));
            }
        }
        // Search for matching struct name
        for s in &typed.structs {
            if s.name == _name {
                let fields: Vec<String> = s
                    .fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.ty.display()))
                    .collect();
                return Some(format!("struct {{ {} }}", fields.join(", ")));
            }
        }
        // Search for matching enum name
        for e in &typed.enums {
            if e.name == _name {
                let variants: Vec<String> =
                    e.variants.iter().map(|v| v.name.clone()).collect();
                return Some(format!("enum {{ {} }}", variants.join(", ")));
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Entry point — stdio transport
// ---------------------------------------------------------------------------

pub async fn run_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| ManiTLanguageServer::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

// ---------------------------------------------------------------------------
// Tests — A6 position conversion
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact shape that used to panic: hovering the `a` after a 3-byte
    /// character. UTF-16 offset 10 was used as byte index 10, which lands
    /// inside '€' (bytes 9..12), and `&line[10..10]` panicked.
    #[test]
    fn a6_hover_after_multibyte_char_does_not_panic() {
        let line = "let s = \"\u{20AC}a\";";
        assert_eq!(line.len(), 15, "byte length");
        // UTF-16 offset of the 'a'
        let character = line
            .encode_utf16()
            .position(|u| u == b'a' as u16)
            .expect("'a' present") as u32;
        assert_eq!(character, 10);

        let col = utf16_to_byte(line, character);
        assert!(line.is_char_boundary(col), "converted offset must be a char boundary");
        let (start, end) = ident_range_at(line, col);
        assert_eq!(&line[start..end], "a", "should find the identifier 'a'");
    }

    #[test]
    fn a6_utf16_to_byte_matches_ascii_offsets() {
        let line = "let value = 1;";
        for (i, _) in line.char_indices() {
            assert_eq!(utf16_to_byte(line, i as u32), i);
        }
    }

    #[test]
    fn a6_round_trip_through_utf16_and_back() {
        for line in ["plain ascii", "caf\u{e9} x", "a \u{20AC} b", "emoji \u{1F600} z"] {
            for (byte_idx, _) in line.char_indices() {
                let units = byte_to_utf16(line, byte_idx);
                assert_eq!(
                    utf16_to_byte(line, units), byte_idx,
                    "round trip failed for {:?} at byte {}", line, byte_idx,
                );
            }
        }
    }

    #[test]
    fn a6_ident_range_finds_whole_identifier() {
        let line = "  let my_var2 = 3;";
        let col = line.find("my_var2").unwrap() + 3; // inside the identifier
        let (s, e) = ident_range_at(line, col);
        assert_eq!(&line[s..e], "my_var2");
    }

    #[test]
    fn a6_ident_range_is_empty_in_open_whitespace() {
        // Not adjacent to any identifier character on either side. (Directly
        // after an identifier the range still covers it, which is the
        // long-standing behaviour and what editors expect.)
        let line = "a  =  b";
        let (s, e) = ident_range_at(line, 2);
        assert_eq!(s, e, "no identifier in open whitespace");
    }

    #[test]
    fn a6_ident_range_covers_identifier_ending_at_cursor() {
        let line = "a = b";
        let (s, e) = ident_range_at(line, 1); // just past 'a'
        assert_eq!(&line[s..e], "a");
    }

    /// Every byte offset in a line with astral-plane characters must stay on a
    /// char boundary, so slicing can never panic wherever the cursor lands.
    #[test]
    fn a6_all_utf16_offsets_land_on_char_boundaries() {
        let line = "x = \"\u{1F600}\u{20AC}\u{e9}ok\";";
        let max_units = line.encode_utf16().count() as u32;
        for character in 0..=max_units + 2 {
            let col = utf16_to_byte(line, character);
            assert!(
                line.is_char_boundary(col),
                "offset {} -> byte {} is not a char boundary", character, col,
            );
            let (s, e) = ident_range_at(line, col);
            let _ = &line[s..e]; // must not panic
        }
    }
}
