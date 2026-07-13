//! Renders a GObject-Introspection (.gir) repository to searchable,
//! displayable Markdown-ish text.
//!
//! This is deliberately the *only* place GIR gets turned into text — see
//! the module-level rationale in the previous revision of this file for
//! why (search/show offset alignment). This revision replaces every
//! "BEST GUESS, VERIFY" accessor from the first draft with the confirmed
//! API surface from gir-parser 0.1.6's own source (traits.rs, class.rs,
//! namespace.rs, etc.), so there is nothing left to guess here.
//!
//! A few names differ from the first draft's guesses, worth noting since
//! they're easy to get wrong from memory:
//! - `Namespace::enums()` / `Namespace::flags()`, not `enumerations()` /
//!   `bitfields()`.
//! - `Field::ty()` / `Constant::ty()` / `Alias::ty()`, not `type_ref()`.
//! - There are two different `Callable`s: the *trait* in `traits.rs`
//!   (implemented by `Function`/`Method`/`Callback`/`VirtualMethod`/
//!   `FunctionMacro` via the `impl_callable!` macro) and the `Callable`
//!   *enum* in `callable.rs` (`Constructor(Function) | Method(Method) |
//!   Function(Function)`, used internally by `Class`/`Interface`/
//!   `Record`/`Union` to store their members). The enum only forwards a
//!   handful of fields and has no `.doc()` — but `Class::constructors()`/
//!   `.methods()`/`.functions()` already unwrap it down to the concrete
//!   `Function`/`Method` types, which do implement `Documentable`. Using
//!   those iterator methods (as this file does) sidesteps the enum
//!   entirely.
//! - `Signal` does not implement the `Callable`/`FunctionLike` traits —
//!   it has its own inherent `name()`/`parameters()`/`return_value()`
//!   methods instead. Handled via the same local `CallableLike` bridge
//!   trait as everything else, just with a separate `impl` block.

use gir_parser::prelude::*;
use gir_parser::{AnyType, ClassField, InterfaceField, UnionField};
use gir_parser::{
    Array, BitField, Callback, Class, Enumeration, FieldType, Function, Interface,
    Member, Method, Parameter, ParameterType, Parameters, Property, Record, RecordField,
    Repository, Signal, Type, Union, VirtualMethod,
};
use std::io;

/// Parse GIR XML from bytes (already decompressed by lore-core if needed)
/// and render it to text, including doc comments and full signatures.
pub fn render_gir(data: &[u8]) -> io::Result<Vec<u8>> {
    let text = String::from_utf8_lossy(data);
    let repo: Repository = text.parse().map_err(|e: gir_parser::ParserError| {
        io::Error::new(io::ErrorKind::InvalidData, format!("{e}"))
    })?;
    Ok(render_repository(&repo).into_bytes())
}

// ============================================================================
// Type formatting
// ============================================================================

fn format_type(t: &Type) -> String {
    let base = t.name().or_else(|| t.c_type()).unwrap_or("?");
    if t.types().is_empty() {
        base.to_string()
    } else {
        let args: Vec<String> = t.types().iter().map(format_type).collect();
        format!("{}<{}>", base, args.join(", "))
    }
}

fn format_array(a: &Array) -> String {
    format!("{}[]", format_type(a.ty()))
}

fn format_any_type(t: &AnyType) -> String {
    match t {
        AnyType::Type(ty) => format_type(ty),
        AnyType::Array(arr) => format_array(arr),
    }
}

fn format_field_type(t: &FieldType) -> String {
    match t {
        FieldType::Type(ty) => format_type(ty),
        FieldType::Callback(cb) => format!("callback {}", cb.name()),
        FieldType::Array(arr) => format_array(arr),
    }
}

fn format_parameter_type(t: &ParameterType) -> String {
    match t {
        ParameterType::Type(ty) => format_type(ty),
        ParameterType::Array(arr) => format_array(arr),
        ParameterType::VarArgs => "...".to_string(),
    }
}

fn format_parameters(params: &Parameters) -> String {
    let mut parts = vec![];
    if let Some(inst) = params.instance() {
        let ty = inst
            .ty()
            .map(format_type)
            .unwrap_or_else(|| "Self".to_string());
        parts.push(format!("{}: {}", inst.name(), ty));
    }
    for p in params.inner() {
        parts.push(format_single_parameter(p));
    }
    parts.join(", ")
}

fn format_single_parameter(p: &Parameter) -> String {
    let ty = p
        .ty()
        .map(format_parameter_type)
        .unwrap_or_else(|| "?".to_string());
    format!("{}: {}", p.name(), ty)
}

