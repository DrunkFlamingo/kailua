use std::fmt;
use std::mem;
use std::ops::Deref;
use std::sync::Arc;
use parking_lot::{RwLock, RwLockReadGuard};

use kailua_env::Spanned;
use kailua_diag::Reporter;
use kailua_syntax::M;
use diag::CheckResult;
use super::{Dyn, Nil, T, Ty, TypeContext, Lattice, Union, Display, TVar, Tag};
use super::{error_not_sub, error_not_eq};
use super::flags::Flags;
use message as m;

// slot type flexibility (a superset of type mutability)
#[derive(Copy, Clone, PartialEq)]
pub enum F {
    // invalid top type (no type information available)
    // for the typing convenience the type information itself is retained, but never used.
    Any,
    // dynamic slot; all assignments are allowed and ignored
    Dynamic(Dyn),
    // temporary r-value slot
    // coerces to Var (default) or Const depending on the unification
    Just,
    // covariant immutable slot
    Const,
    // invariant mutable slot
    Var,
}

impl F {
    pub fn from(modf: M) -> F {
        match modf {
            M::None => F::Var,
            M::Const => F::Const,
        }
    }
}

impl<'a> fmt::Debug for F {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            F::Any                => write!(f, "any"),
            F::Dynamic(Dyn::User) => write!(f, "?"),
            F::Dynamic(Dyn::Oops) => write!(f, "?!"),
            F::Just               => write!(f, "just"),
            F::Const              => write!(f, "const"),
            F::Var                => write!(f, "var"),
        }
    }
}

// slot types
#[derive(Clone, PartialEq)]
pub struct S {
    flex: F,
    ty: Ty,
}

impl S {
    pub fn flex(&self) -> F {
        self.flex
    }

    pub fn unlift(&self) -> &Ty {
        &self.ty
    }

    pub fn with_nil(self) -> S {
        S { flex: self.flex, ty: self.ty.with_nil() }
    }

    pub fn without_nil(self) -> S {
        S { flex: self.flex, ty: self.ty.without_nil() }
    }

    // self and other may be possibly different slots and being merged by union
    // mainly used by `and`/`or` operators and table lifting
    pub fn union(&mut self, other: &mut S, explicit: bool,
                 ctx: &mut TypeContext) -> CheckResult<S> {
        let (flex, ty) = match (self.flex, other.flex) {
            (F::Dynamic(dyn1), F::Dynamic(dyn2)) => {
                let dyn = dyn1.union(dyn2);
                (F::Dynamic(dyn), Ty::new(T::Dynamic(dyn)))
            },
            (F::Dynamic(dyn), _) | (_, F::Dynamic(dyn)) =>
                (F::Dynamic(dyn), Ty::new(T::Dynamic(dyn))),
            (F::Any, _) | (_, F::Any) => (F::Any, Ty::new(T::None)),

            // it's fine to merge r-values
            (F::Just, F::Just) => (F::Just, self.ty.union(&other.ty, explicit, ctx)?),

            // merging Var requires a and b are identical
            // (the user is expected to annotate a and b if it's not the case)
            (F::Var, F::Var) |
            (F::Var, F::Just) |
            (F::Just, F::Var) => {
                self.ty.assert_eq(&other.ty, ctx)?;
                (F::Var, self.ty.clone())
            },

            // Var and Just are lifted to Const otherwise
            (F::Const, F::Just) |
            (F::Const, F::Const) |
            (F::Const, F::Var) |
            (F::Just, F::Const) |
            (F::Var, F::Const) =>
                (F::Const, self.ty.union(&other.ty, explicit, ctx)?),
        };

        Ok(S { flex: flex, ty: ty })
    }

    pub fn assert_sub(&self, other: &S, ctx: &mut TypeContext) -> CheckResult<()> {
        debug!("asserting a constraint {:?} <: {:?}", *self, *other);

        match (self.flex, other.flex) {
            (_, F::Any) => Ok(()),
            (_, F::Dynamic(_)) | (F::Dynamic(_), _) => Ok(()),

            (F::Just, _) | (_, F::Const) => self.ty.assert_sub(&other.ty, ctx),
            (F::Var, F::Var) => self.ty.assert_eq(&other.ty, ctx),

            (_, _) => error_not_sub(self, other),
        }
    }

