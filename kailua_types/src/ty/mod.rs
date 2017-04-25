//! Individual types.

use std::fmt;
use std::result;
use diag::{TypeReport, TypeResult};
use kailua_env::Spanned;
use kailua_diag::{Result, Locale, Report};
use kailua_syntax::Name;

pub use self::display::{Display, Displayed, DisplayState, DisplayName};
pub use self::literals::{Numbers, Strings};
pub use self::tables::{Key, Tables};
pub use self::functions::{Function, Functions};
pub use self::union::Unioned;
pub use self::value::{Dyn, Nil, T, Ty};
pub use self::slot::{F, S, Slot};
pub use self::seq::{SeqIter, TySeq, SpannedTySeq, SlotSeq, SpannedSlotSeq};
pub use self::tag::Tag;

mod display;
mod literals;
mod tables;
mod functions;
mod union;
mod value;
mod slot;
mod seq;
mod tag;

/// Anonymous, unifiable type variables generated by `TypeContext`.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct TVar(pub u32);

/// In the debugging output the type variable is denoted <code>&lt;#<i>tvar</i>&gt;</code>.
impl fmt::Debug for TVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<#{}>", self.0)
    }
}

/// Row variables generated by `TypeContext`.
///
/// A row variable #0 (`RVar::empty()`) denotes a special, inextensible "empty" row variable.
/// This cannot occur from unification, but is required to handle subtyping between
/// records and non-records, which effectively disables any further record extension.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RVar(u32);

impl RVar {
    pub fn new(id: usize) -> RVar { RVar(id as u32) }
    pub fn empty() -> RVar { RVar::new(0) }
    pub fn any() -> RVar { RVar::new(0xffffffff) }

    fn to_u32(&self) -> u32 {
        self.0
    }

    pub fn to_usize(&self) -> usize {
        self.to_u32() as usize
    }
}

/// In the debugging output the row variable is denoted <code>&lt;row #<i>rvar</i>&gt;</code>,
/// or <code>...<i>rvar</i></code> in the record types.
impl fmt::Debug for RVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if *self == RVar::empty() {
            write!(f, "<empty row>")
        } else if *self == RVar::any() {
            write!(f, "<row #?>")
        } else {
            write!(f, "<row #{}>", self.to_u32())
        }
    }
}

/// Identifiers for nominal types (currently only used for instantiable classes).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(pub u32);

/// In the debugging output the nominal identifier is denoted <code>&lt;%<i>cid</i>&gt;</code>.
impl fmt::Debug for ClassId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<%{}>", self.0)
    }
}

/// Nominal types.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// A class prototype.
    Prototype(ClassId),

    /// A class instance.
    Instance(ClassId),
}

impl Display for Class {
    fn fmt_displayed(&self, f: &mut fmt::Formatter, st: &DisplayState) -> fmt::Result {
        match *self {
            Class::Prototype(cid) => match (&st.locale[..], st.context.get_class_name(cid)) {
                ("ko", Some(name)) => write!(f, "<{:+} 프로토타입>", name),
                (_,    Some(name)) => write!(f, "<prototype for {:+}>", name),
                ("ko", None) => write!(f, "<이름 없는 클래스 #{}의 프로토타입>", cid.0),
                (_,    None) => write!(f, "<prototype for unnamed class #{}>", cid.0),
            },

            Class::Instance(cid) => match (&st.locale[..], st.context.get_class_name(cid)) {
                (_,    Some(name)) => write!(f, "{:+}", name),
                ("ko", None) => write!(f, "<이름 없는 클래스 #{}>", cid.0),
                (_,    None) => write!(f, "<unnamed class #{}>", cid.0),
            },
        }
    }
}

impl fmt::Debug for Class {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Class::Prototype(cid) => write!(f, "<%{} prototype>", cid.0),
            Class::Instance(cid) => write!(f, "<%{}>", cid.0),
        }
    }
}

/// A superset of the type context that also provides type name resolution.
///
/// This is required for converting a syntax-level type ("kind") to the actual type.
pub trait TypeResolver: Report {
    /// Returns an immutable reference to associated type context.
    fn context(&self) -> &TypeContext;

    /// Returns a mutable reference to associated type context.
    fn context_mut(&mut self) -> &mut TypeContext;

    /// Resolves a type name to a type if any. The span is used for error reporting.
    fn ty_from_name(&self, name: &Spanned<Name>) -> Result<Ty>;
}

