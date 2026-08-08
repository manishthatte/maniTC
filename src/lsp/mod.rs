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
    fn check_source(&self, source: &str) -> Vec<Diagnostic> {
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
        let col = pos.character as usize;
        if col >= line.len() {
            return Ok(None);
        }

        // Extract the word at the cursor position
        let bytes = line.as_bytes();
        let mut start = col;
        while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_')
        {
            start -= 1;
        }
        let mut end = col;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        let word = &line[start..end];

        if word.is_empty() {
            return Ok(None);
        }

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
                        character: start as u32,
                    },
                    end: Position {
                        line: pos.line,
                        character: end as u32,
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
                            character: start as u32,
                        },
                        end: Position {
                            line: pos.line,
                            character: end as u32,
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
        let mut lexer = Lexer::with_file(source, "<lsp>");
        let tokens = lexer.tokenize().ok()?;
        let mut parser = ManiParser::with_file(tokens, "<lsp>");
        let program = parser.parse().ok()?;
        let mut analyzer = SemanticAnalyzer::with_file("<lsp>");
        let typed = analyzer.analyze(&program).ok()?;

        // Search for matching function name
        for func in &typed.functions {
            if func.name == _name {
                return Some(format!("fn(…) -> {:?}", func.ret_ty));
            }
        }
        // Search for matching struct name
        for s in &typed.structs {
            if s.name == _name {
                let fields: Vec<String> = s
                    .fields
                    .iter()
                    .map(|f| format!("{}: {:?}", f.name, f.ty))
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