    pub fn assert_eq(&self, other: &S, ctx: &mut TypeContext) -> CheckResult<()> {
        debug!("asserting a constraint {:?} = {:?}", *self, *other);

        match (self.flex, other.flex) {
            (F::Any, F::Any) => Ok(()),
            (_, F::Dynamic(_)) | (F::Dynamic(_), _) => Ok(()),

            (F::Just, F::Just) | (F::Const, F::Const) | (F::Var, F::Var) =>
                self.ty.assert_eq(&other.ty, ctx),

            (_, _) => error_not_eq(self, other),
        }
    }
}

impl fmt::Debug for S {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} {:?}", self.flex, self.ty)
    }
}

pub struct UnliftedSlot<'a>(RwLockReadGuard<'a, S>);

impl<'a> Deref for UnliftedSlot<'a> {
    type Target = Ty;
    fn deref(&self) -> &Ty { self.0.unlift() }
}

#[derive(Clone)]
pub struct Slot(Arc<RwLock<S>>);

impl Slot {
    pub fn new<'a>(flex: F, ty: Ty) -> Slot {
        Slot(Arc::new(RwLock::new(S { flex: flex, ty: ty })))
    }

    pub fn from(s: S) -> Slot {
        Slot(Arc::new(RwLock::new(s)))
    }

    pub fn just(t: Ty) -> Slot {
        Slot::new(F::Just, t)
    }

    pub fn var(t: Ty) -> Slot {
        Slot::new(F::Var, t)
    }

    pub fn dummy() -> Slot {
        Slot::new(F::Dynamic(Dyn::Oops), Ty::dummy())
    }

    pub fn unlift<'a>(&'a self) -> UnliftedSlot<'a> {
        UnliftedSlot(self.0.read())
    }

    // one tries to assign to `self` through parent with `flex`. how should `self` change?
    // (only makes sense when `self` is a Just slot, otherwise no-op)
    pub fn adapt(&self, flex: F, _ctx: &mut TypeContext) {
        let mut slot = self.0.write();
        if slot.flex == F::Just {
            slot.flex = match flex {
                F::Const | F::Var => flex,
                _ => F::Just,
            };
        }
    }

    // one tries to assign `rhs` to `self`. is it *accepted*, and if so how should they change?
    // if `init` is true, this is the first time assignment with lax requirements.
    pub fn accept(&self, rhs: &Slot, ctx: &mut TypeContext, init: bool) -> CheckResult<()> {
        // accepting itself is always fine and has no effect,
        // but it has to be filtered since it will borrow twice otherwise
        if self.0.deref() as *const _ == rhs.0.deref() as *const _ { return Ok(()); }

        debug!("accepting {:?} into {:?}{}", rhs, self, if init { " (initializing)" } else { "" });

        let mut lhs = self.0.write();
        let rhs = rhs.0.read();

        match (lhs.flex, rhs.flex, init) {
            (F::Dynamic(_), _, _) => Ok(()),

            (_, F::Any, _) |
            (F::Any, _, _) |
            (F::Const, _, false) => Err(format!("impossible to assign {:?} to {:?}", *rhs, *lhs)),

            (_, F::Dynamic(_), _) => Ok(()),

            // as long as the type is in agreement, Var can be assigned
            (F::Var, _, _) => rhs.ty.assert_sub(&lhs.ty, ctx),

            // assignment to Const slot is for initialization only
            (F::Const, _, true) => rhs.ty.assert_sub(&lhs.ty, ctx),

            // Just becomes Var when assignment happens
            (F::Just, _, _) => {
                rhs.ty.assert_sub(&lhs.ty, ctx)?;
                lhs.flex = F::Var;
                Ok(())
            }
        }
    }

    // one tries to modify `self` in place. the new value, when expressed as a type, is
    // NOT a subtype of the previous value, but is of the same kind (e.g. records, arrays etc).
    // is it *accepted*, and if so how should the slot change?
    //
    // conceptually this is same to `accept`, but some special types (notably nominal types)
    // have a representation that is hard to express with `accept` only.
    pub fn accept_in_place(&self, _ctx: &mut TypeContext) -> CheckResult<()> {
        let mut s = self.0.write();
        match s.flex {
            F::Dynamic(_) => Ok(()),
            F::Any | F::Const => Err(format!("impossible to update {:?}", *s)),
            F::Var => Ok(()),
            F::Just => {
                s.flex = F::Var;
                Ok(())
            }
        }
    }

    pub fn filter_by_flags(&self, flags: Flags, ctx: &mut TypeContext) -> CheckResult<()> {
        // when filter_by_flags fails, the slot itself has no valid type
        let mut s = self.0.write();
        let t = mem::replace(&mut s.ty, Ty::new(T::None).or_nil(Nil::Absent));
        s.ty = t.filter_by_flags(flags, ctx)?;
        Ok(())
    }

    // following methods are direct analogues to value type's ones, whenever applicable

    pub fn flex(&self) -> F { self.0.read().flex() }
    pub fn flags(&self) -> Flags { self.unlift().flags() }

    pub fn is_integral(&self) -> bool { self.flags().is_integral() }
    pub fn is_numeric(&self)  -> bool { self.flags().is_numeric() }
    pub fn is_stringy(&self)  -> bool { self.flags().is_stringy() }
    pub fn is_tabular(&self)  -> bool { self.flags().is_tabular() }
    pub fn is_callable(&self) -> bool { self.flags().is_callable() }
    pub fn is_truthy(&self)   -> bool { self.flags().is_truthy() }
    pub fn is_falsy(&self)    -> bool { self.flags().is_falsy() }

    pub fn get_dynamic(&self) -> Option<Dyn> { self.flags().get_dynamic() }
    pub fn get_tvar(&self) -> Option<TVar> { self.unlift().get_tvar() }
    pub fn tag(&self) -> Option<Tag> { self.unlift().tag() }

    pub fn with_nil(&self) -> Slot {
        let s = self.0.read();
        Slot::from(s.clone().with_nil())
    }

    pub fn without_nil(&self) -> Slot {
        let s = self.0.read();
        Slot::from(s.clone().without_nil())
    }
}