impl<'a, R: TypeResolver + ?Sized> TypeResolver for &'a mut R {
    fn context(&self) -> &TypeContext {
        (**self).context()
    }
    fn context_mut(&mut self) -> &mut TypeContext {
        (**self).context_mut()
    }
    fn ty_from_name(&self, name: &Spanned<Name>) -> Result<Ty> {
        (**self).ty_from_name(name)
    }
}

/// A trait that provides every type-related operations.
///
/// This interface is used to decouple the dependency between types and the type environment.
/// In practice, the full implementation is almost always provided by `kailua_types::env::Types`.
pub trait TypeContext {
    /// Generates a new, empty type report.
    fn gen_report(&self) -> TypeReport;

    /// Returns the latest type variable generated, if any.
    fn last_tvar(&self) -> Option<TVar>;

    /// Generates a new fresh type variable.
    fn gen_tvar(&mut self) -> TVar;

    /// Copies a type variable so that a new variable has the same constraints to the original
    /// but is no longer connected to the original.
    ///
    /// Mainly used for generalization.
    fn copy_tvar(&mut self, tvar: TVar) -> TVar;

    /// Asserts that the type variable has given upper bound.
    fn assert_tvar_sub(&mut self, lhs: TVar, rhs: &Ty) -> TypeResult<()>;

    /// Asserts that the type variable has given lower bound.
    fn assert_tvar_sup(&mut self, lhs: TVar, rhs: &Ty) -> TypeResult<()>;

    /// Asserts that the type variable has given tight bound.
    fn assert_tvar_eq(&mut self, lhs: TVar, rhs: &Ty) -> TypeResult<()>;

    /// Asserts that the first type variable is a subtype of the second.
    fn assert_tvar_sub_tvar(&mut self, lhs: TVar, rhs: TVar) -> TypeResult<()>;

    /// Asserts that the first type variable is equal to the second.
    fn assert_tvar_eq_tvar(&mut self, lhs: TVar, rhs: TVar) -> TypeResult<()>;

    /// Returns lower and upper bounds of given type variable as type flags.
    fn get_tvar_bounds(&self, tvar: TVar) -> (flags::Flags /*lb*/, flags::Flags /*ub*/);

    /// Resolves a given type variable if there is a tight bound.
    fn get_tvar_exact_type(&self, tvar: TVar) -> Option<Ty>;

    /// Generates a new fresh row variable.
    fn gen_rvar(&mut self) -> RVar;

    /// Copies a row variable so that a new variable has the same fields to the original
    /// but is no longer connected to the original.
    ///
    /// Mainly used for generalization.
    fn copy_rvar(&mut self, rvar: RVar) -> RVar;

    /// Asserts that the first row variable is a subtype of the second.
    ///
    /// The row "subtyping" here is quite restricted in Kailua;
    /// the subtyping relation is only checked for *types* of fields,
    /// and the list of fields in both variables should still be same (after necessary extension).
    fn assert_rvar_sub(&mut self, lhs: RVar, rhs: RVar) -> TypeResult<()>;

    /// Asserts that the first row variable is equal to the second.
    fn assert_rvar_eq(&mut self, lhs: RVar, rhs: RVar) -> TypeResult<()>;

    /// Asserts that the row variable contains given fields.
    /// If there is a matching field, the types should be equal to each other.
    fn assert_rvar_includes(&mut self, lhs: RVar, rhs: &[(Key, Slot)]) -> TypeResult<()>;

    /// Asserts that the row variable is no longer extensible.
    fn assert_rvar_closed(&mut self, rvar: RVar) -> TypeResult<()>;

    /// Iterates over a list of field keys and corresponding types.
    /// The closure can stop the iteration by returning `Err`.
    /// The iteration order is unspecified but all keys will be unique (guaranteed by type system).
    ///
    /// If the iteration ends normally, it will return a row variable corresponding to
    /// the extensible portion of given variable (this may be `RVar::any()` if not generated).
    /// For the inextensible records it will return `RVar::empty()`.
    /// This "last" row variable is purely for debugging and should not be used otherwise.
    fn list_rvar_fields(
        &self, rvar: RVar, f: &mut FnMut(&Key, &Slot) -> result::Result<(), ()>
    ) -> result::Result<RVar, ()>;