// ============================================================================
// Local bridge trait: unifies Function/Method/Callback/VirtualMethod
// (which implement the real Callable+FunctionLike traits) and Signal
// (which doesn't, but exposes equivalent inherent methods) under one
// interface so `push_callable` only needs to be written once.
// ============================================================================

trait CallableLike {
    fn cl_name(&self) -> &str;
    fn cl_doc(&self) -> Option<&str>;
    fn cl_params(&self) -> String;
    fn cl_return(&self) -> String;
}

macro_rules! impl_callable_like_via_traits {
    ($ty:ty) => {
        impl CallableLike for $ty {
            fn cl_name(&self) -> &str {
                self.name()
            }
            fn cl_doc(&self) -> Option<&str> {
                self.doc().map(|d| d.text())
            }
            fn cl_params(&self) -> String {
                format_parameters(self.parameters())
            }
            fn cl_return(&self) -> String {
                format_any_type(self.return_value().ty())
            }
        }
    };
}

impl_callable_like_via_traits!(Function);
impl_callable_like_via_traits!(Method);
impl_callable_like_via_traits!(Callback);
impl_callable_like_via_traits!(VirtualMethod);

impl CallableLike for Signal {
    fn cl_name(&self) -> &str {
        self.name()
    }
    fn cl_doc(&self) -> Option<&str> {
        self.doc().map(|d| d.text())
    }
    fn cl_params(&self) -> String {
        format_parameters(self.parameters())
    }
    fn cl_return(&self) -> String {
        format_any_type(self.return_value().ty())
    }
}

fn push_callable(out: &mut String, kind: &str, item: &impl CallableLike) {
    out.push_str(&format!(
        "- **{}** `{}({})` -> `{}`\n",
        kind,
        item.cl_name(),
        item.cl_params(),
        item.cl_return()
    ));
    if let Some(d) = item.cl_doc() {
        let d = d.trim();
        if !d.is_empty() {
            out.push_str(&format!("  {}\n", d.replace('\n', "\n  ")));
        }
    }
}

fn push_doc(out: &mut String, doc: Option<&str>) {
    if let Some(d) = doc {
        let d = d.trim();
        if !d.is_empty() {
            out.push_str(d);
            out.push_str("\n\n");
        }
    }
}

// ============================================================================
// Fields (four structurally-identical enums: ClassField, InterfaceField,
// RecordField, UnionField, each Field | Union | Record | Callback)
// ============================================================================

fn push_field(out: &mut String, name: &str, ty: &FieldType) {
    out.push_str(&format!("- `{}`: `{}`\n", name, format_field_type(ty)));
}