impl Union for Slot {
    type Output = Slot;

    fn union(&self, other: &Slot, explicit: bool, ctx: &mut TypeContext) -> CheckResult<Slot> {
        // if self and other point to the same slot, do not try to borrow mutably
        if self.0.deref() as *const _ == other.0.deref() as *const _ { return Ok(self.clone()); }

        // now it is safe to borrow mutably
        Ok(Slot::from(self.0.write().union(&mut other.0.write(), explicit, ctx)?))
    }
}

impl Lattice for Slot {
    fn assert_sub(&self, other: &Slot, ctx: &mut TypeContext) -> CheckResult<()> {
        self.0.read().assert_sub(&other.0.read(), ctx)
    }

    fn assert_eq(&self, other: &Slot, ctx: &mut TypeContext) -> CheckResult<()> {
        self.0.read().assert_eq(&other.0.read(), ctx)
    }
}

// when the RHS is a normal type the LHS is automatically unlifted; useful for rvalue-only ops.
// note that this makes a big difference from lifting the RHS.
// also, for the convenience, this does NOT print the LHS with the flexibility
// (which doesn't matter here).
impl<'a> Lattice<T<'a>> for Spanned<Slot> {
    fn assert_sub(&self, other: &T<'a>, ctx: &mut TypeContext) -> CheckResult<()> {
        let unlifted = self.unlift();
        if let Err(e) = unlifted.assert_sub(other, ctx) {
            ctx.error(self.span, m::NotSubtype { sub: unlifted.display(ctx),
                                                 sup: other.display(ctx) })
               .done()?;
            Err(e) // XXX not sure if we can recover here
        } else {
            Ok(())
        }
    }