    /// Collects and returns a list of all known fields in the row variable.
    fn get_rvar_fields(&self, rvar: RVar) -> Vec<(Key, Slot)> {
        let mut fields = Vec::new();
        self.list_rvar_fields(rvar, &mut |k, v| {
            fields.push((k.clone(), v.clone()));
            Ok(())
        }).expect("list_rvar_fields exited early while we haven't break");
        fields // do not return the last rvar, to which operations are no-ops
    }

    /// Returns a type name for given nominal identifier, if any.
    fn get_class_name(&self, cid: ClassId) -> Option<&Name>;

    /// Returns true if given nominal instance type is a subtype of another nominal instance type.
    fn is_subclass_of(&self, lhs: ClassId, rhs: ClassId) -> bool;
}

/// Any types that can produce a union type, which is a supertype of two input types.
pub trait Union<Other = Self> {
    /// A type of the resulting type.
    type Output;

    /// Calculates a union type of `self` and `other`, explicitly or implicitly.
    ///
    /// Kailua distinguishes two kinds of union types, explicitly constructed or not.
    /// Explicitly constructed types are from the AST and should be retained as much as possible,
    /// with a good fact that types constructible from the AST are limited and simpler.
    /// `3 | 4` is one such example.
    ///
    /// Implicitly constructed types are used for `or` operations or implicit return types,
    /// and will use a much more coarse lattice than the explicit construction.
    /// `3 | 4` will result in `integer` in this mode.
    /// Because this is severely limited, the implicit union can only shrink the type's size.
    fn union(&self, other: &Other, explicit: bool,
             ctx: &mut TypeContext) -> TypeResult<Self::Output>;
}

/// Any types with subtyping or equivalence relations.
pub trait Lattice<Other = Self> {
    /// Asserts that `self` is a consistent subtype of `other` under the type context.
    fn assert_sub(&self, other: &Other, ctx: &mut TypeContext) -> TypeResult<()>;

    /// Asserts that `self` is a consistent type equal to `other` under the type context.
    fn assert_eq(&self, other: &Other, ctx: &mut TypeContext) -> TypeResult<()>;
}

impl<A: Union<B>, B> Union<Box<B>> for Box<A> {
    type Output = <A as Union<B>>::Output;

    fn union(&self, other: &Box<B>, explicit: bool,
             ctx: &mut TypeContext) -> TypeResult<Self::Output> {
        (**self).union(other, explicit, ctx)
    }
}

impl<A: Lattice<B>, B> Lattice<Box<B>> for Box<A> {
    fn assert_sub(&self, other: &Box<B>, ctx: &mut TypeContext) -> TypeResult<()> {
        (**self).assert_sub(other, ctx)
    }

    fn assert_eq(&self, other: &Box<B>, ctx: &mut TypeContext) -> TypeResult<()> {
        (**self).assert_eq(other, ctx)
    }
}

impl<A, B> Union<Spanned<B>> for Spanned<A> where A: Union<B> {
    type Output = <A as Union<B>>::Output;

    fn union(&self, other: &Spanned<B>, explicit: bool,
             ctx: &mut TypeContext) -> TypeResult<Self::Output> {
        self.base.union(&other.base, explicit, ctx).map_err(|r| {
            r.cannot_union_attach_span(self.span, other.span, explicit)
        })
    }
}

impl<A, B> Lattice<Spanned<B>> for Spanned<A> where A: Lattice<B> {
    fn assert_sub(&self, other: &Spanned<B>, ctx: &mut TypeContext) -> TypeResult<()> {
        self.base.assert_sub(&other.base, ctx).map_err(|r| {
            r.not_sub_attach_span(self.span, other.span)
        })
    }

    fn assert_eq(&self, other: &Spanned<B>, ctx: &mut TypeContext) -> TypeResult<()> {
        self.base.assert_eq(&other.base, ctx).map_err(|r| {
            r.not_eq_attach_span(self.span, other.span)
        })
    }
}

/// Any type that can have a dummy value for errors.
pub trait Dummy {
    /// Generates a dummy value.
    fn dummy() -> Self;
}

/// An implementation of `TypeContext` that raises an error for most methods.
///
/// Useful for ensuring that no operations involve type variables or row variables.
pub struct NoTypeContext;