fn push_class_field(out: &mut String, f: &ClassField) {
    match f {
        ClassField::Field(field) => push_field(out, field.name(), field.ty()),
        ClassField::Union(u) => out.push_str(&format!(
            "- (anonymous union{})\n",
            u.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        ClassField::Record(r) => out.push_str(&format!(
            "- (anonymous record{})\n",
            r.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        ClassField::Callback(cb) => push_callable(out, "Callback field", cb),
    }
}

fn push_interface_field(out: &mut String, f: &InterfaceField) {
    match f {
        InterfaceField::Field(field) => push_field(out, field.name(), field.ty()),
        InterfaceField::Union(u) => out.push_str(&format!(
            "- (anonymous union{})\n",
            u.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        InterfaceField::Record(r) => out.push_str(&format!(
            "- (anonymous record{})\n",
            r.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        InterfaceField::Callback(cb) => push_callable(out, "Callback field", cb),
    }
}

fn push_record_field(out: &mut String, f: &RecordField) {
    match f {
        RecordField::Field(field) => push_field(out, field.name(), field.ty()),
        RecordField::Union(u) => out.push_str(&format!(
            "- (anonymous union{})\n",
            u.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        RecordField::Record(r) => out.push_str(&format!(
            "- (anonymous record{})\n",
            r.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        RecordField::Callback(cb) => push_callable(out, "Callback field", cb),
    }
}

fn push_union_field(out: &mut String, f: &UnionField) {
    match f {
        UnionField::Field(field) => push_field(out, field.name(), field.ty()),
        UnionField::Union(u) => out.push_str(&format!(
            "- (anonymous union{})\n",
            u.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        UnionField::Record(r) => out.push_str(&format!(
            "- (anonymous record{})\n",
            r.name().map(|n| format!(" {n}")).unwrap_or_default()
        )),
        UnionField::Callback(cb) => push_callable(out, "Callback field", cb),
    }
}

// ============================================================================
// Members (Enumeration / BitField)
// ============================================================================

fn push_member(out: &mut String, m: &Member) {
    out.push_str(&format!("- `{}` = {}\n", m.name(), m.value()));
    if let Some(d) = m.doc().map(|d| d.text()) {
        let d = d.trim();
        if !d.is_empty() {
            out.push_str(&format!("  {}\n", d.replace('\n', "\n  ")));
        }
    }
}

// ============================================================================
// Top-level renderer
// ============================================================================

fn render_repository(repo: &Repository) -> String {
    let mut out = String::new();

    out.push_str("# GObject Introspection Repository\n\n");
    if let Some(version) = repo.version() {
        out.push_str(&format!("**Version:** {}\n\n", version));
    }

    let includes = repo.header_includes();
    if !includes.is_empty() {
        out.push_str("## Includes\n\n");
        for include in includes {
            out.push_str(&format!("- {}\n", include.name()));
        }
        out.push('\n');
    }

    let packages = repo.packages();
    if !packages.is_empty() {
        out.push_str("## Packages\n\n");
        for package in packages {
            out.push_str(&format!("- {}\n", package.name()));
        }
        out.push('\n');
    }

    let ns = repo.namespace();
    out.push_str(&format!(
        "## Namespace: {} ({})\n\n",
        ns.name(),
        ns.version()
    ));

    render_classes(&mut out, ns.classes());
    render_interfaces(&mut out, ns.interfaces());
    render_records(&mut out, ns.records());
    render_unions(&mut out, ns.unions());

    let functions = ns.functions();
    if !functions.is_empty() {
        out.push_str(&format!("## Functions ({})\n\n", functions.len()));
        for f in functions {
            push_callable(&mut out, "Function", f);
        }
        out.push('\n');
    }

    let macros = ns.macros();
    if !macros.is_empty() {
        out.push_str(&format!("## Function-like macros ({})\n\n", macros.len()));
        for m in macros {
            // FunctionMacro implements Callable (name) and has an inherent
            // `parameters()`, but no `return_value()` — macros aren't
            // typed the way functions are, so this is a lighter listing
            // than push_callable's.
            let mut parts = vec![];
            if let Some(inst) = m.parameters().instance() {
                parts.push(inst.name().to_string());
            }
            for p in m.parameters().inner() {
                parts.push(p.name().to_string());
            }
            out.push_str(&format!(
                "- **Macro** `{}({})`\n",
                m.name(),
                parts.join(", ")
            ));
            if let Some(d) = m.doc().map(|d| d.text()) {
                let d = d.trim();
                if !d.is_empty() {
                    out.push_str(&format!("  {}\n", d.replace('\n', "\n  ")));
                }
            }
        }
        out.push('\n');
    }

    render_enums(&mut out, ns.enums());
    render_bitfields(&mut out, ns.flags());

    let callbacks = ns.callbacks();
    if !callbacks.is_empty() {
        out.push_str(&format!("## Callbacks ({})\n\n", callbacks.len()));
        for cb in callbacks {
            push_callable(&mut out, "Callback", cb);
        }
        out.push('\n');
    }

    let boxed = ns.boxed();
    if !boxed.is_empty() {
        out.push_str(&format!("## Boxed types ({})\n\n", boxed.len()));
        for b in boxed {
            out.push_str(&format!("### {}\n\n", b.g_name()));
            push_doc(&mut out, b.doc().map(|d| d.text()));
            for f in b.functions() {
                push_callable(&mut out, "Function", f);
            }
            out.push('\n');
        }
    }

    let constants = ns.constants();
    if !constants.is_empty() {
        out.push_str(&format!("## Constants ({})\n\n", constants.len()));
        for c in constants {
            out.push_str(&format!(
                "- **{}**: `{}` = {}\n",
                c.name(),
                format_any_type(c.ty()),
                c.value()
            ));
        }
        out.push('\n');
    }

    let aliases = ns.aliases();
    if !aliases.is_empty() {
        out.push_str(&format!("## Aliases ({})\n\n", aliases.len()));
        for a in aliases {
            out.push_str(&format!(
                "- **{}** -> `{}`\n",
                a.name(),
                format_any_type(a.ty())
            ));
        }
    }

    out
}

fn render_classes(out: &mut String, classes: &[Class]) {
    if classes.is_empty() {
        return;
    }
    out.push_str(&format!("## Classes ({})\n\n", classes.len()));
    for class in classes {
        out.push_str(&format!("### {}", class.name()));
        if let Some(parent) = class.parent() {
            out.push_str(&format!(" (extends {})", parent));
        }
        let impls: Vec<&str> = class.implements().iter().map(|i| i.name()).collect();
        if !impls.is_empty() {
            out.push_str(&format!(" implements {}", impls.join(", ")));
        }
        out.push_str("\n\n");
        push_doc(out, class.doc().map(|d| d.text()));

        for f in class.fields() {
            push_class_field(out, f);
        }
        for ctor in class.constructors() {
            push_callable(out, "Constructor", ctor);
        }
        for method in class.methods() {
            push_callable(out, "Method", method);
        }
        for vm in class.virtual_methods() {
            push_callable(out, "Virtual method", vm);
        }
        for prop in class.properties() {
            push_property(out, prop);
        }
        for signal in class.signals() {
            push_callable(out, "Signal", signal);
        }
        for f in class.functions() {
            push_callable(out, "Function", f);
        }
        for c in class.constants() {
            out.push_str(&format!(
                "- **constant** `{}`: `{}` = {}\n",
                c.name(),
                format_any_type(c.ty()),
                c.value()
            ));
        }
        out.push('\n');
    }
}

fn render_interfaces(out: &mut String, interfaces: &[Interface]) {
    if interfaces.is_empty() {
        return;
    }
    out.push_str(&format!("## Interfaces ({})\n\n", interfaces.len()));
    for interface in interfaces {
        out.push_str(&format!("### {}\n\n", interface.name()));
        push_doc(out, interface.doc().map(|d| d.text()));

        for f in interface.fields() {
            push_interface_field(out, f);
        }
        for ctor in interface.constructors() {
            push_callable(out, "Constructor", ctor);
        }
        for method in interface.methods() {
            push_callable(out, "Method", method);
        }
        for vm in interface.virtual_methods() {
            push_callable(out, "Virtual method", vm);
        }
        for prop in interface.properties() {
            push_property(out, prop);
        }
        for signal in interface.signals() {
            push_callable(out, "Signal", signal);
        }
        for f in interface.functions() {
            push_callable(out, "Function", f);
        }
        for c in interface.constants() {
            out.push_str(&format!(
                "- **constant** `{}`: `{}` = {}\n",
                c.name(),
                format_any_type(c.ty()),
                c.value()
            ));
        }
        out.push('\n');
    }
}

fn render_records(out: &mut String, records: &[Record]) {
    if records.is_empty() {
        return;
    }
    out.push_str(&format!("## Records ({})\n\n", records.len()));
    for record in records {
        let Some(name) = record.name() else { continue };
        out.push_str(&format!("### {}\n\n", name));
        push_doc(out, record.doc().map(|d| d.text()));

        for f in record.fields() {
            push_record_field(out, f);
        }
        for ctor in record.constructors() {
            push_callable(out, "Constructor", ctor);
        }
        for method in record.methods() {
            push_callable(out, "Method", method);
        }
        for f in record.functions() {
            push_callable(out, "Function", f);
        }
        out.push('\n');
    }
}

fn render_unions(out: &mut String, unions: &[Union]) {
    if unions.is_empty() {
        return;
    }
    out.push_str(&format!("## Unions ({})\n\n", unions.len()));
    for u in unions {
        let Some(name) = u.name() else { continue };
        out.push_str(&format!("### {}\n\n", name));
        push_doc(out, u.doc().map(|d| d.text()));

        for f in u.fields() {
            push_union_field(out, f);
        }
        for ctor in u.constructors() {
            push_callable(out, "Constructor", ctor);
        }
        for method in u.methods() {
            push_callable(out, "Method", method);
        }
        for f in u.functions() {
            push_callable(out, "Function", f);
        }
        out.push('\n');
    }
}

fn render_enums(out: &mut String, enums: &[Enumeration]) {
    if enums.is_empty() {
        return;
    }
    out.push_str(&format!("## Enumerations ({})\n\n", enums.len()));
    for e in enums {
        out.push_str(&format!("### enum {}\n\n", e.name()));
        push_doc(out, e.doc().map(|d| d.text()));
        for m in e.members() {
            push_member(out, m);
        }
        out.push('\n');
    }
}

fn render_bitfields(out: &mut String, flags: &[BitField]) {
    if flags.is_empty() {
        return;
    }
    out.push_str(&format!("## Bitfields ({})\n\n", flags.len()));
    for b in flags {
        out.push_str(&format!("### bitfield {}\n\n", b.name()));
        push_doc(out, b.doc().map(|d| d.text()));
        for m in b.members() {
            push_member(out, m);
        }
        out.push('\n');
    }
}

fn push_property(out: &mut String, prop: &Property) {
    let access = match (prop.is_readable(), prop.is_writable()) {
        (true, true) => "rw",
        (true, false) => "ro",
        (false, true) => "wo",
        (false, false) => "--",
    };
    out.push_str(&format!(
        "- **property** `{}`: `{}` ({})\n",
        prop.name(),
        format_any_type(prop.ty()),
        access
    ));
    if let Some(d) = prop.doc().map(|d| d.text()) {
        let d = d.trim();
        if !d.is_empty() {
            out.push_str(&format!("  {}\n", d.replace('\n', "\n  ")));
        }
    }
}
