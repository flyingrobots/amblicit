//! Lexical linking of authenticated source compilation units.
//! Original sources stay immutable; a separate surface carries hygienic names
//! and mapped spans. Linking grants no runtime authority.
use crate::ast::{Decl, ImportKind, Module};
use crate::compiler::SourceTypeOrigin;
use crate::core_ir::{classify_core_type_reference, CoreTypeReference};
use crate::source_closure::SourceClosure;
use crate::token::{lex, Span, TokenKind};
use crate::{
    compile_to_core, CompilerContext, CompilerError, CompilerErrorKind, CompilerStage, CoreModule,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
mod rewrite;
use rewrite::Rewrite;

/// Stable source-linker refusal categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceLinkErrorKind {
    UnresolvedName,
    UnknownExport,
    ImportedIntent,
    NameCollision,
    SpanOverflow,
    DepthExceeded,
    ReservedTypeIdentity,
}
/// A source-local linking failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLinkError {
    /// Stable rejection category.
    pub kind: SourceLinkErrorKind,
    /// Original source package.
    pub coordinate: String,
    /// Original source byte span.
    pub span: Span,
    /// Source name involved in the refusal.
    pub name: String,
}
impl fmt::Display for SourceLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}: {} at {}..{}: {}",
            self.kind, self.coordinate, self.span.start, self.span.end, self.name
        )
    }
}
impl std::error::Error for SourceLinkError {}
/// A compiler error mapped back to its original compilation unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCompilerError {
    /// Original package, empty only when the compiler supplied no mapped span.
    pub coordinate: String,
    /// Failure with its original source-local byte span.
    pub error: CompilerError,
}
#[derive(Debug)]
struct SourceRange {
    coordinate: String,
    start: usize,
    end: usize,
}
/// A compiler-owned linked surface and its authenticated source ownership.
#[derive(Debug)]
pub struct LinkedSourceClosure<'a> {
    closure: &'a SourceClosure,
    module: Module,
    ranges: Vec<SourceRange>,
    type_origins: BTreeMap<String, SourceTypeOrigin>,
}
impl LinkedSourceClosure<'_> {
    /// Borrow the immutable compiler-owned surface for authority preparation.
    #[must_use]
    pub const fn module(&self) -> &Module {
        &self.module
    }
    /// Borrow the exact authentication witness underlying this link.
    #[must_use]
    pub const fn source_closure(&self) -> &SourceClosure {
        self.closure
    }
    /// Compile using explicit profile, lawpack and budget context facts.
    ///
    /// # Errors
    /// Returns compiler failures mapped to original source packages and spans.
    pub fn compile(
        &self,
        context: &CompilerContext,
    ) -> Result<CoreModule, Vec<SourceCompilerError>> {
        let mut collisions = Vec::new();
        for (coordinate, unit) in self.closure.modules() {
            for declaration in &unit.parsed().decls {
                let declared_type = match declaration {
                    Decl::Type(definition)
                        if !matches!(definition.body, crate::ast::TypeExpr::Ref(_)) =>
                    {
                        Some((&definition.name, definition.span))
                    }
                    Decl::Enum(definition) => Some((&definition.name, definition.span)),
                    _ => None,
                };
                if let Some((name, span)) = declared_type {
                    let original = format!("{coordinate}.{name}");
                    if context.has_imported_type_coordinate(&original) {
                        collisions.push(SourceCompilerError {
                            coordinate: coordinate.clone(),
                            error: CompilerError {
                                stage: CompilerStage::Resolve,
                                kind: CompilerErrorKind::InvalidCoreTypeIntegrity,
                                message: "source type redeclares imported authority".into(),
                                span,
                                signature_mismatch: None,
                            },
                        });
                    }
                }
                if let Decl::Function(function) = declaration {
                    if context.has_imported_callable_coordinate(&format!(
                        "{coordinate}.{}",
                        function.name
                    )) {
                        collisions.push(SourceCompilerError {
                            coordinate: coordinate.clone(),
                            error: CompilerError {
                                stage: CompilerStage::Resolve,
                                kind: CompilerErrorKind::UnresolvedFunction,
                                message: "source function collides with imported authority".into(),
                                span: function.span,
                                signature_mismatch: None,
                            },
                        });
                    }
                }
            }
        }
        if !collisions.is_empty() {
            return Err(collisions);
        }
        let context = context
            .clone()
            .with_linked_source_types(self.type_origins.clone());
        compile_to_core(&self.module, &context).map_err(|failures| {
            failures
                .into_iter()
                .map(|mut error| {
                    let coordinate = self
                        .ranges
                        .iter()
                        .find(|range| {
                            error.span.start >= range.start && error.span.end <= range.end
                        })
                        .map_or_else(String::new, |range| {
                            error.span.start -= range.start;
                            error.span.end -= range.start;
                            range.coordinate.clone()
                        });
                    SourceCompilerError { coordinate, error }
                })
                .collect()
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DefinitionKind {
    Type,
    Function,
    Intent,
}
#[derive(Debug)]
struct Definition {
    kind: DefinitionKind,
    linked: String,
}
#[derive(Debug)]
enum ImportBinding {
    Source(String),
    External(String, ImportKind),
}
#[derive(Debug)]
struct UnitNames {
    coordinate: String,
    prefix: String,
    definitions: BTreeMap<String, Definition>,
    imports: BTreeMap<String, ImportBinding>,
}

/// Link aliases and lexical names within an authenticated source closure.
///
/// Root intent names remain public. Other declarations, imports and local
/// bindings receive fresh deterministic names. Imported intent entries refuse.
///
/// # Errors
/// Returns a source-local refusal for unknown names/exports, imported entries,
/// collisions, or structural depth/span boundaries.
pub fn link_source_closure(
    closure: &SourceClosure,
) -> Result<LinkedSourceClosure<'_>, SourceLinkError> {
    let fresh = fresh_namespace(closure)?;
    let units = collect_names(closure, &fresh)?;
    let type_origins = collect_type_origins(closure, &units);
    let root_name = closure.root_coordinate();
    let root = &closure.modules()[root_name];
    let mut linked = Module {
        package: root.parsed().package.clone(),
        imports: Vec::new(),
        decls: Vec::new(),
    };
    let mut ranges = Vec::new();
    let mut offset = 0_usize;
    let order = std::iter::once(root_name).chain(
        closure
            .modules()
            .keys()
            .map(String::as_str)
            .filter(|name| *name != root_name),
    );
    for name in order {
        let unit = &closure.modules()[name];
        let names = &units[name];
        let mut rewrite = Rewrite::new(names, &units, offset);
        for import in &unit.parsed().imports {
            if import.kind == ImportKind::Module {
                continue;
            }
            let mut import = import.clone();
            if let Some(ImportBinding::External(alias, _)) = names.imports.get(&import.alias) {
                import.alias.clone_from(alias);
            }
            rewrite.shift(&mut import.span)?;
            if let Some(package) = &mut import.package {
                rewrite.shift(&mut package.span)?;
            }
            linked.imports.push(import);
        }
        for (index, declaration) in unit.parsed().decls.iter().enumerate() {
            let mut declaration = declaration.clone();
            rewrite.declaration(&mut declaration, index)?;
            linked.decls.push(declaration);
        }
        let end = offset.checked_add(unit.source().len()).ok_or_else(|| {
            failure(
                SourceLinkErrorKind::SpanOverflow,
                name,
                Span::new(0, 0),
                "source range",
            )
        })?;
        ranges.push(SourceRange {
            coordinate: name.to_owned(),
            start: offset,
            end,
        });
        offset = end.checked_add(1).ok_or_else(|| {
            failure(
                SourceLinkErrorKind::SpanOverflow,
                name,
                Span::new(0, 0),
                "source range",
            )
        })?;
    }
    Ok(LinkedSourceClosure {
        closure,
        module: linked,
        ranges,
        type_origins,
    })
}
fn collect_type_origins(
    closure: &SourceClosure,
    units: &BTreeMap<String, UnitNames>,
) -> BTreeMap<String, SourceTypeOrigin> {
    let mut origins = BTreeMap::new();
    for (coordinate, unit) in closure.modules() {
        for declaration in &unit.parsed().decls {
            let name = match declaration {
                Decl::Type(definition)
                    if !matches!(definition.body, crate::ast::TypeExpr::Ref(_)) =>
                {
                    &definition.name
                }
                Decl::Enum(definition) => &definition.name,
                _ => continue,
            };
            let original = format!("{coordinate}.{name}");
            origins.insert(
                units[coordinate].definitions[name].linked.clone(),
                SourceTypeOrigin {
                    coordinate: original.clone(),
                    table_key: if coordinate == closure.root_coordinate() {
                        name.clone()
                    } else {
                        original
                    },
                },
            );
        }
    }
    origins
}

fn fresh_namespace(closure: &SourceClosure) -> Result<String, SourceLinkError> {
    // One scan records counters already present in source identifiers. N source
    // identifiers can occupy at most N counters, requiring at most N+1 probes.
    let mut occupied = BTreeSet::new();
    for (coordinate, module) in closure.modules() {
        let tokens = lex(module.source()).map_err(|error| {
            failure(
                SourceLinkErrorKind::UnresolvedName,
                coordinate,
                error.span,
                "source tokens",
            )
        })?;
        for token in tokens {
            let TokenKind::Ident(name) = token.kind else {
                continue;
            };
            let Some(tail) = name.strip_prefix("EdictLink_") else {
                continue;
            };
            let Some((digits, _)) = tail.split_once('_') else {
                continue;
            };
            if let Ok(number) = digits.parse::<usize>() {
                if number.to_string() == digits {
                    occupied.insert(number);
                }
            }
        }
    }
    let mut counter = 0;
    while occupied.contains(&counter) {
        counter += 1;
    }
    Ok(format!("EdictLink_{counter}_"))
}
fn collect_names(
    closure: &SourceClosure,
    fresh: &str,
) -> Result<BTreeMap<String, UnitNames>, SourceLinkError> {
    let mut units = BTreeMap::new();
    let mut generated = BTreeSet::new();
    for (coordinate, module) in closure.modules() {
        let prefix = format!("{fresh}{:x}_", Sha256::digest(coordinate.as_bytes()));
        let mut names = UnitNames {
            coordinate: coordinate.clone(),
            prefix,
            definitions: BTreeMap::new(),
            imports: BTreeMap::new(),
        };
        for declaration in &module.parsed().decls {
            let (name, kind, span) = match declaration {
                Decl::Type(definition) => (&definition.name, DefinitionKind::Type, definition.span),
                Decl::Enum(definition) => (&definition.name, DefinitionKind::Type, definition.span),
                Decl::Function(definition) => {
                    (&definition.name, DefinitionKind::Function, definition.span)
                }
                Decl::Intent(definition) => {
                    if coordinate != closure.root_coordinate() {
                        return Err(failure(
                            SourceLinkErrorKind::ImportedIntent,
                            coordinate,
                            definition.span,
                            &definition.name,
                        ));
                    }
                    (&definition.name, DefinitionKind::Intent, definition.span)
                }
            };
            let linked = if kind == DefinitionKind::Intent {
                name.clone()
            } else {
                format!("{}G_{name}", names.prefix)
            };
            if kind == DefinitionKind::Type
                && !matches!(
                    classify_core_type_reference(name),
                    Some(CoreTypeReference::Named)
                )
            {
                return Err(failure(
                    SourceLinkErrorKind::ReservedTypeIdentity,
                    coordinate,
                    span,
                    name,
                ));
            }
            if !generated.insert(linked.clone()) {
                return Err(failure(
                    SourceLinkErrorKind::NameCollision,
                    coordinate,
                    span,
                    name,
                ));
            }
            names
                .definitions
                .insert(name.clone(), Definition { kind, linked });
        }
        for import in &module.parsed().imports {
            let binding = if import.kind == ImportKind::Module {
                let package = import.package.as_ref().ok_or_else(|| {
                    failure(
                        SourceLinkErrorKind::UnresolvedName,
                        coordinate,
                        import.span,
                        &import.alias,
                    )
                })?;
                ImportBinding::Source(format!("{}@{}", package.path.join("."), package.version))
            } else {
                let alias = if coordinate == closure.root_coordinate() {
                    import.alias.clone()
                } else {
                    format!("{}I_{}", names.prefix, import.alias)
                };
                if !generated.insert(alias.clone()) {
                    return Err(failure(
                        SourceLinkErrorKind::NameCollision,
                        coordinate,
                        import.span,
                        &import.alias,
                    ));
                }
                ImportBinding::External(alias, import.kind)
            };
            names.imports.insert(import.alias.clone(), binding);
        }
        units.insert(coordinate.clone(), names);
    }
    Ok(units)
}
fn failure(kind: SourceLinkErrorKind, coordinate: &str, span: Span, name: &str) -> SourceLinkError {
    SourceLinkError {
        kind,
        coordinate: coordinate.to_owned(),
        span,
        name: name.to_owned(),
    }
}