impl TypeContext for NoTypeContext {
    fn gen_report(&self) -> TypeReport {
        TypeReport::new(Locale::dummy())
    }
    fn last_tvar(&self) -> Option<TVar> {
        None
    }
    fn gen_tvar(&mut self) -> TVar {
        panic!("gen_tvar is not supposed to be called here");
    }
    fn copy_tvar(&mut self, tvar: TVar) -> TVar {
        panic!("copy_tvar({:?}) is not supposed to be called here", tvar);
    }
    fn assert_tvar_sub(&mut self, lhs: TVar, rhs: &Ty) -> TypeResult<()> {
        panic!("assert_tvar_sub({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn assert_tvar_sup(&mut self, lhs: TVar, rhs: &Ty) -> TypeResult<()> {
        panic!("assert_tvar_sup({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn assert_tvar_eq(&mut self, lhs: TVar, rhs: &Ty) -> TypeResult<()> {
        panic!("assert_tvar_eq({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn assert_tvar_sub_tvar(&mut self, lhs: TVar, rhs: TVar) -> TypeResult<()> {
        panic!("assert_tvar_sub_tvar({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn assert_tvar_eq_tvar(&mut self, lhs: TVar, rhs: TVar) -> TypeResult<()> {
        panic!("assert_tvar_eq_tvar({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn get_tvar_bounds(&self, tvar: TVar) -> (flags::Flags /*lb*/, flags::Flags /*ub*/) {
        panic!("get_tvar_bounds({:?}) is not supposed to be called here", tvar);
    }
    fn get_tvar_exact_type(&self, tvar: TVar) -> Option<Ty> {
        panic!("get_tvar_exact_type({:?}) is not supposed to be called here", tvar);
    }

    fn gen_rvar(&mut self) -> RVar {
        panic!("gen_rvar is not supposed to be called here");
    }
    fn copy_rvar(&mut self, rvar: RVar) -> RVar {
        panic!("copy_rvar({:?}) is not supposed to be called here", rvar);
    }
    fn assert_rvar_sub(&mut self, lhs: RVar, rhs: RVar) -> TypeResult<()> {
        panic!("assert_rvar_sub({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn assert_rvar_eq(&mut self, lhs: RVar, rhs: RVar) -> TypeResult<()> {
        panic!("assert_rvar_eq({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn assert_rvar_includes(&mut self, lhs: RVar, rhs: &[(Key, Slot)]) -> TypeResult<()> {
        panic!("assert_rvar_includes({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
    fn assert_rvar_closed(&mut self, rvar: RVar) -> TypeResult<()> {
        panic!("assert_rvar_closed({:?}) is not supposed to be called here", rvar);
    }
    fn list_rvar_fields(
        &self, rvar: RVar, _f: &mut FnMut(&Key, &Slot) -> result::Result<(), ()>
    ) -> result::Result<RVar, ()> {
        panic!("list_rvar_fields({:?}, ...) is not supposed to be called here", rvar)
    }

    fn get_class_name(&self, cid: ClassId) -> Option<&Name> {
        panic!("get_class_name({:?}) is not supposed to be called here", cid);
    }
    fn is_subclass_of(&self, lhs: ClassId, rhs: ClassId) -> bool {
        panic!("is_subclass_of({:?}, {:?}) is not supposed to be called here", lhs, rhs);
    }
}

impl Lattice for TVar {
    fn assert_sub(&self, other: &Self, ctx: &mut TypeContext) -> TypeResult<()> {
        ctx.assert_tvar_sub_tvar(*self, *other)
    }

    fn assert_eq(&self, other: &Self, ctx: &mut TypeContext) -> TypeResult<()> {
        ctx.assert_tvar_eq_tvar(*self, *other)
    }
}

impl Lattice for RVar {
    fn assert_sub(&self, other: &Self, ctx: &mut TypeContext) -> TypeResult<()> {
        ctx.assert_rvar_sub(self.clone(), other.clone())
    }

    fn assert_eq(&self, other: &Self, ctx: &mut TypeContext) -> TypeResult<()> {
        ctx.assert_rvar_eq(self.clone(), other.clone())
    }
}

/// A compact description of the type.
pub mod flags {
    use ty::value::Dyn;

    bitflags! {
        /// Type flags, a compact description of the type.
        pub flags Flags: u16 {
            /// Empty flags.
            const T_NONE       = 0b0000_0000_0000,

            /// An explicit dynamic type (`WHATEVER`).
            const T_WHATEVER   = 0b0000_0000_0001,

            /// Any dynamic type (`WHATEVER` or the "oops" error type).
            const T_DYNAMIC    = 0b0000_0000_0011,

            /// A noisy nil (`nil?`).
            ///
            /// A silent nil (`nil`) is ignored.
            const T_NOISY_NIL  = 0b0000_0000_0100,

            /// `true`.
            const T_TRUE       = 0b0000_0000_1000,

            /// `false`.
            const T_FALSE      = 0b0000_0001_0000,

            /// `boolean` and its subtype.
            const T_BOOLEAN    = 0b0000_0001_1000,

            /// `number` minus `integer`.
            const T_NONINTEGER = 0b0000_0010_0000,

            /// `integer` and its subtype.
            const T_INTEGER    = 0b0000_0100_0000,

            /// `number` and its subtype.
            const T_NUMBER     = 0b0000_0110_0000,

            /// `string` and its subtype.
            const T_STRING     = 0b0000_1000_0000,

            /// `table` and its subtype.
            const T_TABLE      = 0b0001_0000_0000,

            /// `function` and its subtype.
            const T_FUNCTION   = 0b0010_0000_0000,

            /// `thread`.
            const T_THREAD     = 0b0100_0000_0000,

            /// `userdata`.
            const T_USERDATA   = 0b1000_0000_0000,

            /// All non-dynamic types.
            const T_ALL        = 0b1111_1111_1100,

            // combined flags follow.
            // XXX don't yet support metatables

            /// Any types that can be used as an index to arrays out of the box.
            const T_INTEGRAL = T_DYNAMIC.bits | T_INTEGER.bits,

            /// Any types that allow arithmetic operations out of the box.
            ///
            /// Technically Lua also allows operations like `"3" + 4`, but they are not desirable.
            const T_NUMERIC = T_DYNAMIC.bits | T_NUMBER.bits,

            /// Any types that can be concatenated out of the box.
            const T_STRINGY = T_DYNAMIC.bits | T_NUMBER.bits | T_STRING.bits,

            /// Any types that can be indexed out of the box.
            const T_TABULAR = T_DYNAMIC.bits | T_STRING.bits | T_TABLE.bits,

            /// Any types that can be called out of the box.
            const T_CALLABLE = T_DYNAMIC.bits | T_FUNCTION.bits,

            /// Any types that evaluate to `false` on the condition.
            const T_FALSY = T_NOISY_NIL.bits | T_FALSE.bits,

            /// Any types that evaluate to `true` on the condition.
            const T_TRUTHY = T_ALL.bits ^ T_FALSY.bits,
        }
    }

    impl Flags {
        pub fn is_dynamic(&self) -> bool { self.intersects(T_DYNAMIC) }

        pub fn is_integral(&self) -> bool {
            self.is_dynamic() || (self.intersects(T_INTEGRAL) && !self.intersects(!T_INTEGRAL))
        }

        pub fn is_numeric(&self) -> bool {
            self.is_dynamic() || (self.intersects(T_NUMERIC) && !self.intersects(!T_NUMERIC))
        }

        pub fn is_stringy(&self) -> bool {
            self.is_dynamic() || (self.intersects(T_STRINGY) && !self.intersects(!T_STRINGY))
        }

        pub fn is_tabular(&self) -> bool {
            self.is_dynamic() || (self.intersects(T_TABULAR) && !self.intersects(!T_TABULAR))
        }

        pub fn is_callable(&self) -> bool {
            self.is_dynamic() || (self.intersects(T_CALLABLE) && !self.intersects(!T_CALLABLE))
        }

        pub fn is_truthy(&self) -> bool {
            self.intersects(T_TRUTHY) && !self.intersects(!T_TRUTHY)
        }

        pub fn is_falsy(&self) -> bool {
            self.intersects(T_FALSY) && !self.intersects(!T_FALSY)
        }

        pub fn get_dynamic(&self) -> Option<Dyn> {
            if self.contains(T_DYNAMIC) {
                Some(Dyn::Oops)
            } else if self.contains(T_WHATEVER) {
                Some(Dyn::User)
            } else {
                None
            }
        }
    }

    bitflags! {
        /// A subset of `Flags` that can be `Union`ed with no additional processing.
        pub flags UnionedSimple: u16 {
            /// Empty flags.
            const U_NONE = T_NONE.bits,

            /// `true`.
            const U_TRUE = T_TRUE.bits,

            /// `false`.
            const U_FALSE = T_FALSE.bits,

            /// `boolean` and its subtype.
            const U_BOOLEAN = T_BOOLEAN.bits,

            /// `thread`.
            const U_THREAD = T_THREAD.bits,

            /// `userdata`.
            const U_USERDATA = T_USERDATA.bits,
        }
    }
}