    fn assert_eq(&self, other: &T<'a>, ctx: &mut TypeContext) -> CheckResult<()> {
        let unlifted = self.unlift();
        if let Err(e) = unlifted.assert_eq(other, ctx) {
            ctx.error(self.span, m::NotEqual { lhs: unlifted.display(ctx),
                                               rhs: other.display(ctx) })
               .done()?;
            Err(e) // XXX not sure if we can recover here
        } else {
            Ok(())
        }
    }
}

impl<'a> Lattice<Ty> for Spanned<Slot> {
    fn assert_sub(&self, other: &Ty, ctx: &mut TypeContext) -> CheckResult<()> {
        let unlifted = self.unlift();
        if let Err(e) = unlifted.assert_sub(other, ctx) {
            ctx.error(self.span, m::NotSubtype { sub: unlifted.display(ctx),
                                                 sup: other.display(ctx) })
               .done()?;
            Err(e) // XXX not sure if we can recover here
        } else {
            Ok(())
        }
    }

    fn assert_eq(&self, other: &Ty, ctx: &mut TypeContext) -> CheckResult<()> {
        let unlifted = self.unlift();
        if let Err(e) = unlifted.assert_eq(other, ctx) {
            ctx.error(self.span, m::NotEqual { lhs: unlifted.display(ctx),
                                               rhs: other.display(ctx) })
               .done()?;
            Err(e) // XXX not sure if we can recover here
        } else {
            Ok(())
        }
    }
}

impl PartialEq for Slot {
    fn eq(&self, other: &Slot) -> bool {
        *self.0.read() == *other.0.read()
    }
}

impl Display for Slot {
    fn fmt_displayed(&self, f: &mut fmt::Formatter, ctx: &TypeContext) -> fmt::Result {
        let s = &*self.0.read();

        let prefix = match s.flex {
            F::Any                => return write!(f, "<inaccessible type>"),
            F::Dynamic(Dyn::User) => return write!(f, "WHATEVER"),
            F::Dynamic(Dyn::Oops) => return write!(f, "<error type>"),
            F::Just               => "", // can be seen as a mutable value yet to be assigned
            F::Const              => "const ",
            F::Var                => "",
        };

        write!(f, "{}{}", prefix, s.ty.display(ctx))
    }
}

impl fmt::Debug for Slot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.sign_minus() {
            fmt::Debug::fmt(&self.0.read().ty, f)
        } else {
            fmt::Debug::fmt(&*self.0.read(), f)
        }
    }
}

#[cfg(test)]
#[allow(unused_variables, dead_code)]
mod tests {
    use ty::{T, Ty, Lattice, NoTypeContext};
    use super::*;

    #[test]
    fn test_sub() {
        let just = |t| Slot::new(F::Just, Ty::new(t));
        let cnst = |t| Slot::new(F::Const, Ty::new(t));
        let var = |t| Slot::new(F::Var, Ty::new(t));

        assert_eq!(just(T::Integer).assert_sub(&just(T::Integer), &mut NoTypeContext), Ok(()));
        assert_eq!(just(T::Integer).assert_sub(&just(T::Number), &mut NoTypeContext), Ok(()));
        assert!(just(T::Number).assert_sub(&just(T::Integer), &mut NoTypeContext).is_err());

        assert_eq!(var(T::Integer).assert_sub(&var(T::Integer), &mut NoTypeContext), Ok(()));
        assert!(var(T::Integer).assert_sub(&var(T::Number), &mut NoTypeContext).is_err());
        assert!(var(T::Number).assert_sub(&var(T::Integer), &mut NoTypeContext).is_err());

        assert_eq!(cnst(T::Integer).assert_sub(&cnst(T::Integer), &mut NoTypeContext), Ok(()));
        assert_eq!(cnst(T::Integer).assert_sub(&cnst(T::Number), &mut NoTypeContext), Ok(()));
        assert!(cnst(T::Number).assert_sub(&cnst(T::Integer), &mut NoTypeContext).is_err());
    }
}

